//! Runs an isolated REAPER with the extension until stdin is closed or `q` is entered.
//!
//! `screenshot FILE.png` saves a screenshot (headless only), `screenshot-window FILE.png TITLE` one of a window (Xvfb
//! only, `TITLE` is a regular expression), `sleep MS` waits, other lines except `q` are sent to the REAPER web
//! interface and the response is printed, e.g. `SET/TRACK/13/VOL/0.5;TRACK/13` or `_BANDCONTROL_SHOW_PAIRING`.
//!
//! Usage: `reaper-harness [--release] [--project FILE] [--web-port PORT] [--dir DIR] [--ini KEY=VALUE]...` (default
//! project: the band fixture, default directory: `target/reaper-harness`, wiped on start; `--ini` adds REAPER
//! preferences, e.g. `fontscaling=1.5`);
//! `BANDCONTROL_*` variables are passed to REAPER.

use std::{
    io::{BufRead as _, Write as _},
    path::PathBuf,
    time::Duration,
};

use reaper_harness::{Options, Reaper, band_project, build_plugin, workspace_root};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut release = false;
    let mut project = band_project();
    let mut web_port = None;
    let mut ini = Vec::new();
    let mut dir = workspace_root().join("target/reaper-harness");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--release" => release = true,
            "--project" => project = PathBuf::from(args.next().ok_or("--project needs a value")?),
            "--ini" => ini.push(args.next().ok_or("--ini needs a value")?),
            "--web-port" => web_port = Some(args.next().ok_or("--web-port needs a value")?.parse()?),
            // Absolute: it ends up in REAPER's -cfgfile and the compositor's runtime directory.
            "--dir" => dir = std::env::current_dir()?.join(args.next().ok_or("--dir needs a value")?),
            other => return Err(format!("unknown argument {other}").into()),
        }
    }

    let mut reaper = Reaper::launch(&Options {
        dir,
        project,
        plugin: build_plugin(release)?,
        env: std::env::vars()
            .filter(|(k, _)| k.starts_with("BANDCONTROL_"))
            .collect(),
        web_port,
        ini,
    })?;
    reaper.wait_ready(Duration::from_secs(60))?;
    let mut out = std::io::stdout().lock();
    writeln!(out, "ready, resource dir {}", reaper.dir().display())?;

    for line in std::io::stdin().lock().lines() {
        let line = line?;
        match line.trim() {
            "q" => break,
            "" => {}
            command if command.starts_with("sleep ") => {
                let ms = command.trim_start_matches("sleep ").trim().parse().unwrap_or(0);
                std::thread::sleep(Duration::from_millis(ms));
            }
            command if command.starts_with("screenshot-window ") => {
                let rest = command.trim_start_matches("screenshot-window ").trim();
                let (path, title) = rest.split_once(' ').unwrap_or((rest, ""));
                match reaper.screenshot_window(title, path.as_ref()) {
                    Ok(()) => writeln!(out, "saved {path}")?,
                    Err(e) => writeln!(out, "error: {e}")?,
                }
            }
            command if command.starts_with("screenshot ") => {
                let path = PathBuf::from(command.trim_start_matches("screenshot ").trim());
                match reaper.screenshot(&path) {
                    Ok(()) => writeln!(out, "saved {}", path.display())?,
                    Err(e) => writeln!(out, "error: {e}")?,
                }
            }
            commands => match reaper.web(commands) {
                Ok(response) => write!(out, "{response}")?,
                Err(e) => writeln!(out, "error: {e}")?,
            },
        }
    }
    Ok(())
}
