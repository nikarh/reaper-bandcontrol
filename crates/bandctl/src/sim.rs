//! Simulated REAPER: fake project + real server + engine ticking at 30 Hz, with optional scripted activity and a
//! line based control port for tests (`dump`, `volume 13 0.5`, `play`, `meter 7 -3`, `reset band`, `offline`, ...).

use std::{
    io::{BufRead as _, BufReader, Write as _},
    net::{SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use bandcontrol_core::{
    daw::{Command, Daw as _},
    engine::{Engine, EngineConfig},
    fake::FakeDaw,
    model::{Marker, PlayState},
    rpp,
};
use bandcontrol_server::{Server, ServerOptions};
use parking_lot::Mutex;
use serde_json::json;

use crate::SecretArgs;

#[derive(clap::Args)]
pub struct SimArgs {
    #[command(flatten)]
    secret: SecretArgs,
    /// Server name shown on phones.
    #[arg(long, default_value = "Simulator")]
    name: String,
    /// Serve over BLE.
    #[arg(long)]
    ble: bool,
    /// Serve over TCP on this address.
    #[arg(long)]
    tcp: Option<SocketAddr>,
    /// Line based control port to manipulate the fake project.
    #[arg(long)]
    control: Option<SocketAddr>,
    /// Simulate activity: playback, fader moves, mutes and renames.
    #[arg(long)]
    activity: bool,
    /// REAPER project to simulate instead of the Mother Project: a file, `band` for the bundled band layout or
    /// `example` for the downloadable example project.
    #[arg(long)]
    project: Option<PathBuf>,
}

/// A project shaped like a real band's: input, IEM output and PA (front of house) output folders.
const BAND_PROJECT: &str = include_str!("../../../tests/fixtures/Band Project.RPP");
/// The smallest project laid out the way the app expects, offered for download.
const EXAMPLE_PROJECT: &str = include_str!("../../../examples/BandControl-Example.RPP");

fn load(project: Option<&str>) -> anyhow::Result<FakeDaw> {
    Ok(match project {
        None | Some("mother") => FakeDaw::mother_project(),
        Some("band") => rpp::parse(BAND_PROJECT)?,
        Some("example") => rpp::parse(EXAMPLE_PROJECT)?,
        Some(path) => rpp::parse(&std::fs::read_to_string(path)?)?,
    })
}

pub fn run(args: &SimArgs) -> anyhow::Result<()> {
    let secret = args.secret.load()?;
    crate::print_pairing(&secret, &args.name);

    let options = || ServerOptions {
        secret: secret.clone(),
        name: args.name.clone(),
        ble: args.ble,
        tcp: args.tcp,
    };
    let mut server = Some(Server::start(options())?);
    // Cleared by the `offline` control command to stop serving, as if the computer left.
    let online = Arc::new(AtomicBool::new(true));
    let project = args.project.as_ref().map(|p| p.to_string_lossy().into_owned());
    let daw = Arc::new(Mutex::new(load(project.as_deref())?));

    if let Some(addr) = args.control {
        let listener = TcpListener::bind(addr)?;
        tracing::info!(%addr, "control port listening");
        let daw = daw.clone();
        let online = online.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let daw = daw.clone();
                let online = online.clone();
                let project = project.clone();
                std::thread::spawn(move || control_connection(stream, &daw, &online, project.as_deref()));
            }
        });
    }

    let mut engine = Engine::new(EngineConfig::default());
    let started = Instant::now();
    let mut last = started;
    let mut last_status = None;
    let mut step = 0u64;
    loop {
        let now = Instant::now();
        let serving = online.load(Ordering::Relaxed);
        if serving != server.is_some() {
            tracing::info!(serving, "switching the server");
            engine = Engine::new(EngineConfig::default());
            server = if serving { Some(Server::start(options())?) } else { None };
        }
        let Some(server) = server.as_mut() else {
            std::thread::sleep(Duration::from_millis(33));
            continue;
        };
        let outputs = {
            let mut daw = daw.lock();
            daw.advance(now - last);
            if args.activity {
                let due = (now - started).as_secs() / 2;
                while step < due {
                    step += 1;
                    activity(&mut daw, step);
                }
            }
            engine.tick(&mut *daw, now, server.poll_inputs())
        };
        last = now;
        server.send(outputs);

        let status = server.status();
        if last_status.as_ref() != Some(&status) {
            tracing::info!(?status, "server status");
            last_status = Some(status);
        }
        std::thread::sleep(Duration::from_millis(33));
    }
}

