//! The pairing window: QR code, code, name and status, with buttons to rename and to reset the key.
//!
//! Everything shown is computed by [`layout`] from the window size, so the QR code grows with the window; the platform
//! modules only create the window, forward its events and draw the resulting rectangles and texts. All of it runs on
//! REAPER's main thread.

use std::cell::RefCell;

use bandcontrol_proto::pairing::PairingInfo;

#[cfg(not(target_os = "windows"))]
#[path = "swell.rs"]
mod native;
#[cfg(target_os = "windows")]
#[path = "win32.rs"]
mod native;

const TITLE: &str = "BandControl pairing";
/// Initial client size before scaling.
const SIZE: (f32, f32) = (440.0, 640.0);
const MIN_SIZE: (f32, f32) = (320.0, 460.0);
/// Light modules around the code, as the QR specification asks for.
const QUIET_ZONE: usize = 4;
/// Character advance of the monospace font relative to its size, with some room to spare.
const MONO_ADVANCE: f32 = 0.65;
/// The same, on average, for the proportional font.
const AVERAGE_ADVANCE: f32 = 0.55;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

const BACKGROUND: Rgb = Rgb(31, 41, 55);
const TEXT: Rgb = Rgb(229, 231, 235);
const MUTED: Rgb = Rgb(156, 163, 175);
const BUTTON: Rgb = Rgb(55, 65, 81);
const DANGER: Rgb = Rgb(248, 113, 113);
const OK: Rgb = Rgb(52, 211, 153);
const PENDING: Rgb = Rgb(234, 179, 8);
const ERROR: Rgb = Rgb(239, 68, 68);
const LIGHT: Rgb = Rgb(255, 255, 255);
const DARK: Rgb = Rgb(0, 0, 0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    #[must_use]
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    #[must_use]
    pub const fn width(&self) -> i32 {
        self.right - self.left
    }

    #[must_use]
    pub const fn height(&self) -> i32 {
        self.bottom - self.top
    }

    #[must_use]
    pub const fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Font {
    /// Character height in pixels.
    pub size: i32,
    pub bold: bool,
    pub mono: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Fill(Rect, Rgb),
    /// Single line, vertically centered, cut with an ellipsis when too long.
    Text {
        rect: Rect,
        text: String,
        font: Font,
        color: Rgb,
        center: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Rename,
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Pending,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub text: String,
    pub level: Level,
}

/// The QR code's modules, row by row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Qr {
    width: usize,
    dark: Vec<bool>,
}

/// What the window shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    name: String,
    code: String,
    qr: Option<Qr>,
    status: Status,
}

impl View {
    #[must_use]
    pub fn new(info: &PairingInfo, status: Status) -> Self {
        let qr = qrcode::QrCode::new(info.to_url().as_bytes()).ok().map(|code| Qr {
            width: code.width(),
            dark: code.to_colors().into_iter().map(|c| c == qrcode::Color::Dark).collect(),
        });
        Self {
            name: info.name.clone(),
            code: info.secret.to_display_code(),
            qr,
            status,
        }
    }
}

#[derive(Debug, Default)]
pub struct Layout {
    pub ops: Vec<Op>,
    pub buttons: Vec<(Rect, Button)>,
    /// The QR code including its quiet zone, `None` when the window is too small for it.
    pub qr: Option<Rect>,
}

