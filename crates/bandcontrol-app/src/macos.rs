//! The macOS user interface: a menu bar item and the pairing window, in `AppKit`.
//!
//! `AppKit` runs on the main thread; the relay runs on its own thread with tokio and posts [`UiUpdate`]s to the main
//! queue. The user interface lives in a thread local of the main thread and sends [`Action`]s back.

#![allow(unsafe_code)]

use std::{cell::RefCell, fs::File, path::PathBuf, process::ExitCode, sync::Arc};

use bandcontrol_server::helper::{self, Phone};
use block2::RcBlock;
use dispatch2::DispatchQueue;
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, Bool, NSObject, NSObjectProtocol, ProtocolObject, Sel},
    sel,
};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSAlertStyle, NSApplication, NSApplicationActivationPolicy,
    NSApplicationDelegate, NSBackingStoreType, NSBezierPath, NSBorderType, NSBox, NSBoxType, NSButton, NSColor,
    NSControlSize, NSFont, NSFontWeight, NSFontWeightMedium, NSFontWeightSemibold, NSImage, NSImageView,
    NSLayoutAttribute, NSLayoutConstraintOrientation, NSLineBreakMode, NSLineCapStyle, NSMenu, NSMenuItem,
    NSScrollView, NSStackView, NSStackViewDistribution, NSStatusBar, NSStatusItem, NSTextAlignment, NSTextField,
    NSUserInterfaceLayoutOrientation, NSVariableStatusItemLength, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSArray, NSEdgeInsets, NSPoint, NSRect, NSSize, NSString, ns_string};
use parking_lot::Mutex;
use tokio::sync::mpsc;

use crate::{
    relay::{self, Action, Relay, Snapshot, UiUpdate},
    view::{self, Level},
};

pub fn main() -> ExitCode {
    init_logging();
    let Some(mtm) = MainThreadMarker::new() else {
        return ExitCode::FAILURE;
    };
    let Some(socket) = helper::default_socket() else {
        tracing::error!("no home folder for the socket");
        return ExitCode::FAILURE;
    };
    tracing::info!(version = env!("CARGO_PKG_VERSION"), socket = %socket.display(), "starting");

    let app = NSApplication::sharedApplication(mtm);
    // A menu bar app: no Dock icon, no main menu.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let (actions, actions_rx) = mpsc::unbounded_channel();
    let ui = Ui::new(mtm, actions);
    app.setDelegate(Some(ProtocolObject::from_ref(&*ui.target)));
    UI.with(|u| *u.borrow_mut() = Some(ui));

    let spawned = std::thread::Builder::new()
        .name("relay".into())
        .spawn(move || serve(socket, actions_rx));
    if let Err(e) = spawned {
        tracing::error!("starting the relay thread failed: {e}");
        return ExitCode::FAILURE;
    }
    app.run();
    ExitCode::SUCCESS
}

fn init_logging() {
    let Some(home) = std::env::var_os("HOME") else { return };
    let path = PathBuf::from(home).join("Library/Logs/BandControl.log");
    let Ok(file) = File::create(path) else { return };
    let file = Arc::new(Mutex::new(file));
    let filter = tracing_subscriber::EnvFilter::try_from_env("BANDCONTROL_LOG").unwrap_or_else(|_| "info".into());
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(move || LogWriter(file.clone()))
        .try_init();
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

/// The relay thread.
fn serve(socket: PathBuf, actions: mpsc::UnboundedReceiver<Action>) {
    let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("starting tokio failed: {e}");
            post(UiUpdate::Quit);
            return;
        }
    };
    runtime.block_on(async move {
        let listener = match relay::listen(&socket).await {
            Ok(l) => l,
            Err(e) => {
                // Launch Services doesn't start a second instance, so this is one started some other way.
                tracing::error!("listening on {} failed: {e}", socket.display());
                post(UiUpdate::Quit);
                return;
            }
        };
        let start: relay::StartBluetooth =
            Box::new(|config, events, commands| Box::pin(bandcontrol_server::ble::start(config, events, commands)));
        Relay::new(relay::Options::default(), start, Box::new(post))
            .run(listener, actions)
            .await;
        let _ = std::fs::remove_file(&socket);
    });
}

/// Hands an update to the main thread.
fn post(update: UiUpdate) {
    DispatchQueue::main().exec_async(move || apply(update));
}

