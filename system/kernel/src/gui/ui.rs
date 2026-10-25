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
}

/// A screen the loop drives: it handles events and draws damaged areas.
pub trait View {
    /// Returns true to leave the loop.
    fn event(&mut self, l: &mut Looper, ev: Event) -> bool;
    /// Draws everything inside `r` (the canvas is clipped to it).
    fn draw(&mut self, c: &mut Canvas, r: Rect);
    /// The screen changed size.
    fn resized(&mut self, _w: i32, _h: i32) {}
}

/// Mouse, keyboard and screen updates for one interactive screen.
pub struct Looper {
    pub cursor: Cursor,
    dirty: Rect,
    last_click: (u64, i32, i32),
    last_tick: u64,
    prev_buttons: u8,
}

impl Looper {
    pub fn new(d: &Display) -> Looper {
        let (w, h) = (d.width(), d.height());
        input::set_capture(true);
        Looper { cursor: Cursor::new(w, h), dirty: Rect::EMPTY, last_click: (0, -100, -100), last_tick: 0, prev_buttons: 0 }
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
            if b & 1 != 0 && self.prev_buttons & 1 == 0 {
                let now = crate::time::uptime_ms();
                let (t, px, py) = self.last_click;
                let double = now - t < 450 && (px - x).abs() < 6 && (py - y).abs() < 6;
                self.last_click = if double { (0, -100, -100) } else { (now, x, y) };
                events.push(Event::Press { x, y, double });
            }
            if b & 1 == 0 && self.prev_buttons & 1 != 0 {
                events.push(Event::Release);
            }
            if b & 2 != 0 && self.prev_buttons & 2 == 0 {
                events.push(Event::Menu { x, y });
            }
            self.prev_buttons = b;
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
            crate::task::sleep_ms(8);
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
