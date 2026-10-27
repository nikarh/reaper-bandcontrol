//! REAPER extension: serves phones over Bluetooth LE (and optionally TCP for tests).
//!
//! Environment overrides (mostly for automated tests):
//! * `BANDCONTROL_SECRET` — pairing code to use instead of the stored one
//! * `BANDCONTROL_TCP` — also listen on this TCP address
//! * `BANDCONTROL_BLE=0` — disable Bluetooth
//! * `BANDCONTROL_NAME` — server name shown on phones
//!
//! On macOS Bluetooth goes through the BandControl app (`bandcontrol_server::helper`): REAPER's `Info.plist` doesn't
//! declare Bluetooth use, so macOS doesn't let REAPER use it.

mod daw;
mod ffi;
mod pairing;
mod version;
mod window;

use std::{
    cell::RefCell,
    ffi::{CStr, c_int, c_void},
    fs::File,
    net::SocketAddr,
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use bandcontrol_core::engine::{Engine, EngineConfig};
use bandcontrol_proto::{
    pairing::{PairingInfo, PairingSecret},
    random::OsRandom,
};
use bandcontrol_server::{
    Server, ServerOptions, TransportStatus,
    helper::{HelperOptions, Request},
};
use daw::ReaperDaw;
use ffi::{PluginInfo, Reaper};
use parking_lot::Mutex;

const EXT_SECTION: &CStr = c"bandcontrol";
const EXT_SECRET: &CStr = c"secret";
const EXT_NAME: &CStr = c"name";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    ShowPairing,
    ResetKey,
    Rename,
    Status,
}

#[derive(Debug)]
struct Plugin {
    reaper: &'static Reaper,
    daw: ReaperDaw,
    engine: Engine,
    server: Option<Server>,
    secret: PairingSecret,
    name: String,
    ble: bool,
    tcp: Option<SocketAddr>,
    actions: Vec<(i32, Action)>,
    window_checked: Instant,
}

thread_local! {
    static PLUGIN: RefCell<Option<Plugin>> = const { RefCell::new(None) };
}

impl Plugin {
    fn new(reaper: &'static Reaper) -> Self {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let secret = env("BANDCONTROL_SECRET")
            .and_then(|c| PairingSecret::from_code(&c).ok())
            .or_else(|| {
                reaper
                    .ext_state(EXT_SECTION, EXT_SECRET)
                    .and_then(|c| PairingSecret::from_code(&c).ok())
            })
            .unwrap_or_else(|| {
                let secret = PairingSecret::generate(&mut OsRandom);
                reaper.set_ext_state(EXT_SECTION, EXT_SECRET, &secret.to_code());
                secret
            });

        let actions = [
            (
                "BANDCONTROL_SHOW_PAIRING",
                "BandControl: Show pairing QR code",
                Action::ShowPairing,
            ),
            (
                "BANDCONTROL_RESET_KEY",
                "BandControl: Reset pairing key (unpairs all phones)",
                Action::ResetKey,
            ),
            (
                "BANDCONTROL_RENAME",
                "BandControl: Set the name shown on phones",
                Action::Rename,
            ),
            ("BANDCONTROL_STATUS", "BandControl: Show status", Action::Status),
        ]
        .into_iter()
        .map(|(id, name, action)| (reaper.register_action(id, name), action))
        .filter(|(cmd, _)| *cmd != 0)
        .collect();

        let mut plugin = Self {
            reaper,
            daw: ReaperDaw::new(reaper),
            engine: Engine::new(EngineConfig::default()),
            server: None,
            secret,
            name: env("BANDCONTROL_NAME")
                .or_else(|| reaper.ext_state(EXT_SECTION, EXT_NAME).filter(|n| !n.trim().is_empty()))
                .unwrap_or_else(pairing::hostname),
            ble: env("BANDCONTROL_BLE").is_none_or(|v| v != "0"),
            tcp: env("BANDCONTROL_TCP").and_then(|a| a.parse().ok()),
            actions,
            window_checked: Instant::now(),
        };
        plugin.start_server();
        plugin
    }

