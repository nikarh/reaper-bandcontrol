//! Runs an isolated REAPER instance with the bandcontrol extension installed.
//!
//! Every instance gets a fresh resource directory (`-cfgfile`), so the user's REAPER configuration is never
//! touched. On Linux REAPER runs headless (a `cage` compositor, or Xvfb where cage isn't available, like CI) with
//! the dummy audio device, so playback advances without a sound card or a display. The REAPER web interface is
//! enabled on a free port so tests can act like the sound engineer (change volumes, add markers...).
//!
//! Environment:
//! * `REAPER_BIN` — REAPER executable (default: common install locations)
//! * `REAPER_LICENSE` — license file to copy (default: the one from the user's REAPER configuration, empty for none)
//! * `REAPER_HEADLESS` — `cage`, `xvfb`, or `0` to show the REAPER window (default: cage if installed, else xvfb)

use std::{
    io::{Read as _, Write as _},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("REAPER executable not found, set REAPER_BIN")]
    ReaperNotFound,
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("building the extension failed")]
    Build,
    #[error("timed out waiting for {0}")]
    Timeout(String),
    #[error("REAPER exited early with {0}, see {1}")]
    Exited(std::process::ExitStatus, PathBuf),
    #[error("unexpected web interface response: {0}")]
    Web(String),
    #[error("screenshots need a headless REAPER")]
    NotHeadless,
    #[error("the screenshot tool failed with {0}")]
    Screenshot(std::process::ExitStatus),
    #[error("unreadable screenshot")]
    Image,
    #[error("finding and resizing windows needs REAPER under Xvfb (xdotool)")]
    NoWindowControl,
}