fn apply(update: UiUpdate) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let busy = UI.with(|ui| {
        let Ok(mut ui) = ui.try_borrow_mut() else { return true };
        if let Some(ui) = ui.as_mut() {
            ui.update(mtm, update.clone());
        }
        false
    });
    // A modal dialog is open and holds the interface: try again after it.
    if busy {
        post(update);
    }
}

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
}

/// Runs `f` on the user interface, unless it is busy (a nested event loop of a modal dialog).
fn with_ui<T>(f: impl FnOnce(&mut Ui) -> T) -> Option<T> {
    UI.with(|ui| ui.try_borrow_mut().ok()?.as_mut().map(f))
}

define_class!(
    /// Receives clicks from the menu and the window, and is the application delegate.
    // SAFETY: NSObject has no subclassing requirements and `Target` does not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BandControlTarget"]
    struct Target;

    // SAFETY: NSObjectProtocol has no requirements.
    unsafe impl NSObjectProtocol for Target {}

    // SAFETY: The signature matches `NSApplicationDelegate`.
    unsafe impl NSApplicationDelegate for Target {
        /// Opening the app again (from Finder or Spotlight) shows the window.
        #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
        fn should_handle_reopen(&self, _app: &NSApplication, _visible: bool) -> bool {
            show_window(self.mtm());
            true
        }
    }

    impl Target {
        #[unsafe(method(showPairing:))]
        fn show_pairing(&self, _sender: Option<&AnyObject>) {
            show_window(self.mtm());
        }

        #[unsafe(method(rename:))]
        fn rename(&self, _sender: Option<&AnyObject>) {
            rename(self.mtm());
        }

        #[unsafe(method(resetKey:))]
        fn reset_key(&self, _sender: Option<&AnyObject>) {
            reset_key(self.mtm());
        }

        #[unsafe(method(disconnectPhone:))]
        fn disconnect_phone(&self, sender: Option<&AnyObject>) {
            if let Some(client) = tag(sender) {
                send(Action::Block(client));
            }
        }

        #[unsafe(method(allowPhone:))]
        fn allow_phone(&self, sender: Option<&AnyObject>) {
            if let Some(client) = tag(sender) {
                send(Action::Unblock(client));
            }
        }

        #[unsafe(method(quit:))]
        fn quit(&self, _sender: Option<&AnyObject>) {
            NSApplication::sharedApplication(self.mtm()).terminate(None);
        }
    }
);

impl Target {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: NSObject's `init` has no preconditions.
        unsafe { msg_send![super(this), init] }
    }
}

/// The phone a menu item or button is about, from its tag.
fn tag(sender: Option<&AnyObject>) -> Option<u64> {
    let sender = sender?;
    // SAFETY: Only menu items and buttons with a tag (an `NSInteger`) call the phone actions.
    let tag: isize = unsafe { msg_send![sender, tag] };
    u64::try_from(tag).ok()
}

fn send(action: Action) {
    with_ui(|ui| {
        let _ = ui.actions.send(action);
    });
}

struct Ui {
    target: Retained<Target>,
    item: Retained<NSStatusItem>,
    actions: mpsc::UnboundedSender<Action>,
    snapshot: Snapshot,
    /// What the current menu shows.
    menu_key: Option<String>,
    window: Option<Window>,
}

impl Ui {
    fn new(mtm: MainThreadMarker, actions: mpsc::UnboundedSender<Action>) -> Self {
        let target = Target::new(mtm);
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        // macOS remembers where the user ⌘-drags the icon, e.g. away from the notch, where a long menu hides it.
        item.setAutosaveName(Some(ns_string!("BandControl")));
        let mut ui = Self {
            target,
            item,
            actions,
            snapshot: Snapshot::default(),
            menu_key: None,
            window: None,
        };
        ui.update(mtm, UiUpdate::State(Snapshot::default()));
        ui
    }

    fn update(&mut self, mtm: MainThreadMarker, update: UiUpdate) {
        match update {
            UiUpdate::State(snapshot) => {
                self.snapshot = snapshot;
                self.update_item(mtm);
                // Replacing the menu while it is open would close it.
                let menu = menu_key(&self.snapshot);
                if self.menu_key.as_ref() != Some(&menu) {
                    self.menu_key = Some(menu);
                    self.item.setMenu(Some(&self.menu(mtm)));
                }
                if let Some(window) = &self.window {
                    window.update(mtm, &self.snapshot, &self.target);
                }
            }
            UiUpdate::ShowPairing => self.show_window(mtm),
            UiUpdate::Quit => NSApplication::sharedApplication(mtm).terminate(None),
        }
    }

