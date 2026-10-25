//! Pieces the screens are built from: glass panels, buttons, text fields,
//! and the loop that turns the mouse and keyboard into events.

use super::cursor::Cursor;
use super::gfx::{Canvas, Rect};
use super::text::{self, Font};
use super::Display;
use crate::dev::input::{self, KeyEvent};

pub const TEXT: u32 = 0xFFF5_F2F8;
pub const TEXT_DIM: u32 = 0xB3F5_F2F8;
pub const TEXT_FAINT: u32 = 0x80F5_F2F8;
/// The tint of the dark glass panels.
pub const GLASS: u32 = 0x9E24_1A2C;
pub const GLASS_STRONG: u32 = 0xC71E_1626;
pub const BORDER: u32 = 0x2EFF_FFFF;
/// The highlight colour (selections, switches, the cursor).
pub const ACCENT: u32 = 0xFF0A_84FF;
pub const ORANGE: u32 = 0xFFFF_9F0A;
pub const RED: u32 = 0xFFFF_453A;
pub const GREEN: u32 = 0xFF30_D158;
/// A panel inside a window (lists, sidebars).
pub const PANEL: u32 = 0x1FFF_FFFF;
pub const PANEL_HOVER: u32 = 0x2EFF_FFFF;
pub const SEPARATOR: u32 = 0x24FF_FFFF;

/// A frosted-glass panel: blurred wallpaper under a tint, a thin light edge
/// and a soft shadow.
pub fn glass(c: &mut Canvas, blur: &Canvas, r: Rect, rad: i32, tint: u32) {
    c.shadow(r, rad, 14, 70);
    c.blit_round(blur, r, rad, tint);
    c.stroke_round(r, rad, 1, BORDER);
}

/// The area a panel's shadow reaches.
pub fn with_shadow(r: Rect) -> Rect {
    r.grow(16).offset(0, 5)
}

/// A pill-shaped button; `primary` is white with dark text.
pub fn button(c: &mut Canvas, r: Rect, label: &str, primary: bool, hover: bool) {
    let (bg, fg) = if primary {
        (if hover { 0xFFFF_FFFF } else { 0xF2FF_FFFF }, 0xFF1E_1626)
    } else {
        (if hover { 0x40FF_FFFF } else { 0x26FF_FFFF }, TEXT)
    };
    c.fill_round(r, r.h / 2, bg);
    let (lh, _) = text::metrics(Font::Strong);
    text::draw_centered(c, Font::Strong, r.x + r.w / 2, r.y + (r.h - lh) / 2, label, fg);
}

/// A button in a colour of its own.
pub fn button_color(c: &mut Canvas, r: Rect, label: &str, bg: u32, fg: u32) {
    c.fill_round(r, r.h / 2, bg);
    let (lh, _) = text::metrics(Font::Strong);
    text::draw_centered(c, Font::Strong, r.x + r.w / 2, r.y + (r.h - lh) / 2, label, fg);
}

/// A round button with a white glyph from the icon set.
pub fn icon_button(c: &mut Canvas, r: Rect, glyph: &str, hover: bool) {
    c.fill_circle(r.x + r.w / 2, r.y + r.h / 2, r.w / 2, if hover { 0x40FF_FFFF } else { 0x26FF_FFFF });
    let s = r.w * 5 / 9;
    super::res::draw_icon(c, glyph, s, r.x + (r.w - s) / 2, r.y + (r.h - s) / 2);
}

/// An on/off switch (52 x 30 looks right).
pub fn toggle(c: &mut Canvas, r: Rect, on: bool) {
    c.fill_round(r, r.h / 2, if on { GREEN } else { 0x40FF_FFFF });
    let d = r.h - 6;
    let x = if on { r.right() - 3 - d } else { r.x + 3 };
    c.fill_circle(x + d / 2, r.y + 3 + d / 2, d / 2 + 1, 0x40_00_00_00);
    c.fill_circle(x + d / 2, r.y + 3 + d / 2, d / 2, 0xFFFF_FFFF);
}

/// A horizontal slider; `value` in 0..=1000.
pub fn slider(c: &mut Canvas, r: Rect, value: u32) {
    let track = Rect::new(r.x, r.y + r.h / 2 - 3, r.w, 6);
    c.fill_round(track, 3, 0x40FF_FFFF);
    let x = r.x + (r.w as i64 * value.min(1000) as i64 / 1000) as i32;
    c.fill_round(Rect::new(track.x, track.y, (x - r.x).max(6), 6), 3, 0xFFFF_FFFF);
    c.fill_circle(x, r.y + r.h / 2, 10, 0x50_00_00_00);
    c.fill_circle(x, r.y + r.h / 2, 9, 0xFFFF_FFFF);
}