/// Moves faders and mutes of the first output tracks (hardware output and receives), renames a track now and then
/// and keeps playback running, whatever the project.
fn activity(daw: &mut FakeDaw, step: u64) {
    let wave = [0.2f32, 0.5, 0.8, 1.0, 0.6, 0.3];
    let level = wave
        .get(usize::try_from(step).unwrap_or(0) % wave.len())
        .copied()
        .unwrap_or(1.0);
    let outputs: Vec<u32> = daw
        .tracks
        .iter()
        .filter(|t| t.hw_out && !t.receives.is_empty())
        .map(|t| t.id)
        .collect();
    let first = outputs.first().copied().unwrap_or(0);
    let second = outputs.get(1).copied().unwrap_or(first);
    match step % 5 {
        0 => {
            if let Some(t) = daw.track_mut(first) {
                t.volume = level;
            }
        }
        1 => {
            if let Some(r) = daw.track_mut(second).and_then(|t| t.receives.get_mut(1)) {
                r.mute = !r.mute;
            }
        }
        2 => {
            if let Some(r) = daw.track_mut(first).and_then(|t| t.receives.get_mut(0)) {
                r.volume = level;
            }
        }
        3 if step % 15 == 3 => {
            if let Some(t) = daw.tracks.get_mut(1) {
                t.name = match t.name.strip_suffix(" *") {
                    Some(name) => name.to_owned(),
                    None => format!("{} *", t.name),
                };
            }
            daw.touch();
        }
        _ => {
            if daw.transport.play != PlayState::Playing
                && let Some(region) = daw.regions.first().map(|r| r.id)
            {
                let _ = daw.apply(&Command::Goto { region, marker: None });
                let _ = daw.apply(&Command::Play);
            }
        }
    }
}

fn control_connection(stream: TcpStream, daw: &Mutex<FakeDaw>, online: &AtomicBool, project: Option<&str>) {
    let Ok(mut writer) = stream.try_clone() else { return };
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { return };
        let line = line.trim();
        let reply = match line {
            "offline" | "online" => {
                online.store(line == "online", Ordering::Relaxed);
                Ok("ok".into())
            }
            _ => control(line, &mut daw.lock(), project),
        };
        let reply = match reply {
            Ok(reply) => reply,
            Err(e) => format!("error {e}"),
        };
        if writeln!(writer, "{reply}").is_err() {
            return;
        }
    }
}

fn arg<T: std::str::FromStr>(parts: &[&str], i: usize) -> anyhow::Result<T> {
    parts
        .get(i)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow::anyhow!("missing or invalid argument {i}"))
}

fn control(line: &str, daw: &mut FakeDaw, project: Option<&str>) -> anyhow::Result<String> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    let command = parts.first().copied().unwrap_or_default();
    match command {
        "dump" => return Ok(dump(daw).to_string()),
        // `reset` reloads the simulated project, `reset band` / `reset mother` / `reset <file>` switch projects.
        "reset" => daw.replace_project(load(parts.get(1).copied().or(project))?),
        // `meter <track> <dBFS>` fixes a track's meter, `meter <track> off` returns to generated levels.
        "meter" => {
            let id: u32 = arg(&parts, 1)?;
            if parts.get(2) == Some(&"off") {
                daw.meter_overrides.remove(&id);
            } else {
                let db: f32 = arg(&parts, 2)?;
                daw.meter_overrides.insert(id, 10f32.powf(db / 20.0));
            }
        }
        "play" => daw.apply(&Command::Play)?,
        "pause" => daw.apply(&Command::Pause)?,
        "stop" => daw.apply(&Command::Stop)?,
        "seek" => daw.transport.position = arg(&parts, 1)?,
        "volume" => {
            let id: u32 = arg(&parts, 1)?;
            daw.track_mut(id)
                .ok_or_else(|| anyhow::anyhow!("no track {id}"))?
                .volume = arg(&parts, 2)?;
        }
        "receive" => {
            let id: u32 = arg(&parts, 1)?;
            let index: usize = arg(&parts, 2)?;
            let r = daw
                .track_mut(id)
                .and_then(|t| t.receives.get_mut(index))
                .ok_or_else(|| anyhow::anyhow!("no receive {id}/{index}"))?;
            r.volume = arg(&parts, 3)?;
            r.mute = arg::<u8>(&parts, 4)? != 0;
        }
        "rename" => {
            let id: u32 = arg(&parts, 1)?;
            daw.track_mut(id).ok_or_else(|| anyhow::anyhow!("no track {id}"))?.name =
                parts.get(2..).unwrap_or_default().join(" ");
            daw.touch();
        }
        "add-marker" => {
            daw.markers.push(Marker {
                id: arg(&parts, 1)?,
                pos: arg(&parts, 2)?,
                name: parts.get(3..).unwrap_or_default().join(" "),
                color: None,
            });
            daw.touch();
        }
        other => anyhow::bail!("unknown command {other:?}"),
    }
    Ok("ok".into())
}

fn dump(daw: &FakeDaw) -> serde_json::Value {
    json!({
        "transport": {
            "play": format!("{:?}", daw.transport.play),
            "position": daw.transport.position,
            "repeat": daw.transport.repeat,
            "recording": daw.transport.recording,
        },
        "time_selection": daw.time_selection,
        "region_order": bandcontrol_core::model::format_region_order(&daw.region_order),
        "tracks": daw.tracks.iter().map(|t| json!({
            "id": t.id,
            "name": t.name,
            "volume": t.volume,
            "receives": t.receives.iter().map(|r| json!({"src": r.src, "volume": r.volume, "mute": r.mute})).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}