/// Lays the window out for a client area of `width` x `height` pixels at `scale` (1.0 = 96 dpi).
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
pub fn layout(view: &View, width: i32, height: i32, scale: f32) -> Layout {
    let px = |v: f32| (v * scale).round() as i32;
    let font = |size: f32, bold: bool, mono: bool| Font {
        size: px(size),
        bold,
        mono,
    };
    let mut out = Layout::default();
    out.ops.push(Op::Fill(Rect::new(0, 0, width, height), BACKGROUND));
    let mut text = |rect: Rect, text: &str, font: Font, color: Rgb| {
        out.ops.push(Op::Text {
            rect,
            text: fit(text, rect.width(), font),
            font,
            color,
            center: true,
        });
    };

    let pad = px(16.0);
    let (left, right) = (pad, width - pad);
    let mut top = pad;
    let title = format!("Pair a phone with {}", view.name);
    text(
        Rect::new(left, top, right, top + px(26.0)),
        &title,
        font(18.0, true, false),
        TEXT,
    );
    top += px(26.0);
    let hint = "Scan with the BandControl app, or type the code in it";
    text(
        Rect::new(left, top, right, top + px(20.0)),
        hint,
        font(13.0, false, false),
        MUTED,
    );
    top += px(30.0);

    // From the bottom up: buttons, note, status, code; the QR code gets what is left.
    let mut bottom = height - pad;
    let buttons = Rect::new(left, bottom - px(32.0), right, bottom);
    bottom = buttons.top - px(12.0);
    let note = "Anyone with this code can control playback and mixes";
    text(
        Rect::new(left, bottom - px(16.0), right, bottom),
        note,
        font(11.0, false, false),
        MUTED,
    );
    bottom -= px(24.0);
    let color = match view.status.level {
        Level::Ok => OK,
        Level::Pending => PENDING,
        Level::Error => ERROR,
    };
    let status = Rect::new(left, bottom - px(20.0), right, bottom);
    text(status, &view.status.text, font(13.0, false, false), color);
    bottom -= px(24.0);
    let code = Rect::new(left, bottom - px(32.0), right, bottom);
    // As large as fits: monospace characters are about 0.6 of the font size wide.
    let chars = view.code.chars().count().max(1) as f32;
    let code_font = Font {
        size: px(22.0).min((code.width() as f32 / (chars * MONO_ADVANCE)) as i32),
        bold: true,
        mono: true,
    };
    text(code, &view.code, code_font, TEXT);
    bottom -= px(42.0);

    if let Some(qr) = &view.qr {
        out.qr = draw_qr(&mut out.ops, qr, Rect::new(left, top, right, bottom));
    }

    let gap = px(10.0);
    let button_width = px(170.0).min((buttons.width() - gap) / 2);
    let x = (width - 2 * button_width - gap) / 2;
    for (i, (button, label, color)) in [
        (Button::Rename, "Rename…", TEXT),
        (Button::Reset, "Reset pairing key…", DANGER),
    ]
    .into_iter()
    .enumerate()
    {
        let offset = if i == 0 { 0 } else { button_width + gap };
        let rect = Rect::new(x + offset, buttons.top, x + offset + button_width, buttons.bottom);
        out.ops.push(Op::Fill(rect, BUTTON));
        out.ops.push(Op::Text {
            rect,
            text: label.to_owned(),
            font: font(13.0, false, false),
            color,
            center: true,
        });
        out.buttons.push((rect, button));
    }
    out
}

/// `text` shortened with an ellipsis to what roughly fits `width`: not every platform cuts centered text.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]
fn fit(text: &str, width: i32, font: Font) -> String {
    let advance = if font.mono { MONO_ADVANCE } else { AVERAGE_ADVANCE };
    let fits = (width.max(0) as f32 / (font.size.max(1) as f32 * advance)) as usize;
    if text.chars().count() <= fits {
        return text.to_owned();
    }
    let mut short: String = text.chars().take(fits.saturating_sub(1)).collect();
    short.truncate(short.trim_end().len());
    short.push('…');
    short
}

