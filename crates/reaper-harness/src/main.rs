//! Runs an isolated REAPER with the extension until stdin is closed or `q` is entered.
//!
//! Lines other than `q` are sent to the REAPER web interface and the response is printed, e.g.
//! `SET/TRACK/13/VOL/0.5;TRACK/13`.
//!
//! Usage: `reaper-harness [--release] [--project FILE]`; `BANDCONTROL_*` variables are passed to REAPER.

use std::{
    io::{BufRead as _, Write as _},
    path::PathBuf,
    time::Duration,
};

use reaper_harness::{Options, Reaper, build_plugin, mother_project, workspace_root};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut release = false;
    let mut project = mother_project();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--release" => release = true,
            "--project" => project = PathBuf::from(args.next().ok_or("--project needs a value")?),
            other => return Err(format!("unknown argument {other}").into()),
        }
    }

    let mut reaper = Reaper::launch(&Options {
        dir: workspace_root().join("target/reaper-harness"),
        project,
        plugin: build_plugin(release)?,
        env: std::env::vars()
            .filter(|(k, _)| k.starts_with("BANDCONTROL_"))
            .collect(),
    })?;
    reaper.wait_ready(Duration::from_secs(60))?;
    let mut out = std::io::stdout().lock();
    writeln!(out, "ready, resource dir {}", reaper.dir().display())?;

    for line in std::io::stdin().lock().lines() {
        let line = line?;
        match line.trim() {
            "q" => break,
            "" => {}
            commands => match reaper.web(commands) {
                Ok(response) => write!(out, "{response}")?,
                Err(e) => writeln!(out, "error: {e}")?,
            },
        }
    }
    Ok(())
}