    fn start_server(&mut self) {
        // Stop the old server first so the BLE registration is released.
        self.server = None;
        self.engine = Engine::new(EngineConfig::default());
        self.server = match Server::start(ServerOptions {
            secret: self.secret.clone(),
            name: self.name.clone(),
            ble: self.ble,
            tcp: self.tcp,
            helper: helper_options(),
        }) {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::error!("failed to start server: {e}");
                self.reaper.console(&format!("BandControl: failed to start: {e}\n"));
                None
            }
        };
    }

    fn tick(&mut self) {
        if window::is_open() && self.window_checked.elapsed() > WINDOW_STATUS_INTERVAL {
            self.window_checked = Instant::now();
            window::set_status(self.window_status());
            window::set_scaling(self.window_scaling());
        }
        let Some(server) = &mut self.server else { return };
        let requests = server.poll_requests();
        if !requests.is_empty() {
            for request in requests {
                match request {
                    Request::Rename(name) => self.rename(&name),
                    Request::ResetKey => self.reset_key(),
                }
            }
            return;
        }
        let inputs = server.poll_inputs();
        if inputs.is_empty() && self.engine.sessions() == 0 {
            return;
        }
        let outputs = self.engine.tick(&mut self.daw, Instant::now(), inputs);
        server.send(outputs);
    }

    fn pairing_info(&self) -> PairingInfo {
        PairingInfo {
            secret: self.secret.clone(),
            name: self.name.clone(),
        }
    }

    fn window_view(&self) -> window::View {
        window::View::new(&self.pairing_info(), self.window_status())
    }

    fn window_scaling(&self) -> window::Scaling {
        window::Scaling::new(
            self.reaper.config_f64(c"uiscale"),
            self.reaper.config_f64(c"fontscaling"),
        )
    }

    fn window_status(&self) -> window::Status {
        let status = |level, text: String| window::Status { text, level };
        let Some(server) = &self.server else {
            return status(window::Level::Error, "Not running, see bandcontrol.log".into());
        };
        let s = server.status();
        let phones = match s.sessions {
            0 => "no phones connected".to_owned(),
            1 => "1 phone connected".to_owned(),
            n => format!("{n} phones connected"),
        };
        match (&s.ble, &s.tcp) {
            (TransportStatus::Running(_), _) => status(window::Level::Ok, format!("Bluetooth on · {phones}")),
            (TransportStatus::Starting, _) => status(window::Level::Pending, "Bluetooth is starting…".into()),
            // The details are in the status action and the log.
            (TransportStatus::Failed(_), _) => status(
                window::Level::Error,
                "Bluetooth isn't working, see BandControl: Show status".into(),
            ),
            // TCP is only used by tests and screenshots.
            (TransportStatus::Disabled, TransportStatus::Running(_)) => {
                status(window::Level::Ok, format!("Ready · {phones}"))
            }
            (TransportStatus::Disabled, _) => status(window::Level::Error, "Bluetooth is disabled".into()),
        }
    }

    fn status_text(&self) -> String {
        let describe = |s: &TransportStatus| match s {
            TransportStatus::Disabled => "disabled".to_owned(),
            TransportStatus::Starting => "starting".to_owned(),
            TransportStatus::Running(detail) => format!("running ({detail})"),
            TransportStatus::Failed(e) => format!("FAILED: {e}"),
        };
        match &self.server {
            None => "Server is not running, see bandcontrol.log in the REAPER resource folder.".into(),
            Some(server) => {
                let status = server.status();
                let via = if helper_options().is_some() {
                    " (through the BandControl app)"
                } else {
                    ""
                };
                format!(
                    "Name shown on phones: {}\nBluetooth{via}: {}\nTCP: {}\nConnected phones: {}\nREAPER: {} (supported: {} and \
                     newer)\n\n\
                     The pairing code and the name are stored in this REAPER's settings (resource folder: {}), \
                     so they are the same for every project.",
                    self.name,
                    describe(&status.ble),
                    describe(&status.tcp),
                    status.sessions,
                    self.reaper.app_version(),
                    version::min_reaper(),
                    self.reaper.resource_path(),
                )
            }
        }
    }

    fn rename(&mut self, name: &str) {
        let name = name.trim();
        if name.is_empty() || name == self.name {
            return;
        }
        name.clone_into(&mut self.name);
        self.reaper.set_ext_state(EXT_SECTION, EXT_NAME, name);
        // Phones reconnect and get the new name in the handshake.
        self.start_server();
    }

    fn reset_key(&mut self) {
        self.secret = PairingSecret::generate(&mut OsRandom);
        self.reaper
            .set_ext_state(EXT_SECTION, EXT_SECRET, &self.secret.to_code());
        self.start_server();
    }
}

/// macOS: Bluetooth through the BandControl app, started when it isn't running.
#[cfg(target_os = "macos")]
fn helper_options() -> Option<HelperOptions> {
    bandcontrol_server::helper::default_socket().map(|socket| HelperOptions { socket, launch: true })
}

#[cfg(not(target_os = "macos"))]
fn helper_options() -> Option<HelperOptions> {
    None
}

const WINDOW_STATUS_INTERVAL: Duration = Duration::from_millis(500);

/// A button in the pairing window was clicked.
fn window_button(button: window::Button) {
    match button {
        window::Button::Rename => run_action(Action::Rename),
        window::Button::Reset => run_action(Action::ResetKey),
    }
}

fn with_plugin<T>(f: impl FnOnce(&mut Plugin) -> T) -> Option<T> {
    PLUGIN.with(|p| {
        // Modal dialogs run nested event loops which may re-enter callbacks: skip instead of panicking.
        let mut guard = p.try_borrow_mut().ok()?;
        guard.as_mut().map(f)
    })
}