/// The largest whole number of pixels per module that fits `area`, so the code stays sharp at any size.
fn draw_qr(ops: &mut Vec<Op>, qr: &Qr, area: Rect) -> Option<Rect> {
    let modules = i32::try_from(qr.width + 2 * QUIET_ZONE).ok()?;
    let module = area.width().min(area.height()) / modules;
    if module < 1 {
        return None;
    }
    let side = module * modules;
    let x0 = area.left + (area.width() - side) / 2;
    let y0 = area.top + (area.height() - side) / 2;
    let rect = Rect::new(x0, y0, x0 + side, y0 + side);
    ops.push(Op::Fill(rect, LIGHT));
    let quiet = i32::try_from(QUIET_ZONE).ok()?;
    for (y, row) in (0..).zip(qr.dark.chunks(qr.width)) {
        let mut x = 0;
        // One rectangle per run of dark modules.
        for run in row.chunk_by(|a, b| a == b) {
            let len = i32::try_from(run.len()).ok()?;
            if run.first() == Some(&true) {
                let left = x0 + (quiet + x) * module;
                let top = y0 + (quiet + y) * module;
                ops.push(Op::Fill(Rect::new(left, top, left + len * module, top + module), DARK));
            }
            x += len;
        }
    }
    Some(rect)
}

fn hit(view: &View, width: i32, height: i32, scale: f32, x: i32, y: i32) -> Option<Button> {
    layout(view, width, height, scale)
        .buttons
        .into_iter()
        .find_map(|(rect, button)| rect.contains(x, y).then_some(button))
}

struct Open {
    window: native::Window,
    view: View,
}

thread_local! {
    static OPEN: RefCell<Option<Open>> = const { RefCell::new(None) };
}

/// Shows the window (or brings it to the front) with `view`. `false` when no native window can be created here.
pub fn show(view: View, parent: *mut std::ffi::c_void) -> bool {
    if OPEN
        .with_borrow(|open| open.as_ref().map(|o| o.window.raise()))
        .is_some()
    {
        update(view);
        return true;
    }
    // Messages sent while creating find no state yet and only paint the background.
    let Some(window) = native::Window::create(TITLE, SIZE, parent) else {
        return false;
    };
    window.invalidate();
    OPEN.with_borrow_mut(|open| *open = Some(Open { window, view }));
    true
}

/// Replaces what an open window shows.
pub fn update(view: View) {
    OPEN.with_borrow_mut(|open| {
        if let Some(open) = open.as_mut().filter(|o| o.view != view) {
            open.view = view;
            open.window.invalidate();
        }
    });
}

pub fn set_status(status: Status) {
    OPEN.with_borrow_mut(|open| {
        if let Some(open) = open.as_mut().filter(|o| o.view.status != status) {
            open.view.status = status;
            open.window.invalidate();
        }
    });
}

#[must_use]
pub fn is_open() -> bool {
    OPEN.with_borrow(Option::is_some)
}

/// Closes the window, e.g. when the extension unloads.
pub fn close() {
    // Taken out first: destroying sends messages that look at the state again.
    if let Some(open) = OPEN.with_borrow_mut(Option::take) {
        open.window.destroy();
    }
}

// Called by the platform window.

fn paint(width: i32, height: i32, scale: f32) -> Vec<Op> {
    OPEN.try_with(|open| match open.try_borrow().ok().as_ref().and_then(|o| o.as_ref()) {
        Some(open) => layout(&open.view, width, height, scale).ops,
        None => vec![Op::Fill(Rect::new(0, 0, width, height), BACKGROUND)],
    })
    .unwrap_or_default()
}

fn clicked(width: i32, height: i32, scale: f32, x: i32, y: i32) {
    let button = OPEN.with_borrow(|open| open.as_ref().and_then(|o| hit(&o.view, width, height, scale, x, y)));
    // No borrow held: the actions open modal dialogs, which paint this window meanwhile.
    if let Some(button) = button {
        crate::window_button(button);
    }
}

fn destroyed() {
    OPEN.with_borrow_mut(|open| *open = None);
}

#[cfg(test)]
mod tests {
    use bandcontrol_proto::pairing::PairingSecret;
    use pretty_assertions::assert_eq;

    use super::*;