    fn update_item(&self, mtm: MainThreadMarker) {
        let Some(button) = self.item.button(mtm) else { return };
        let (level, status) = view::status(&self.snapshot);
        button.setImage(Some(&menu_bar_icon(level)));
        button.setToolTip(Some(&NSString::from_str(&format!("BandControl: {status}"))));
    }

    fn menu(&self, mtm: MainThreadMarker) -> Retained<NSMenu> {
        let menu = NSMenu::new(mtm);
        menu.setAutoenablesItems(false);
        let target = &self.target;
        let info = |title: &str| {
            let item = menu_item(mtm, title, None, target);
            item.setEnabled(false);
            menu.addItem(&item);
        };
        let action = |title: &str, action: Sel| menu.addItem(&menu_item(mtm, title, Some(action), target));

        match &self.snapshot.pairing {
            Some(p) => info(&format!("BandControl · {}", p.name)),
            None => info("BandControl"),
        }
        info(&view::status(&self.snapshot).1);
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        action("Show Pairing QR Code…", sel!(showPairing:));

        if !self.snapshot.phones.is_empty() || !self.snapshot.blocked.is_empty() {
            menu.addItem(&NSMenuItem::separatorItem(mtm));
            info("Phones");
            for phone in &self.snapshot.phones {
                menu.addItem(&phone_item(mtm, phone, "Disconnect", sel!(disconnectPhone:), target));
            }
            for phone in &self.snapshot.blocked {
                let item = phone_item(mtm, phone, "Allow Again", sel!(allowPhone:), target);
                item.setTitle(&NSString::from_str(&format!(
                    "{} (disconnected)",
                    view::phone_name(phone)
                )));
                menu.addItem(&item);
            }
        }

        menu.addItem(&NSMenuItem::separatorItem(mtm));
        let connected = self.snapshot.pairing.is_some();
        for (title, sel) in [("Rename…", sel!(rename:)), ("Reset Pairing Key…", sel!(resetKey:))] {
            let item = menu_item(mtm, title, Some(sel), target);
            item.setEnabled(connected);
            menu.addItem(&item);
        }
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        action("Quit BandControl", sel!(quit:));
        menu
    }

    fn show_window(&mut self, mtm: MainThreadMarker) {
        let window = self.window.get_or_insert_with(|| Window::new(mtm, &self.target));
        window.update(mtm, &self.snapshot, &self.target);
        activate(mtm);
        window.window.makeKeyAndOrderFront(None);
    }
}

/// The app's logo, three faders, as a template image the menu bar tints: dimmed while waiting, struck through when
/// Bluetooth doesn't work.
fn menu_bar_icon(level: Level) -> Retained<NSImage> {
    let draw = RcBlock::new(move |_rect: NSRect| -> Bool {
        draw_faders(level);
        Bool::YES
    });
    let image = NSImage::imageWithSize_flipped_drawingHandler(NSSize::new(18.0, 18.0), true, &draw);
    image.setTemplate(true);
    image.setAccessibilityDescription(Some(ns_string!("BandControl")));
    image
}

/// Draws the icon in an 18 × 18 point flipped context.
fn draw_faders(level: Level) {
    let alpha = if level == Level::Ok { 1.0 } else { 0.55 };
    NSColor::colorWithWhite_alpha(0.0, alpha).setFill();
    // Fader positions like in the app icon: low, high, middle.
    for (x, knob) in [(4.0, 11.5), (9.0, 5.5), (14.0, 9.0)] {
        NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
            NSRect::new(NSPoint::new(x - 0.75, 2.0), NSSize::new(1.5, 14.0)),
            0.75,
            0.75,
        )
        .fill();
        NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
            NSRect::new(NSPoint::new(x - 2.5, knob - 1.6), NSSize::new(5.0, 3.2)),
            1.2,
            1.2,
        )
        .fill();
    }
    if level == Level::Error {
        NSColor::blackColor().setStroke();
        let slash = NSBezierPath::bezierPath();
        slash.moveToPoint(NSPoint::new(1.5, 1.5));
        slash.lineToPoint(NSPoint::new(16.5, 16.5));
        slash.setLineWidth(1.8);
        slash.setLineCapStyle(NSLineCapStyle::Round);
        slash.stroke();
    }
}

/// Everything the menu shows, to rebuild it only when that changes.
fn menu_key(snapshot: &Snapshot) -> String {
    format!(
        "{:?}|{}|{:?}|{:?}",
        snapshot.pairing.as_ref().map(|p| &p.name),
        view::status(snapshot).1,
        snapshot.phones,
        snapshot.blocked
    )
}