fn run_action(action: Action) {
    let Some(reaper) = Reaper::get() else { return };
    match action {
        Action::ShowPairing => {
            // macOS: the BandControl app shows it.
            if with_plugin(|p| p.server.as_ref().is_some_and(Server::show_pairing)) == Some(true) {
                return;
            }
            let Some((view, scaling, info, resource)) = with_plugin(|p| {
                (
                    p.window_view(),
                    p.window_scaling(),
                    p.pairing_info(),
                    p.reaper.resource_path(),
                )
            }) else {
                return;
            };
            if window::show(view, scaling, reaper.main_window()) {
                return;
            }
            // No native window (no SWELL): a page in the browser instead.
            let path = PathBuf::from(resource).join("bandcontrol-pairing.html");
            let opened = std::fs::write(&path, pairing::page(&info)).and_then(|()| pairing::open(&path));
            let mut msg = format!(
                "Scan the QR code in the browser window with the phone camera, or enter this code in the app:\n\n{}",
                info.secret.to_display_code()
            );
            if let Err(e) = opened {
                msg = format!("{msg}\n\n(Could not open {}: {e})", path.display());
            }
            reaper.message_box(&msg, "BandControl pairing", 0);
        }
        Action::ResetKey => {
            let answer = reaper.message_box(
                "Generate a new pairing key? All paired phones will have to be paired again.",
                "BandControl",
                4,
            );
            if answer == 6 {
                with_plugin(Plugin::reset_key);
                run_action(Action::ShowPairing);
            }
        }
        Action::Rename => {
            let Some(current) = with_plugin(|p| p.name.clone()) else {
                return;
            };
            let caption = "Name shown on phones:,extrawidth=150";
            if let Some(name) = reaper.user_input("BandControl", caption, &current)
                && let Some(view) = with_plugin(|p| {
                    p.rename(&name);
                    p.window_view()
                })
            {
                window::update(view);
            }
        }
        Action::Status => {
            if let Some(text) = with_plugin(|p| p.status_text()) {
                reaper.message_box(&text, "BandControl status", 0);
            }
        }
    }
}

extern "C" fn timer() {
    let _ = catch_unwind(|| with_plugin(Plugin::tick));
}

extern "C" fn hook_command(
    _section: *mut c_void,
    command: c_int,
    _val: c_int,
    _valhw: c_int,
    _relmode: c_int,
    _hwnd: *mut c_void,
) -> bool {
    catch_unwind(|| {
        let action = with_plugin(|p| p.actions.iter().find(|(c, _)| *c == command).map(|(_, a)| *a)).flatten();
        action.is_some_and(|a| {
            run_action(a);
            true
        })
    })
    .unwrap_or(false)
}

struct LogWriter(Arc<Mutex<File>>);

impl std::io::Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.lock().flush()
    }
}

fn init_logging(reaper: &Reaper) {
    let path = PathBuf::from(reaper.resource_path()).join("bandcontrol.log");
    let Ok(file) = File::create(&path) else { return };
    let file = Arc::new(Mutex::new(file));
    let filter =
        tracing_subscriber::EnvFilter::try_from_env("BANDCONTROL_LOG").unwrap_or_else(|_| "info,bluer=warn".into());
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(move || LogWriter(file.clone()))
        .try_init();
}

fn load(reaper: &'static Reaper) -> Result<(), String> {
    init_logging(reaper);
    let app = reaper.app_version();
    tracing::info!(version = bandcontrol_server::VERSION, reaper = app, "loading");
    if version::older_than_supported(&app) == Some(true) {
        let msg = format!(
            "BandControl supports REAPER {} and newer, this is REAPER {app}: some things may not work.",
            version::min_reaper()
        );
        tracing::warn!("{msg}");
        reaper.console(&format!("{msg}\n"));
    }

    let plugin = catch_unwind(AssertUnwindSafe(|| Plugin::new(reaper))).map_err(|_| "panic during startup")?;
    PLUGIN.with(|p| *p.borrow_mut() = Some(plugin));
    reaper.register_fn("hookcommand2", hook_command as *const c_void);
    reaper.register_fn("timer", timer as *const c_void);
    Ok(())
}

fn unload() {
    if let Some(reaper) = Reaper::get() {
        reaper.register_fn("-timer", timer as *const c_void);
        reaper.register_fn("-hookcommand2", hook_command as *const c_void);
    }
    window::close();
    let plugin = PLUGIN.with(|p| p.borrow_mut().take());
    drop(plugin);
    tracing::info!("unloaded");
}

/// REAPER plugin entry point.
///
/// # Safety
/// Must only be called by REAPER, with the arguments documented in `reaper_plugin.h`.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ReaperPluginEntry(_instance: *mut c_void, info: *const PluginInfo) -> c_int {
    if info.is_null() {
        let _ = catch_unwind(unload);
        return 0;
    }
    // SAFETY: `info` is the pointer REAPER passed to the plugin entry.
    match unsafe { ffi::init(info) }.and_then(load) {
        Ok(()) => 1,
        Err(e) => {
            if let Some(reaper) = Reaper::get() {
                reaper.console(&format!(
                    "BandControl failed to load ({e}). It needs REAPER {} or newer.\n",
                    version::min_reaper()
                ));
            }
            0
        }
    }
}