    fn view() -> View {
        View::new(
            &PairingInfo {
                secret: PairingSecret::from_bytes([7; 16]),
                name: "Stage PC".into(),
            },
            Status {
                text: "Bluetooth on".into(),
                level: Level::Ok,
            },
        )
    }

    fn texts(layout: &Layout) -> Vec<&str> {
        layout
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::Text { text, .. } => Some(text.as_str()),
                Op::Fill(..) => None,
            })
            .collect()
    }

    #[test]
    fn shows_name_code_and_status() {
        let view = view();
        let layout = layout(&view, 440, 640, 1.0);
        let texts = texts(&layout);
        assert!(texts.contains(&"Pair a phone with Stage PC"));
        assert!(texts.contains(&view.code.as_str()));
        assert!(texts.contains(&"Bluetooth on"));
    }

    #[test]
    fn long_texts_are_shortened() {
        let font = Font {
            size: 10,
            bold: false,
            mono: false,
        };
        assert_eq!(fit("Bluetooth on", 200, font), "Bluetooth on");
        assert_eq!(
            fit("Pair a phone with a computer named at length", 110, font),
            "Pair a phone with a…"
        );
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn the_code_fits_narrow_windows() {
        let view = view();
        let layout = layout(&view, 320, 460, 1.0);
        let fits = layout.ops.iter().any(|op| match op {
            Op::Text { rect, text, font, .. } if *text == view.code => {
                let width = text.chars().count() as f32 * font.size as f32 * 0.6;
                width <= rect.width() as f32
            }
            _ => false,
        });
        assert!(fits);
    }

    #[test]
    fn qr_code_grows_with_the_window_in_whole_pixels() {
        let view = view();
        let modules = i32::try_from(view.qr.as_ref().map_or(0, |q| q.width) + 2 * QUIET_ZONE).unwrap_or(0);
        let mut last = 0;
        for size in [(320, 460), (440, 640), (800, 900), (1600, 1400)] {
            let layout = layout(&view, size.0, size.1, 1.0);
            let qr = layout.qr.unwrap_or_default();
            assert_eq!(qr.width(), qr.height());
            assert_eq!(qr.width() % modules, 0, "whole pixels per module at {size:?}");
            assert!(qr.width() > last, "grows at {size:?}");
            assert!(qr.left >= 0 && qr.right <= size.0 && qr.top > 0 && qr.bottom < size.1);
            // Nothing else overlaps the code.
            for (rect, _) in &layout.buttons {
                assert!(rect.top >= qr.bottom);
            }
            last = qr.width();
        }
    }

    #[test]
    fn qr_modules_match_the_code() {
        let view = view();
        let (width, qr) = view
            .qr
            .as_ref()
            .map(|q| (q.width, q.dark.iter().filter(|d| **d).count()))
            .unwrap_or_default();
        let layout = layout(&view, 440, 640, 1.0);
        let module = layout.qr.map_or(0, |r| r.width()) / i32::try_from(width + 2 * QUIET_ZONE).unwrap_or(1);
        let area: i32 = layout
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::Fill(rect, DARK) => Some(rect.width() * rect.height()),
                _ => None,
            })
            .sum();
        assert_eq!(area, i32::try_from(qr).unwrap_or(0) * module * module);
    }

    #[test]
    fn tiny_windows_drop_the_code_instead_of_overlapping() {
        let layout = layout(&view(), 200, 250, 1.0);
        assert_eq!(layout.qr, None);
    }

    #[test]
    fn buttons_are_hit() {
        let view = view();
        let layout = layout(&view, 440, 640, 2.0);
        for (rect, button) in &layout.buttons {
            let (x, y) = (rect.left.midpoint(rect.right), rect.top.midpoint(rect.bottom));
            assert_eq!(hit(&view, 440, 640, 2.0, x, y), Some(*button));
        }
        assert_eq!(hit(&view, 440, 640, 2.0, 5, 5), None);
        assert_eq!(layout.buttons.len(), 2);
    }
}