fn menu_item(mtm: MainThreadMarker, title: &str, action: Option<Sel>, target: &Target) -> Retained<NSMenuItem> {
    // SAFETY: The action, if any, is a method of `target` taking the sender.
    unsafe {
        let item = NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            action,
            ns_string!(""),
        );
        if action.is_some() {
            item.setTarget(Some(target));
        }
        item
    }
}

/// A phone, with a submenu that disconnects or allows it.
fn phone_item(mtm: MainThreadMarker, phone: &Phone, title: &str, action: Sel, target: &Target) -> Retained<NSMenuItem> {
    let item = menu_item(mtm, &view::phone_name(phone), None, target);
    let submenu = NSMenu::new(mtm);
    let entry = menu_item(mtm, title, Some(action), target);
    entry.setTag(isize::try_from(phone.client).unwrap_or(-1));
    submenu.addItem(&entry);
    item.setSubmenu(Some(&submenu));
    item
}

/// Brings the app to the front, for the window or a dialog.
fn activate(mtm: MainThreadMarker) {
    // `activate` replaces it from macOS 14 on, older versions need this one.
    #[allow(deprecated)]
    NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
}

fn show_window(mtm: MainThreadMarker) {
    with_ui(|ui| ui.show_window(mtm));
}

/// Asks for the name shown on phones; REAPER stores it.
fn rename(mtm: MainThreadMarker) {
    let Some(current) = with_ui(|ui| ui.snapshot.pairing.as_ref().map(|p| p.name.clone())).flatten() else {
        return;
    };
    let alert = NSAlert::new(mtm);
    alert.setMessageText(ns_string!("Name shown on phones"));
    alert.setInformativeText(ns_string!(
        "Phones show this name for the computer. Give every computer a name the band recognizes, like \
         \"Rehearsal laptop\" or \"Stage PC\"."
    ));
    alert.addButtonWithTitle(ns_string!("Rename"));
    alert.addButtonWithTitle(ns_string!("Cancel"));
    let field = NSTextField::textFieldWithString(&NSString::from_str(&current), mtm);
    field.setFrameSize(NSSize::new(260.0, 24.0));
    alert.setAccessoryView(Some(&field));
    alert.window().setInitialFirstResponder(Some(&field));
    activate(mtm);
    if alert.runModal() == NSAlertFirstButtonReturn {
        let name = field.stringValue().to_string();
        if !name.trim().is_empty() && name.trim() != current {
            send(Action::Rename(name.trim().to_owned()));
        }
    }
}

fn reset_key(mtm: MainThreadMarker) {
    let alert = NSAlert::new(mtm);
    alert.setAlertStyle(NSAlertStyle::Warning);
    alert.setMessageText(ns_string!("Reset the pairing key?"));
    alert.setInformativeText(ns_string!(
        "Every paired phone will have to scan the new QR code. Phones paired before can't find this computer anymore."
    ));
    alert.addButtonWithTitle(ns_string!("Reset"));
    alert.addButtonWithTitle(ns_string!("Cancel"));
    activate(mtm);
    if alert.runModal() == NSAlertFirstButtonReturn {
        send(Action::ResetKey);
        show_window(mtm);
    }
}

/// The pairing window. Left: the QR code and the code. Right: the name, the status, the phones and the key.
struct Window {
    window: Retained<NSWindow>,
    qr: Retained<QrView>,
    code: Retained<NSTextField>,
    name: Retained<NSTextField>,
    dot: Retained<NSTextField>,
    status: Retained<NSTextField>,
    phones_title: Retained<NSTextField>,
    phones: Retained<NSStackView>,
    /// The phones the list shows (and whether REAPER is connected), to rebuild it only when they change.
    rows: RefCell<Option<Rows>>,
    buttons: Vec<Retained<NSButton>>,
}

/// Connected and disconnected phones, and whether REAPER is connected.
type Rows = (Vec<Phone>, Vec<Phone>, bool);

/// Width of the right column, in points.
const SIDE_WIDTH: f64 = 300.0;