/// The slider value (0..=1000) for a pointer at `x`.
pub fn slider_value(r: Rect, x: i32) -> u32 {
    ((x - r.x).clamp(0, r.w.max(1)) as i64 * 1000 / r.w.max(1) as i64) as u32
}

/// The parts of a segmented control.
pub fn segments(r: Rect, n: usize) -> alloc::vec::Vec<Rect> {
    let n = n.max(1) as i32;
    let w = (r.w - 4) / n;
    (0..n).map(|i| Rect::new(r.x + 2 + i * w, r.y + 2, w, r.h - 4)).collect()
}

/// Tabs in a pill: `labels`, with `sel` highlighted.
pub fn segmented(c: &mut Canvas, r: Rect, labels: &[&str], sel: usize) {
    c.fill_round(r, r.h / 2, 0x26FF_FFFF);
    let (lh, _) = text::metrics(Font::Strong);
    for (i, (s, label)) in segments(r, labels.len()).iter().zip(labels).enumerate() {
        if i == sel {
            c.fill_round(*s, s.h / 2, 0x40FF_FFFF);
        }
        let color = if i == sel { TEXT } else { TEXT_DIM };
        text::draw_centered(c, Font::Strong, s.x + s.w / 2, s.y + (s.h - lh) / 2, label, color);
    }
}

/// A search field with its magnifier.
pub fn search_field(c: &mut Canvas, r: Rect, value: &str, placeholder: &str, focused: bool) {
    c.fill_round(r, r.h / 2, 0x26FF_FFFF);
    if focused {
        c.stroke_round(r, r.h / 2, 2, ACCENT);
    }
    super::res::draw_icon(c, "g-search", 14, r.x + 12, r.y + (r.h - 14) / 2);
    let (lh, _) = text::metrics(Font::Body);
    let ty = r.y + (r.h - lh) / 2;
    if value.is_empty() {
        text::draw(c, Font::Body, r.x + 34, ty, placeholder, TEXT_FAINT);
        if focused {
            c.fill(Rect::new(r.x + 34, ty + 2, 2, lh - 4), ACCENT);
        }
    } else {
        let shown = text::fit(Font::Body, value, r.w - 50);
        let w = text::draw(c, Font::Body, r.x + 34, ty, &shown, TEXT);
        if focused {
            c.fill(Rect::new(r.x + 35 + w, ty + 2, 2, lh - 4), ACCENT);
        }
    }
}

/// A one-line text field.
pub fn text_field(c: &mut Canvas, r: Rect, value: &str, placeholder: &str, focused: bool) {
    c.fill_round(r, 12, 0x33FF_FFFF);
    c.stroke_round(r, 12, if focused { 2 } else { 1 }, if focused { 0xFFFF_FFFF } else { BORDER });
    let (lh, _) = text::metrics(Font::Heading);
    let ty = r.y + (r.h - lh) / 2;
    let shown = text::fit(Font::Heading, value, r.w - 36);
    let w = if value.is_empty() {
        text::draw(c, Font::Heading, r.x + 16, ty, placeholder, TEXT_FAINT);
        0
    } else {
        text::draw(c, Font::Heading, r.x + 16, ty, &shown, TEXT)
    };
    if focused {
        c.fill(Rect::new(r.x + 17 + w, ty + 2, 2, lh - 4), 0xFFFF_FFFF);
    }
}

/// Applies a key to a text value; returns whether it changed.
pub fn edit_text(value: &mut alloc::string::String, ev: &KeyEvent, max: usize) -> bool {
    use crate::dev::input::Key;
    match ev.key {
        Key::Backspace => value.pop().is_some(),
        Key::Char(ch) if !ev.ctrl && !ch.is_control() && value.chars().count() < max => {
            value.push(ch);
            true
        }
        _ => false,
    }
}

