//! Simulated REAPER: fake project + real server + engine ticking at 30 Hz, with optional scripted activity and a
//! line based control port for tests (`dump`, `volume 13 0.5`, `play`, ...).

use std::{
    io::{BufRead as _, BufReader, Write as _},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::Arc,
    time::{Duration, Instant},
};

use bandcontrol_core::{
    daw::{Command, Daw as _},
    engine::{Engine, EngineConfig},
    fake::FakeDaw,
    model::{Marker, PlayState},
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
    /// Serve over BLE (Linux only for now).
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
}

pub fn run(args: &SimArgs) -> anyhow::Result<()> {
    let secret = args.secret.load()?;
    crate::print_pairing(&secret, &args.name);

    let mut server = Server::start(ServerOptions {
        secret,
        name: args.name.clone(),
        ble: args.ble,
        tcp: args.tcp,
    })?;
    let daw = Arc::new(Mutex::new(FakeDaw::mother_project()));

    if let Some(addr) = args.control {
        let listener = TcpListener::bind(addr)?;
        tracing::info!(%addr, "control port listening");
        let daw = daw.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let daw = daw.clone();
                std::thread::spawn(move || control_connection(stream, &daw));
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

fn activity(daw: &mut FakeDaw, step: u64) {
    let wave = [0.2f32, 0.5, 0.8, 1.0, 0.6, 0.3];
    let level = wave
        .get(usize::try_from(step).unwrap_or(0) % wave.len())
        .copied()
        .unwrap_or(1.0);
    match step % 5 {
        0 => {
            if let Some(t) = daw.track_mut(13) {
                t.volume = level;
            }
        }
        1 => {
            if let Some(r) = daw.track_mut(15).and_then(|t| t.receives.get_mut(1)) {
                r.mute = !r.mute;
            }
        }
        2 => {
            if let Some(r) = daw.track_mut(14).and_then(|t| t.receives.get_mut(0)) {
                r.volume = level;
            }
        }
        3 if step % 15 == 3 => {
            if let Some(t) = daw.track_mut(2) {
                t.name = if t.name == "Click" {
                    "Click*".into()
                } else {
                    "Click".into()
                };
            }
            daw.touch();
        }
        _ => {
            if daw.transport.play != PlayState::Playing {
                let _ = daw.apply(&Command::Goto {
                    region: 1,
                    marker: None,
                });
                let _ = daw.apply(&Command::Play);
            }
        }
    }
}

fn control_connection(stream: TcpStream, daw: &Mutex<FakeDaw>) {
    let Ok(mut writer) = stream.try_clone() else { return };
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { return };
        let reply = match control(line.trim(), &mut daw.lock()) {
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

fn control(line: &str, daw: &mut FakeDaw) -> anyhow::Result<String> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    let command = parts.first().copied().unwrap_or_default();
    match command {
        "dump" => return Ok(dump(daw).to_string()),
        "reset" => *daw = FakeDaw::mother_project(),
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