impl Window {
    fn new(mtm: MainThreadMarker, target: &Target) -> Self {
        let window = new_window(mtm);

        let (left, qr, code) = code_column(mtm);

        // Right: who this computer is to the phones, and which phones are connected.
        let (name_row, name, rename) = name_views(mtm, target);
        let caption = small_label(mtm, "Name shown on phones");
        let (status_row, dot, status) = status_views(mtm);

        let phones_title = label(mtm, "Phones");
        phones_title.setFont(Some(&NSFont::systemFontOfSize_weight(13.0, weight(Weight::Semibold))));
        let phones = vstack(mtm, &[], 2.0);
        let scroll = scrolling(mtm, &phones);

        let reset = button(mtm, "Reset Pairing Key…", sel!(resetKey:), target, None);
        let reset_hint = small_label(mtm, "Unpairs every phone: they have to scan the new code.");

        let separators = [separator(mtm), separator(mtm)];
        let right = vstack(
            mtm,
            &[
                &name_row,
                &caption,
                &status_row,
                &separators[0],
                &phones_title,
                &scroll,
                &separators[1],
                &reset,
                &reset_hint,
            ],
            8.0,
        );
        right.setCustomSpacing_afterView(14.0, &status_row);
        right.setCustomSpacing_afterView(4.0, &phones_title);
        right.setDistribution(NSStackViewDistribution::Fill);
        right
            .widthAnchor()
            .constraintEqualToConstant(SIDE_WIDTH)
            .setActive(true);
        // Rows and texts span the column, the button keeps its size.
        let spanning: [&NSView; 8] = [
            &name_row,
            &caption,
            &status_row,
            &separators[0],
            &phones_title,
            &scroll,
            &separators[1],
            &reset_hint,
        ];
        for view in spanning {
            view.widthAnchor()
                .constraintEqualToAnchor(&right.widthAnchor())
                .setActive(true);
        }
        scroll.setContentHuggingPriority_forOrientation(1.0, NSLayoutConstraintOrientation::Vertical);
        scroll
            .heightAnchor()
            .constraintGreaterThanOrEqualToConstant(96.0)
            .setActive(true);

        let root = hstack(mtm, &[&left, &right]);
        root.setSpacing(28.0);
        // The QR code and its code are centered, the right column fills the height.
        root.setAlignment(NSLayoutAttribute::CenterY);
        root.setDistribution(NSStackViewDistribution::Fill);
        root.setEdgeInsets(NSEdgeInsets {
            top: 24.0,
            left: 24.0,
            bottom: 24.0,
            right: 24.0,
        });
        right
            .heightAnchor()
            .constraintEqualToAnchor_constant(&root.heightAnchor(), -48.0)
            .setActive(true);
        left.heightAnchor()
            .constraintLessThanOrEqualToAnchor_constant(&root.heightAnchor(), -48.0)
            .setActive(true);
        window.setContentView(Some(&root));
        window.center();
        window.setFrameAutosaveName(ns_string!("BandControlPairing"));

        Self {
            window,
            qr,
            code,
            name,
            dot,
            status,
            phones_title,
            phones,
            rows: RefCell::new(None),
            buttons: vec![rename, reset],
        }
    }

    fn update(&self, mtm: MainThreadMarker, snapshot: &Snapshot, target: &Target) {
        let (level, status) = view::status(snapshot);
        self.status.setStringValue(&NSString::from_str(&status));
        let color = match level {
            Level::Ok => NSColor::systemGreenColor(),
            Level::Pending => NSColor::systemOrangeColor(),
            Level::Error => NSColor::systemRedColor(),
        };
        self.dot.setTextColor(Some(&color));
        if let Some(p) = &snapshot.pairing {
            self.qr.set(view::qr(&p.to_url()));
            self.code
                .setStringValue(&NSString::from_str(&p.secret.to_display_code()));
            self.name.setStringValue(&NSString::from_str(&p.name));
            self.window
                .setTitle(&NSString::from_str(&format!("BandControl · {}", p.name)));
        } else {
            self.qr.set(None);
            self.code.setStringValue(ns_string!(" "));
            self.name.setStringValue(ns_string!("BandControl"));
            self.window.setTitle(ns_string!("BandControl"));
        }
        for b in &self.buttons {
            b.setEnabled(snapshot.pairing.is_some());
        }
        let count = snapshot.phones.len();
        self.phones_title.setStringValue(&NSString::from_str(&if count == 0 {
            "Phones".to_owned()
        } else {
            format!("Phones ({count})")
        }));

        let rows = (
            snapshot.phones.clone(),
            snapshot.blocked.clone(),
            snapshot.pairing.is_some(),
        );
        if self.rows.borrow().as_ref() == Some(&rows) {
            return;
        }
        *self.rows.borrow_mut() = Some(rows);
        for view in self.phones.arrangedSubviews().to_vec() {
            view.removeFromSuperview();
        }
        if snapshot.phones.is_empty() && snapshot.blocked.is_empty() {
            let none = wrapping_label(
                mtm,
                if snapshot.pairing.is_some() {
                    "No phones connected. Paired phones connect by themselves while the app is open on them."
                } else {
                    "Phones connect once REAPER is running."
                },
            );
            none.setTextColor(Some(&NSColor::secondaryLabelColor()));
            self.add_row(&none);
        }
        for (phone, blocked) in snapshot
            .phones
            .iter()
            .map(|p| (p, false))
            .chain(snapshot.blocked.iter().map(|p| (p, true)))
        {
            let row = phone_row(mtm, phone, blocked, target);
            self.add_row(&row);
        }
    }