/// What the event loop hands to a screen.
#[derive(Clone, Copy, Debug)]
pub enum Event {
    /// The left button went down (`double`: the second click of a pair).
    Press {
        x: i32,
        y: i32,
        double: bool,
    },
    /// The right button went down.
    Menu {
        x: i32,
        y: i32,
    },
    /// The left button went up.
    Release,
    /// The pointer moved (with the left button `held` or not).
    Move {
        x: i32,
        y: i32,
        held: bool,
    },
    /// The wheel turned (positive: scroll down).
    Wheel {
        lines: i32,
    },
    Key(KeyEvent),
    /// About ten times a second, for clocks and blinking.
    Tick,
    /// As often as the screen can be drawn, while the view animates.
    Frame,
}

/// A screen the loop drives: it handles events and draws damaged areas.
pub trait View {
    /// Returns true to leave the loop.
    fn event(&mut self, l: &mut Looper, ev: Event) -> bool;
    /// Draws everything inside `r` (the canvas is clipped to it).
    fn draw(&mut self, c: &mut Canvas, r: Rect);
    /// The screen changed size.
    fn resized(&mut self, _w: i32, _h: i32) {}
    /// Is something moving? The loop then sends [`Event::Frame`] for every
    /// frame instead of resting between events.
    fn animating(&self) -> bool {
        false
    }
}

// ----------------------------------------------------------------------------
// Animation

/// A transition that runs for a fixed time, whatever the frame rate: each
/// frame asks how far along it is.
#[derive(Clone, Copy, Debug)]
pub struct Anim {
    start: u64,
    ms: u64,
}

impl Anim {
    pub fn new(ms: u64) -> Anim {
        Anim { start: crate::time::uptime_ms(), ms: ms.max(1) }
    }

    /// Progress, 0..=1000.
    pub fn t(&self) -> u32 {
        let el = crate::time::uptime_ms().saturating_sub(self.start);
        (el.min(self.ms) * 1000 / self.ms) as u32
    }

    pub fn done(&self) -> bool {
        self.t() >= 1000
    }

    /// Eased progress (fast start, gentle stop), 0..=1000.
    pub fn out(&self) -> u32 {
        ease_out(self.t())
    }

    pub fn in_out(&self) -> u32 {
        ease_in_out(self.t())
    }

    /// Milliseconds since it started.
    pub fn elapsed(&self) -> u64 {
        crate::time::uptime_ms().saturating_sub(self.start)
    }
}

/// Cubic ease-out: 1 - (1 - t)^3, in 1/1000.
pub fn ease_out(t: u32) -> u32 {
    let u = 1000 - t.min(1000) as u64;
    (1000 - u * u * u / 1_000_000) as u32
}

/// Cubic ease-in-out, in 1/1000.
pub fn ease_in_out(t: u32) -> u32 {
    let t = t.min(1000) as u64;
    if t < 500 {
        (4 * t * t * t / 1_000_000) as u32
    } else {
        let u = 1000 - t;
        (1000 - 4 * u * u * u / 1_000_000) as u32
    }
}

/// `a` to `b` at `t` (0..=1000).
pub fn lerp(a: i32, b: i32, t: u32) -> i32 {
    a + ((b - a) as i64 * t as i64 / 1000) as i32
}

pub fn lerp_rect(a: Rect, b: Rect, t: u32) -> Rect {
    Rect::new(lerp(a.x, b.x, t), lerp(a.y, b.y, t), lerp(a.w, b.w, t), lerp(a.h, b.h, t))
}

/// `r` scaled by `permille` around its centre.
pub fn scale_rect(r: Rect, permille: i32) -> Rect {
    let w = (r.w as i64 * permille as i64 / 1000) as i32;
    let h = (r.h as i64 * permille as i64 / 1000) as i32;
    Rect::new(r.x + (r.w - w) / 2, r.y + (r.h - h) / 2, w, h)
}

/// A value that glides towards a target (hover highlights, the dock's
/// magnification): call [`step`](Self::step) every frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Glide {
    pub value: i32,
    pub target: i32,
    last: u64,
}

impl Glide {
    pub fn new(v: i32) -> Glide {
        Glide { value: v, target: v, last: 0 }
    }

    /// Moves a share of the way to the target (about 90% in 150 ms);
    /// returns whether it is still moving.
    pub fn step(&mut self) -> bool {
        let now = crate::time::uptime_ms();
        let dt = if self.last == 0 { 16 } else { now.saturating_sub(self.last).min(100) } as i64;
        self.last = now;
        if self.value == self.target {
            self.last = 0;
            return false;
        }
        let diff = (self.target - self.value) as i64;
        let mut d = diff * (dt * 15).min(1000) / 1000;
        if d == 0 {
            d = diff.signum();
        }
        self.value += d as i32;
        if (self.target - self.value).abs() <= 2 {
            self.value = self.target;
        }
        true
    }