/// An 8 bit grayscale image.
#[derive(Debug, Clone)]
pub struct Gray {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

impl Gray {
    /// Parses a binary PPM (`P6`, 8 bit), as written by `grim -t ppm` and `import`.
    fn from_ppm(data: &[u8]) -> Option<Self> {
        let mut fields = Vec::new();
        let mut pos = 0;
        while fields.len() < 4 {
            while data.get(pos).is_some_and(u8::is_ascii_whitespace) {
                pos += 1;
            }
            if data.get(pos) == Some(&b'#') {
                while data.get(pos).is_some_and(|b| *b != b'\n') {
                    pos += 1;
                }
                continue;
            }
            let start = pos;
            while data.get(pos).is_some_and(|b| !b.is_ascii_whitespace()) {
                pos += 1;
            }
            fields.push(std::str::from_utf8(data.get(start..pos)?).ok()?.to_owned());
        }
        let [magic, width, height, max] = fields.as_slice() else {
            return None;
        };
        let (width, height): (usize, usize) = (width.parse().ok()?, height.parse().ok()?);
        if magic != "P6" || max != "255" {
            return None;
        }
        let rgb = data.get(pos + 1..pos + 1 + width * height * 3)?;
        let pixels = rgb
            .as_chunks::<3>()
            .0
            .iter()
            .map(|[r, g, b]| {
                u8::try_from((u32::from(*r) * 299 + u32::from(*g) * 587 + u32::from(*b) * 114) / 1000)
                    .unwrap_or(u8::MAX)
            })
            .collect();
        Some(Self { width, height, pixels })
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[must_use]
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

#[must_use]
pub fn mother_project() -> PathBuf {
    workspace_root().join("tests/fixtures/Mother Project.RPP")
}

/// A real band's layout (inputs, IEM mixes, PA outputs) with invented songs, see `scripts/make-band-fixture.py`.
#[must_use]
pub fn band_project() -> PathBuf {
    workspace_root().join("tests/fixtures/Band Project.RPP")
}

#[must_use]
pub fn reaper_binary() -> Option<PathBuf> {
    if let Some(bin) = std::env::var_os("REAPER_BIN") {
        return Some(bin.into());
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    [
        PathBuf::from("/usr/lib/REAPER/reaper"),
        PathBuf::from("/opt/REAPER/reaper"),
        home.join("opt/REAPER/reaper"),
        PathBuf::from("/Applications/REAPER.app/Contents/MacOS/REAPER"),
        PathBuf::from(r"C:\Program Files\REAPER (x64)\reaper.exe"),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

fn license() -> Option<PathBuf> {
    // An empty REAPER_LICENSE runs unlicensed (evaluation), like CI.
    if let Some(path) = std::env::var_os("REAPER_LICENSE") {
        return (!path.is_empty()).then(|| path.into());
    }
    let config = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(|d| PathBuf::from(d).join("REAPER"))
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support/REAPER"))
    } else {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/REAPER"))
    };
    config.map(|c| c.join("reaper-license.rk")).filter(|p| p.is_file())
}

/// Builds the extension with cargo and returns the path of the shared library.
///
/// # Errors
/// When cargo fails.
pub fn build_plugin(release: bool) -> Result<PathBuf> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(cargo);
    command
        .current_dir(workspace_root())
        .args(["build", "-p", "reaper-bandcontrol", "--lib"]);
    if release {
        command.arg("--release");
    }
    if !command.status()?.success() {
        return Err(Error::Build);
    }
    let target = std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| workspace_root().join("target"), PathBuf::from);
    let file = format!(
        "{}reaper_bandcontrol{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    );
    Ok(target.join(if release { "release" } else { "debug" }).join(file))
}

/// Returns a TCP port that was free a moment ago.
///
/// # Errors
/// When no local port can be bound.
pub fn free_port() -> Result<u16> {
    Ok(TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?.local_addr()?.port())
}

/// Polls until something accepts TCP connections on `addr`.
///
/// # Errors
/// On timeout.
pub fn wait_for_port(addr: SocketAddr, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    while TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_err() {
        if Instant::now() > deadline {
            return Err(Error::Timeout(addr.to_string()));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Options {
    /// Resource directory, wiped on launch.
    pub dir: PathBuf,
    /// Project to open, copied into `dir` so it is never modified.
    pub project: PathBuf,
    /// Extension shared library to install.
    pub plugin: PathBuf,
    /// Extra environment for REAPER (the extension reads `BANDCONTROL_*`).
    pub env: Vec<(String, String)>,
    /// Web interface port, a free one when `None`.
    pub web_port: Option<u16>,
    /// Extra `reaper.ini` preferences (`key=value` lines of the `[REAPER]` section), e.g. `fontscaling=1.5`.
    pub ini: Vec<String>,
}

/// A running REAPER instance, killed on drop.
#[derive(Debug)]
pub struct Reaper {
    child: Child,
    dir: PathBuf,
    web: SocketAddr,
    display: Display,
    licensed: bool,
}

/// Where the REAPER window goes.
#[derive(Debug)]
enum Display {
    Windowed,
    /// `cage` with its own runtime directory, so screenshots can only ever capture it.
    Cage {
        runtime: PathBuf,
    },
    /// A private X server, killed with the instance.
    Xvfb {
        server: Child,
        display: String,
    },
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|d| d.join(program).is_file()))
}

impl Display {
    fn start(dir: &Path) -> Result<Self> {
        let choice = std::env::var("REAPER_HEADLESS").unwrap_or_default();
        if !cfg!(target_os = "linux") || choice == "0" {
            return Ok(Self::Windowed);
        }
        if choice == "cage" || (choice != "xvfb" && on_path("cage")) {
            let runtime = dir.join("runtime");
            std::fs::create_dir_all(&runtime)?;
            #[cfg(unix)]
            std::fs::set_permissions(&runtime, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
            return Ok(Self::Cage { runtime });
        }
        let mut server = Command::new("Xvfb")
            .args(["-displayfd", "1", "-screen", "0", "1920x1080x24", "-nolisten", "tcp"])
            .stdout(Stdio::piped())
            .stderr(std::fs::File::create(dir.join("xvfb.log"))?)
            .spawn()?;
        let mut number = String::new();
        if let Some(stdout) = server.stdout.take() {
            std::io::BufRead::read_line(&mut std::io::BufReader::new(stdout), &mut number)?;
        }
        let number = number.trim();
        if number.is_empty() {
            let _ = server.kill();
            return Err(Error::Timeout("Xvfb display".into()));
        }
        Ok(Self::Xvfb {
            server,
            display: format!(":{number}"),
        })
    }
}

impl Reaper {
    /// Prepares a fresh resource directory and starts REAPER.
    ///
    /// # Errors
    /// When REAPER is not installed or the directory can't be prepared.
    pub fn launch(options: &Options) -> Result<Self> {
        let binary = reaper_binary().ok_or(Error::ReaperNotFound)?;
        let dir = &options.dir;
        if dir.exists() {
            std::fs::remove_dir_all(dir)?;
        }
        std::fs::create_dir_all(dir.join("UserPlugins"))?;
        let plugin_name = format!("reaper_bandcontrol{}", std::env::consts::DLL_SUFFIX);
        std::fs::copy(&options.plugin, dir.join("UserPlugins").join(plugin_name))?;
        let license = license();
        if let Some(license) = &license {
            std::fs::copy(license, dir.join("reaper-license.rk"))?;
        }
        let project = dir.join(options.project.file_name().unwrap_or("project.RPP".as_ref()));
        std::fs::copy(&options.project, &project)?;

        let web_port = options.web_port.map_or_else(free_port, Ok)?;
        let ini = dir.join("reaper.ini");
        let preferences = options.ini.iter().fold(reaper_ini(web_port), |mut all, line| {
            all.push_str(line);
            all.push('\n');
            all
        });
        std::fs::write(&ini, preferences)?;

        let display = Display::start(dir)?;
        let mut command = match &display {
            Display::Cage { runtime } => {
                let mut c = Command::new("cage");
                c.envs([
                    ("WLR_BACKENDS", "headless"),
                    ("WLR_LIBINPUT_NO_DEVICES", "1"),
                    ("WLR_RENDERER", "pixman"),
                ])
                .env("XDG_RUNTIME_DIR", runtime)
                .env_remove("WAYLAND_DISPLAY")
                .env_remove("DISPLAY")
                .arg("--")
                .arg(&binary);
                c
            }
            Display::Xvfb { display, .. } => {
                // REAPER keeps its instance pipe in the runtime directory; parallel instances must not share it.
                let runtime = dir.join("runtime");
                std::fs::create_dir_all(&runtime)?;
                #[cfg(unix)]
                std::fs::set_permissions(&runtime, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
                let mut c = Command::new(&binary);
                c.env("DISPLAY", display)
                    .env("GDK_BACKEND", "x11")
                    .env("XDG_RUNTIME_DIR", runtime)
                    .env_remove("WAYLAND_DISPLAY");
                c
            }
            Display::Windowed => Command::new(&binary),
        };
        command
            .args(["-newinst", "-cfgfile"])
            .arg(&ini)
            .arg(&project)
            .envs(options.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .stdout(std::fs::File::create(dir.join("stdout.log"))?)
            .stderr(std::fs::File::create(dir.join("stderr.log"))?);
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);

        Ok(Self {
            child: command.spawn()?,
            dir: dir.clone(),
            web: SocketAddr::from((Ipv4Addr::LOCALHOST, web_port)),
            display,
            licensed: license.is_some(),
        })
    }

    /// Waits until REAPER has loaded the project and the web interface is up.
    ///
    /// # Errors
    /// On timeout or when REAPER exits.
    pub fn wait_ready(&mut self, timeout: Duration) -> Result<()> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.child.try_wait()? {
                return Err(Error::Exited(status, self.dir.clone()));
            }
            if TcpStream::connect_timeout(&self.web, Duration::from_millis(200)).is_ok() {
                self.close_evaluation_reminder();
                return Ok(());
            }
            if Instant::now() > deadline {
                return Err(Error::Timeout(format!("REAPER web interface on {}", self.web)));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Unlicensed REAPER shows a modal "About" reminder that stops REAPER's timers (and so the extension). Under Xvfb
    /// nothing closes it, so it is closed here; it can take a while to appear when several instances start at once.
    fn close_evaluation_reminder(&self) {
        let Display::Xvfb { display, .. } = &self.display else {
            return;
        };
        if self.licensed {
            return;
        }
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            let closed = Command::new("xdotool")
                .env("DISPLAY", display)
                .args(["search", "--onlyvisible", "--name", "^About REAPER", "windowclose"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            if closed {
                return;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Contents of the extension log.
    #[must_use]
    pub fn log(&self) -> String {
        std::fs::read_to_string(self.dir.join("bandcontrol.log")).unwrap_or_default()
    }

    /// Runs web interface commands (`/_/` API, e.g. `SET/TRACK/13/VOL/0.5;TRACK/13`) and returns the response.
    ///
    /// # Errors
    /// On connection errors or a non-200 response.
    pub fn web(&self, commands: &str) -> Result<String> {
        let mut stream = TcpStream::connect_timeout(&self.web, Duration::from_secs(5))?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let path = commands.replace(' ', "%20");
        write!(
            stream,
            "GET /_/{path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
        )?;
        let mut response = String::new();
        stream.read_to_string(&mut response)?;
        let (head, body) = response
            .split_once("\r\n\r\n")
            .ok_or_else(|| Error::Web(response.clone()))?;
        if !head.starts_with("HTTP/1.1 200") && !head.starts_with("HTTP/1.0 200") {
            return Err(Error::Web(head.to_owned()));
        }
        Ok(body.to_owned())
    }

    /// Saves a PNG of the headless REAPER window (`grim` under cage, `import` from `ImageMagick` under Xvfb).
    ///
    /// # Errors
    /// When REAPER is not headless or the screenshot tool fails.
    pub fn screenshot(&self, path: &Path) -> Result<()> {
        let status = match &self.display {
            Display::Cage { runtime } => {
                let socket = std::fs::read_dir(runtime)?
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .find(|n| n.starts_with("wayland-") && Path::new(n).extension().is_none())
                    .ok_or(Error::NotHeadless)?;
                let ppm = path.extension().is_some_and(|e| e == "ppm");
                Command::new("grim")
                    .env("XDG_RUNTIME_DIR", runtime)
                    .env("WAYLAND_DISPLAY", socket)
                    .args(if ppm { &["-t", "ppm"][..] } else { &[] })
                    .arg(path)
                    .status()?
            }
            Display::Xvfb { display, .. } => Command::new("import")
                .args(["-display", display, "-window", "root", "-depth", "8"])
                .arg(path)
                .status()?,
            Display::Windowed => return Err(Error::NotHeadless),
        };
        if status.success() {
            Ok(())
        } else {
            Err(Error::Screenshot(status))
        }
    }

    /// A grayscale screenshot of the headless REAPER screen.
    ///
    /// # Errors
    /// See [`Self::screenshot`].
    pub fn screenshot_gray(&self) -> Result<Gray> {
        let path = self.dir.join("screenshot.ppm");
        self.screenshot(&path)?;
        Gray::from_ppm(&std::fs::read(&path)?).ok_or(Error::Image)
    }

    /// Resizes the window whose title matches `title` (a regular expression) to `width` x `height`.
    ///
    /// # Errors
    /// When REAPER doesn't run under Xvfb, or the window isn't found.
    pub fn resize_window(&self, title: &str, width: u32, height: u32) -> Result<()> {
        let Display::Xvfb { display, .. } = &self.display else {
            return Err(Error::NoWindowControl);
        };
        let status = Command::new("xdotool")
            .env("DISPLAY", display)
            .args([
                "search",
                "--sync",
                "--onlyvisible",
                "--name",
                title,
                "windowsize",
                "--sync",
                "%1",
            ])
            .args([width.to_string(), height.to_string()])
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(Error::Timeout(format!("window {title:?}")))
        }
    }

    /// Saves a PNG of the window whose title matches `title` (a regular expression).
    ///
    /// # Errors
    /// When REAPER doesn't run under Xvfb, or the window isn't found.
    pub fn screenshot_window(&self, title: &str, path: &Path) -> Result<()> {
        let Display::Xvfb { display, .. } = &self.display else {
            return Err(Error::NoWindowControl);
        };
        let found = Command::new("xdotool")
            .env("DISPLAY", display)
            .args(["search", "--onlyvisible", "--name", title])
            .output()?;
        let id = String::from_utf8_lossy(&found.stdout)
            .lines()
            .next()
            .unwrap_or_default()
            .to_owned();
        if id.is_empty() {
            return Err(Error::Timeout(format!("window {title:?}")));
        }
        let status = Command::new("import")
            .args(["-display", display, "-window", &id])
            .arg(path)
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(Error::Screenshot(status))
        }
    }

    /// Whether [`Self::resize_window`] works here.
    #[must_use]
    pub fn can_resize_windows(&self) -> bool {
        matches!(self.display, Display::Xvfb { .. })
    }

    /// Runs a REAPER action by command id.
    ///
    /// # Errors
    /// See [`Self::web`].
    pub fn action(&self, id: u32) -> Result<()> {
        self.web(&id.to_string()).map(drop)
    }
}

impl Drop for Reaper {
    fn drop(&mut self) {
        // Everything started for this instance shares the process group created at launch. Not `kill -KILL -PID`:
        // procps' kill turns that into kill(-1), which kills every process of the user.
        #[cfg(unix)]
        if let Ok(pid) = i32::try_from(self.child.id()) {
            let _ = nix::sys::signal::killpg(nix::unistd::Pid::from_raw(pid), nix::sys::signal::Signal::SIGKILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Display::Xvfb { server, .. } = &mut self.display {
            let _ = server.kill();
            let _ = server.wait();
        }
    }
}

fn reaper_ini(web_port: u16) -> String {
    format!(
        "[REAPER]\n\
         splash=0\n\
         splash2=0\n\
         verchk=0\n\
         linux_audio_mode=2\n\
         autosaveint=0\n\
         csurf_cnt=1\n\
         csurf_0=HTTP 0 {web_port} '' '' 0 ''\n\
         wnd_x=0\n\
         wnd_y=0\n\
         wnd_w=1920\n\
         wnd_h=1080\n"
    )
}