    fn add_row(&self, row: &NSView) {
        self.phones.addArrangedSubview(row);
        row.widthAnchor()
            .constraintEqualToAnchor(&self.phones.widthAnchor())
            .setActive(true);
    }
}

/// Left column: the QR code on a white card, as large as the window allows, with the code to type below it.
fn code_column(mtm: MainThreadMarker) -> (Retained<NSStackView>, Retained<QrView>, Retained<NSTextField>) {
    let qr = QrView::new(mtm);
    let code = label(mtm, "");
    code.setFont(Some(&NSFont::monospacedSystemFontOfSize_weight(
        15.0,
        weight(Weight::Medium),
    )));
    code.setSelectable(true);
    let hint = wrapping_label(mtm, "Scan the QR code with the BandControl app, or type the code.");
    hint.setTextColor(Some(&NSColor::secondaryLabelColor()));
    hint.setAlignment(NSTextAlignment::Center);
    let left = vstack(mtm, &[&qr, &code, &hint], 10.0);
    left.setAlignment(NSLayoutAttribute::CenterX);
    // Takes the width the right column leaves.
    left.setHuggingPriority_forOrientation(1.0, NSLayoutConstraintOrientation::Horizontal);
    // As high as its content, so the root centers it vertically.
    left.setHuggingPriority_forOrientation(999.0, NSLayoutConstraintOrientation::Vertical);
    // A square as wide as the column, smaller when the window is too low for that.
    qr.heightAnchor()
        .constraintEqualToAnchor(&qr.widthAnchor())
        .setActive(true);
    qr.widthAnchor()
        .constraintLessThanOrEqualToAnchor(&left.widthAnchor())
        .setActive(true);
    let fill = qr.widthAnchor().constraintEqualToAnchor(&left.widthAnchor());
    fill.setPriority(750.0);
    fill.setActive(true);
    qr.heightAnchor()
        .constraintGreaterThanOrEqualToConstant(200.0)
        .setActive(true);
    hint.widthAnchor()
        .constraintLessThanOrEqualToAnchor(&left.widthAnchor())
        .setActive(true);
    (left, qr, code)
}

/// The name phones show, large, with the button that changes it.
fn name_views(
    mtm: MainThreadMarker,
    target: &Target,
) -> (Retained<NSStackView>, Retained<NSTextField>, Retained<NSButton>) {
    let name = label(mtm, "");
    name.setFont(Some(&NSFont::systemFontOfSize_weight(18.0, weight(Weight::Semibold))));
    name.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    name.setContentHuggingPriority_forOrientation(1.0, NSLayoutConstraintOrientation::Horizontal);
    name.setContentCompressionResistancePriority_forOrientation(250.0, NSLayoutConstraintOrientation::Horizontal);
    let rename = button(mtm, "Rename…", sel!(rename:), target, None);
    (hstack(mtm, &[&name, &rename]), name, rename)
}

/// A colored dot and the status text.
fn status_views(mtm: MainThreadMarker) -> (Retained<NSStackView>, Retained<NSTextField>, Retained<NSTextField>) {
    let dot = label(mtm, "●");
    let status = wrapping_label(mtm, "");
    let row = hstack(mtm, &[&dot, &status]);
    row.setSpacing(6.0);
    row.setAlignment(NSLayoutAttribute::FirstBaseline);
    (row, dot, status)
}

fn small_label(mtm: MainThreadMarker, text: &str) -> Retained<NSTextField> {
    let label = wrapping_label(mtm, text);
    label.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    label.setTextColor(Some(&NSColor::secondaryLabelColor()));
    label
}

