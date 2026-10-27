//! Development tool for reaper-bandcontrol.

mod sim;

use std::{net::SocketAddr, path::PathBuf, time::Duration};

use anyhow::Context as _;
use bandcontrol_proto::{
    crypto,
    pairing::{PairingInfo, PairingSecret},
    random::OsRandom,
};
use bandcontrol_server::{
    Server, ServerOptions, TransportStatus,
    client::{Mirror, TcpClient},
    helper::HelperOptions,
};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(about, version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Args, Clone)]
struct SecretArgs {
    /// Pairing code. Defaults to the code stored in `--secret-file` (created if missing).
    #[arg(long, env = "BANDCONTROL_SECRET")]
    secret: Option<String>,
    #[arg(long, default_value = "~/.config/bandcontrol/sim-secret")]
    secret_file: String,
}

impl SecretArgs {
    fn load(&self) -> anyhow::Result<PairingSecret> {
        if let Some(code) = &self.secret {
            return Ok(PairingSecret::from_code(code)?);
        }
        let path = expand_home(&self.secret_file);
        if let Ok(code) = std::fs::read_to_string(&path) {
            return Ok(PairingSecret::from_code(code.trim())?);
        }
        let secret = PairingSecret::generate(&mut OsRandom);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, secret.to_code()).with_context(|| format!("writing {}", path.display()))?;
        Ok(secret)
    }
}

fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(path),
    }
}

#[derive(Subcommand)]
enum Command {
    /// Run a simulated REAPER (fake project) serving phones over BLE and/or TCP.
    Sim(sim::SimArgs),
    /// Connect to a server over TCP and print what it sends.
    Client {
        #[command(flatten)]
        secret: SecretArgs,
        #[arg(long, default_value = "127.0.0.1:7878")]
        tcp: SocketAddr,
        /// Packet size announced to the server, small values exercise fragmentation.
        #[arg(long, default_value_t = 512)]
        max_packet: usize,
        /// Exit after receiving the initial state.
        #[arg(long)]
        once: bool,
    },
    /// Print the pairing URL and QR code.
    Pair {
        #[command(flatten)]
        secret: SecretArgs,
        #[arg(long, default_value = "Simulator")]
        name: String,
    },
    /// macOS: connect to a running BandControl app like the REAPER extension and wait until it reports Bluetooth
    /// (on, or why not). Fails when the app doesn't answer.
    AppCheck {
        /// The app's socket; defaults to the one the extension uses.
        #[arg(long)]
        socket: Option<PathBuf>,
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
    },
}

/// Talks to the BandControl app through the same server the extension uses.
fn app_check(socket: Option<PathBuf>, timeout: Duration) -> anyhow::Result<()> {
    let socket = socket
        .or_else(bandcontrol_server::helper::default_socket)
        .context("no socket path")?;
    let server = Server::start(ServerOptions {
        secret: PairingSecret::generate(&mut OsRandom),
        name: "app-check".into(),
        ble: true,
        tcp: None,
        helper: Some(HelperOptions { socket, launch: false }),
    })?;
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        match server.status().ble {
            TransportStatus::Running(service) => {
                tracing::info!(service, "the app runs Bluetooth");
                return Ok(());
            }
            // Before the app answered, the transport fails with "isn't running" or "quit".
            TransportStatus::Failed(e) if !e.contains("BandControl app") => {
                tracing::info!("the app answered, Bluetooth is unavailable: {e}");
                return Ok(());
            }
            _ => std::thread::sleep(Duration::from_millis(100)),
        }
    }
    anyhow::bail!("no answer from the BandControl app: {:?}", server.status().ble)
}

fn print_pairing(secret: &PairingSecret, name: &str) {
    let info = PairingInfo {
        secret: secret.clone(),
        name: name.to_owned(),
    };
    let url = info.to_url();
    if let Ok(code) = qrcode::QrCode::new(url.as_bytes()) {
        let qr = code
            .render::<qrcode::render::unicode::Dense1x2>()
            .quiet_zone(true)
            .build();
        tracing::info!("pairing QR code:\n{qr}");
    }
    tracing::info!(
        url,
        code = secret.to_display_code(),
        service_uuid = %uuid::Uuid::from_bytes(crypto::service_uuid(secret)),
        "pairing"
    );
}

async fn client(secret: &PairingSecret, addr: SocketAddr, max_packet: usize, once: bool) -> anyhow::Result<()> {
    let mut client = TcpClient::connect(addr, secret, max_packet).await?;
    tracing::info!(session = client.session_id, server = client.server_name, "connected");
    let mut mirror = Mirror::default();
    loop {
        let msg = client.next_timeout(Duration::from_secs(30)).await?;
        tracing::info!("{msg:?}");
        mirror.apply(&msg);
        if once && mirror.state.is_some() && mirror.transport.is_some() {
            return Ok(());
        }
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,bluer=warn".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    match Cli::parse().command {
        Command::Sim(args) => sim::run(&args),
        Command::Client {
            secret,
            tcp,
            max_packet,
            once,
        } => {
            let secret = secret.load()?;
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(client(&secret, tcp, max_packet, once))
        }
        Command::Pair { secret, name } => {
            print_pairing(&secret.load()?, &name);
            Ok(())
        }
        Command::AppCheck { socket, timeout_secs } => app_check(socket, Duration::from_secs(timeout_secs)),
    }
}