    pub fn moving(&self) -> bool {
        self.value != self.target
    }
}

// ----------------------------------------------------------------------------
// Clipboard

static CLIPBOARD: crate::sync::IrqLock<alloc::string::String> = crate::sync::IrqLock::new(alloc::string::String::new());

pub fn copy(text: &str) {
    *CLIPBOARD.lock() = alloc::string::String::from(text);
}

pub fn paste() -> alloc::string::String {
    CLIPBOARD.lock().clone()
}

/// Mouse, keyboard and screen updates for one interactive screen.
pub struct Looper {
    pub cursor: Cursor,
    dirty: Rect,
    last_click: (u64, i32, i32),
    last_tick: u64,
}

impl Looper {
    pub fn new(d: &Display) -> Looper {
        let (w, h) = (d.width(), d.height());
        input::set_capture(true);
        input::take_clicks();
        Looper { cursor: Cursor::new(w, h), dirty: Rect::EMPTY, last_click: (0, -100, -100), last_tick: 0 }
    }

    /// Marks part of the screen to be drawn again.
    pub fn damage(&mut self, r: Rect) {
        self.dirty = self.dirty.union(&r);
    }

    pub fn damage_all(&mut self, d: &Display) {
        self.damage(d.scene.bounds());
    }

    /// Waits for the next events and passes each to `handle`; redraws the
    /// damaged area with `draw` after each batch. Returns when `handle`
    /// returns true.
    pub fn run(&mut self, d: &mut Display, view: &mut dyn View) {
        loop {
            if let Some(mode) = super::take_mode_request() {
                self.cursor.hide(d);
                match d.set_mode(mode) {
                    Ok(()) => {
                        view.resized(d.width(), d.height());
                        self.damage_all(d);
                    }
                    Err(e) => crate::warn!("gui", "{}x{}: {e}", mode.width, mode.height),
                }
            }
            let mut events: alloc::vec::Vec<Event> = alloc::vec::Vec::new();
            let (w, h) = (d.width(), d.height());
            let moved = self.cursor.poll(w, h);
            let (x, y) = (self.cursor.x, self.cursor.y);
            let b = self.cursor.buttons;
            if moved {
                events.push(Event::Move { x, y, held: b & 1 != 0 });
            }
            // Clicks in the order they happened, even if quicker than a frame.
            for (button, down, rx, ry) in input::take_clicks() {
                let (cx, cy) = self.cursor.click_at(rx, ry, w, h);
                match (button, down) {
                    (0, true) => {
                        let now = crate::time::uptime_ms();
                        let (t, px, py) = self.last_click;
                        let double = now - t < 450 && (px - cx).abs() < 6 && (py - cy).abs() < 6;
                        self.last_click = if double { (0, -100, -100) } else { (now, cx, cy) };
                        events.push(Event::Press { x: cx, y: cy, double });
                    }
                    (0, false) => events.push(Event::Release),
                    (1, true) => events.push(Event::Menu { x: cx, y: cy }),
                    _ => {}
                }
            }
            if self.cursor.scrolled != 0 {
                events.push(Event::Wheel { lines: self.cursor.scrolled });
            }
            while let Some(k) = input::gui_key() {
                events.push(Event::Key(k));
            }
            let now = crate::time::uptime_ms();
            if now - self.last_tick >= 100 {
                self.last_tick = now;
                events.push(Event::Tick);
            }
            let animating = view.animating();
            if animating {
                events.push(Event::Frame);
            }
            for ev in events {
                if view.event(self, ev) {
                    self.flush(d, view);
                    return;
                }
            }
            if moved {
                self.cursor.hide(d);
            }
            self.flush(d, view);
            if moved {
                self.cursor.show(d);
            }
            crate::task::sleep_ms(if animating { 2 } else { 8 });
        }
    }

    fn flush(&mut self, d: &mut Display, view: &mut dyn View) {
        let r = self.dirty.intersect(&d.scene.bounds());
        self.dirty = Rect::EMPTY;
        if r.is_empty() {
            return;
        }
        d.scene.set_clip(r);
        view.draw(&mut d.scene, r);
        d.scene.reset_clip();
        d.present(r);
        if r.overlaps(&self.cursor.rect()) {
            self.cursor.show(d);
        }
    }
}