/// A resizable window; it is kept in `Ui` and not released when closed.
fn new_window(mtm: MainThreadMarker) -> Retained<NSWindow> {
    let style = NSWindowStyleMask::Titled
        | NSWindowStyleMask::Closable
        | NSWindowStyleMask::Miniaturizable
        | NSWindowStyleMask::Resizable;
    // SAFETY: A plain window; it is kept in `Ui` and not released when closed.
    let window = unsafe {
        let window = NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(760.0, 480.0)),
            style,
            NSBackingStoreType::Buffered,
            false,
        );
        window.setReleasedWhenClosed(false);
        window
    };
    window.setTitle(ns_string!("BandControl"));
    window.setContentMinSize(NSSize::new(700.0, 440.0));
    window
}

/// A phone: its icon, its name and a button that disconnects it (or allows it again).
fn phone_row(mtm: MainThreadMarker, phone: &Phone, blocked: bool, target: &Target) -> Retained<NSStackView> {
    let name = view::phone_name(phone);
    let symbol = if blocked {
        ns_string!("iphone.slash")
    } else {
        ns_string!("iphone")
    };
    let icon = NSImage::imageWithSystemSymbolName_accessibilityDescription(symbol, None)
        .map(|image| NSImageView::imageViewWithImage(&image, mtm));
    let text = label(mtm, &name);
    text.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    text.setContentHuggingPriority_forOrientation(1.0, NSLayoutConstraintOrientation::Horizontal);
    text.setContentCompressionResistancePriority_forOrientation(250.0, NSLayoutConstraintOrientation::Horizontal);
    let (title, action) = if blocked {
        text.setStringValue(&NSString::from_str(&format!("{name} · disconnected")));
        text.setTextColor(Some(&NSColor::secondaryLabelColor()));
        ("Allow Again", sel!(allowPhone:))
    } else {
        ("Disconnect", sel!(disconnectPhone:))
    };
    let b = button(mtm, title, action, target, Some(phone.client));
    b.setControlSize(NSControlSize::Small);
    b.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    let mut views: Vec<&NSView> = vec![];
    if let Some(icon) = &icon {
        icon.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
        views.push(icon);
    }
    views.extend([&**text as &NSView, &**b]);
    let row = hstack(mtm, &views);
    row.setSpacing(8.0);
    row.setEdgeInsets(NSEdgeInsets {
        top: 4.0,
        left: 2.0,
        bottom: 4.0,
        right: 2.0,
    });
    row
}

/// `stack` in a scroll view, top aligned, scrolling when it is taller than the room it gets.
fn scrolling(mtm: MainThreadMarker, stack: &NSStackView) -> Retained<NSScrollView> {
    let scroll = NSScrollView::new(mtm);
    scroll.setHasVerticalScroller(true);
    scroll.setAutohidesScrollers(true);
    scroll.setDrawsBackground(false);
    scroll.setBorderType(NSBorderType::NoBorder);
    let document = FlippedView::new(mtm);
    document.setTranslatesAutoresizingMaskIntoConstraints(false);
    stack.setTranslatesAutoresizingMaskIntoConstraints(false);
    document.addSubview(stack);
    scroll.setDocumentView(Some(&document));
    let clip = scroll.contentView();
    for constraint in [
        document.leadingAnchor().constraintEqualToAnchor(&clip.leadingAnchor()),
        document
            .trailingAnchor()
            .constraintEqualToAnchor(&clip.trailingAnchor()),
        document.topAnchor().constraintEqualToAnchor(&clip.topAnchor()),
        stack.leadingAnchor().constraintEqualToAnchor(&document.leadingAnchor()),
        stack
            .trailingAnchor()
            .constraintEqualToAnchor(&document.trailingAnchor()),
        stack.topAnchor().constraintEqualToAnchor(&document.topAnchor()),
        stack.bottomAnchor().constraintEqualToAnchor(&document.bottomAnchor()),
    ] {
        constraint.setActive(true);
    }
    scroll
}

fn separator(mtm: MainThreadMarker) -> Retained<NSBox> {
    let separator = NSBox::new(mtm);
    separator.setBoxType(NSBoxType::Separator);
    separator
}

#[derive(Clone, Copy)]
enum Weight {
    Medium,
    Semibold,
}

fn weight(weight: Weight) -> NSFontWeight {
    // SAFETY: Framework constants.
    unsafe {
        match weight {
            Weight::Medium => NSFontWeightMedium,
            Weight::Semibold => NSFontWeightSemibold,
        }
    }
}

fn label(mtm: MainThreadMarker, text: &str) -> Retained<NSTextField> {
    NSTextField::labelWithString(&NSString::from_str(text), mtm)
}

fn wrapping_label(mtm: MainThreadMarker, text: &str) -> Retained<NSTextField> {
    let label = NSTextField::wrappingLabelWithString(&NSString::from_str(text), mtm);
    label.setContentCompressionResistancePriority_forOrientation(250.0, NSLayoutConstraintOrientation::Horizontal);
    label
}

fn button(mtm: MainThreadMarker, title: &str, action: Sel, target: &Target, tag: Option<u64>) -> Retained<NSButton> {
    // SAFETY: `action` is a method of `target` taking the sender.
    let button =
        unsafe { NSButton::buttonWithTitle_target_action(&NSString::from_str(title), Some(target), Some(action), mtm) };
    if let Some(tag) = tag {
        button.setTag(isize::try_from(tag).unwrap_or(-1));
    }
    button
}

fn vstack(mtm: MainThreadMarker, views: &[&NSView], spacing: f64) -> Retained<NSStackView> {
    let stack = NSStackView::stackViewWithViews(&NSArray::from_slice(views), mtm);
    stack.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
    stack.setAlignment(NSLayoutAttribute::Leading);
    stack.setSpacing(spacing);
    stack
}

fn hstack(mtm: MainThreadMarker, views: &[&NSView]) -> Retained<NSStackView> {
    let stack = NSStackView::stackViewWithViews(&NSArray::from_slice(views), mtm);
    stack.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
    stack.setAlignment(NSLayoutAttribute::CenterY);
    stack.setSpacing(12.0);
    stack
}

define_class!(
    /// A view with its origin at the top left, so a scroll view's content starts at the top.
    // SAFETY: NSView has no subclassing requirements and `FlippedView` does not implement `Drop`.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "BandControlFlippedView"]
    struct FlippedView;

    // SAFETY: NSObjectProtocol has no requirements.
    unsafe impl NSObjectProtocol for FlippedView {}

    impl FlippedView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }
    }
);

impl FlippedView {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: NSView's `init` has no preconditions.
        unsafe { msg_send![super(this), init] }
    }
}

/// Quiet zone around the QR code, in modules.
const QUIET_ZONE: f64 = 2.0;

struct QrIvars {
    modules: RefCell<Option<(Vec<bool>, usize)>>,
}

define_class!(
    /// Draws the QR code as large as it fits, centered on a white card (also in dark mode, for the phone camera).
    // SAFETY: NSView has no subclassing requirements and `QrView` does not implement `Drop`.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "BandControlQrView"]
    #[ivars = QrIvars]
    struct QrView;

    // SAFETY: NSObjectProtocol has no requirements.
    unsafe impl NSObjectProtocol for QrView {}

    impl QrView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            self.draw();
        }
    }
);

impl QrView {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(QrIvars {
            modules: RefCell::new(None),
        });
        // SAFETY: NSView's `init` has no preconditions.
        unsafe { msg_send![super(this), init] }
    }

    fn set(&self, modules: Option<(Vec<bool>, usize)>) {
        let mut current = self.ivars().modules.borrow_mut();
        if *current != modules {
            *current = modules;
            self.setNeedsDisplay(true);
        }
    }

    fn draw(&self) {
        let bounds = self.bounds();
        let size = bounds.size.width.min(bounds.size.height);
        let left = bounds.origin.x + (bounds.size.width - size) / 2.0;
        let top = bounds.origin.y + (bounds.size.height - size) / 2.0;
        let card = NSRect::new(NSPoint::new(left, top), NSSize::new(size, size));
        let modules = self.ivars().modules.borrow();
        let Some((dark, width)) = modules.as_ref() else {
            // No REAPER yet: an empty, quiet card.
            NSColor::quaternaryLabelColor().setFill();
            NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(card, 14.0, 14.0).fill();
            return;
        };
        NSColor::whiteColor().setFill();
        NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(card, 14.0, 14.0).fill();
        #[allow(clippy::cast_precision_loss)]
        let module = size / (*width as f64 + 2.0 * QUIET_ZONE);
        NSColor::blackColor().setFill();
        for (i, _) in dark.iter().enumerate().filter(|(_, d)| **d) {
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = ((i % width) as f64, (i / width) as f64);
            // A hair larger than a module, so neighbors don't show seams when scaled.
            NSBezierPath::fillRect(NSRect::new(
                NSPoint::new(left + (QUIET_ZONE + x) * module, top + (QUIET_ZONE + y) * module),
                NSSize::new(module + 0.25, module + 0.25),
            ));
        }
    }
}
