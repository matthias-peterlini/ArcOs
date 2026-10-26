//! Windows: a floating title pill above a glass card, as in the arc design,
//! with buttons to minimise, maximise and close. Their content is an [`App`].
//!
//! An app with a sidebar gets two cards side by side, as in ticly Do: the
//! sidebar and the content, with a gap of wallpaper between them. A
//! maximised window fills the screen without its title pill (and the desktop
//! hides its bar and dock); the pill slides in when the pointer touches the
//! top edge.

use super::gfx::{Canvas, Rect};
use super::res;
use super::text::{self, Font};
use super::ui::{self, TEXT};
use crate::dev::input::KeyEvent;
use alloc::boxed::Box;
use alloc::string::String;

pub const TITLE_H: i32 = 42;
pub const GAP: i32 = 8;
/// The corner that resizes a window.
const GRIP: i32 = 18;

/// What an app asks the desktop to do after an event.
pub enum Action {
    None,
    /// Draw the content again.
    Redraw,
    /// Type a command line into the terminal (opening it).
    Run(String),
    /// Open a file with the app made for it.
    OpenFile(String),
    /// Open a built-in app.
    Open(&'static str),
    /// Start an installed program that opens its own window.
    Spawn(String),
    /// Show a notification: title, text, icon.
    Notify(String, String, &'static str),
    /// The wallpaper changed: draw everything again.
    Wallpaper,
}

/// The content of a window. Positions handed to it are relative to the
/// content area unless said otherwise.
pub trait App {
    fn title(&self) -> String;
    fn icon(&self) -> &str;
    /// The size of the content area the app would like.
    fn size(&self) -> (i32, i32);
    /// Draws the content; `blur` is the blurred wallpaper (for glass).
    fn draw(&mut self, c: &mut Canvas, area: Rect, blur: &Canvas, focused: bool);
    fn press(&mut self, _x: i32, _y: i32, _double: bool, _area: Rect) -> Action {
        Action::None
    }
    /// The pointer moved with the button down, after a press on the content.
    fn drag(&mut self, _x: i32, _y: i32, _area: Rect) -> bool {
        false
    }
    fn release(&mut self, _area: Rect) -> Action {
        Action::None
    }
    /// The pointer moved over the content (`None`: it left).
    fn hover(&mut self, _at: Option<(i32, i32)>, _area: Rect) -> bool {
        false
    }
    /// The right button over the content.
    fn menu(&mut self, _x: i32, _y: i32, _area: Rect) -> Action {
        Action::None
    }
    fn key(&mut self, _ev: KeyEvent) -> Action {
        Action::None
    }
    /// Called ten times a second; true if the content changed.
    fn tick(&mut self) -> bool {
        false
    }
    /// Does the app want a call to [`frame`](Self::frame) on every frame?
    fn animating(&self) -> bool {
        false
    }
    /// A frame while [`animating`](Self::animating); true to redraw.
    fn frame(&mut self) -> bool {
        false
    }
    /// A short name, to find an open window of the same kind.
    fn kind(&self) -> &'static str;
    /// Mouse wheel over the content.
    fn scroll(&mut self, _lines: i32) -> bool {
        false
    }
    /// Can the window be made bigger than [`size`](Self::size)?
    fn resizable(&self) -> bool {
        false
    }
    /// Width of a sidebar drawn as a card of its own at the left of the
    /// content area (0: none). The app still draws into the whole area;
    /// its main content starts at `sidebar() + GAP`.
    fn sidebar(&self) -> i32 {
        0
    }
    /// The window is closing (save what needs saving).
    fn close(&mut self) {}
    /// False when the window should go (its program ended).
    fn alive(&self) -> bool {
        true
    }
}

pub struct Window {
    pub id: u32,
    pub app: Box<dyn App>,
    /// Title pill and body together.
    pub rect: Rect,
    pub minimized: bool,
    /// The size before maximising.
    pub restore: Option<Rect>,
    /// The title button under the pointer.
    pub hover: Option<usize>,
    /// Maximised and the pointer at the top: show the title pill.
    pub reveal: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hit {
    Title,
    Minimize,
    Maximize,
    Close,
    Resize,
    Body(i32, i32),
}

impl Window {
    pub fn new(id: u32, app: Box<dyn App>, x: i32, y: i32) -> Window {
        let (w, h) = app.size();
        Window { id, app, rect: Rect::new(x, y, w, h + TITLE_H + GAP), minimized: false, restore: None, hover: None, reveal: false }
    }

    /// Filling the screen, without a title pill.
    pub fn full(&self) -> bool {
        self.restore.is_some()
    }

    /// The title pill; when maximised, a short one in the middle of the top
    /// edge, shown over the content.
    pub fn title_rect(&self) -> Rect {
        if self.full() {
            let w = (text::width(Font::Strong, &self.app.title()) + 190).clamp(300, self.rect.w / 2);
            return Rect::new(self.rect.x + (self.rect.w - w) / 2, self.rect.y + 10, w, TITLE_H);
        }
        Rect::new(self.rect.x, self.rect.y, self.rect.w, TITLE_H)
    }

    pub fn body(&self) -> Rect {
        if self.full() {
            return self.rect;
        }
        Rect::new(self.rect.x, self.rect.y + TITLE_H + GAP, self.rect.w, self.rect.h - TITLE_H - GAP)
    }

    /// Is the title pill on the screen?
    pub fn has_title(&self) -> bool {
        !self.full() || self.reveal
    }

    /// Minimise, maximise and close, right to left at the end of the title.
    pub fn buttons(&self) -> [Rect; 3] {
        let t = self.title_rect();
        let s = 26;
        let y = t.y + (TITLE_H - s) / 2;
        let x = t.right() - 12 - s;
        [Rect::new(x - 2 * (s + 8), y, s, s), Rect::new(x - (s + 8), y, s, s), Rect::new(x, y, s, s)]
    }

    fn grip(&self) -> Rect {
        let b = self.body();
        Rect::new(b.right() - GRIP, b.bottom() - GRIP, GRIP + 4, GRIP + 4)
    }

    /// Everything the window paints, shadow included.
    pub fn bounds(&self) -> Rect {
        ui::with_shadow(self.rect).union(&ui::with_shadow(self.title_rect()))
    }

    pub fn hit(&self, x: i32, y: i32) -> Option<Hit> {
        if self.has_title() {
            let [min, max, close] = self.buttons();
            if close.contains(x, y) {
                return Some(Hit::Close);
            } else if max.contains(x, y) {
                return Some(Hit::Maximize);
            } else if min.contains(x, y) {
                return Some(Hit::Minimize);
            } else if self.title_rect().contains(x, y) {
                return Some(Hit::Title);
            }
        }
        if self.app.resizable() && !self.full() && self.grip().contains(x, y) {
            Some(Hit::Resize)
        } else if self.body().contains(x, y) {
            let b = self.body();
            Some(Hit::Body(x - b.x, y - b.y))
        } else {
            None
        }
    }

    pub fn draw(&mut self, c: &mut Canvas, blur: &Canvas, focused: bool, clip: Rect) {
        let body = self.body();
        if clip.overlaps(&ui::with_shadow(body)) {
            let tint = if focused { ui::GLASS } else { 0xA61A_1414 };
            let rad = 22;
            let side = self.app.sidebar();
            if side > 0 {
                let sb = Rect::new(body.x, body.y, side, body.h);
                let main = Rect::new(body.x + side + GAP, body.y, body.w - side - GAP, body.h);
                ui::card(c, blur, sb, rad, tint);
                ui::card(c, blur, main, rad, tint);
            } else {
                ui::card(c, blur, body, rad, tint);
            }
            let saved = c.clip();
            c.set_clip(saved.intersect(&body));
            self.app.draw(c, body, blur, focused);
            if self.app.resizable() && !self.full() {
                // Three short diagonal strokes in the corner.
                let (gx, gy) = (body.right() - 8, body.bottom() - 8);
                for k in [4, 8, 12] {
                    c.line(gx - k, gy, gx, gy - k, 1, 0x40FF_FFFF);
                }
            }
            c.set_clip(saved);
        }
        let title = self.title_rect();
        if self.has_title() && clip.overlaps(&ui::with_shadow(title)) {
            let tint = if self.full() { ui::GLASS_STRONG } else if focused { 0xC01A_1414 } else { 0x9C1A_1414 };
            ui::glass(c, blur, title, TITLE_H / 2, tint);
            res::draw_icon(c, self.app.icon(), 22, title.x + 12, title.y + (TITLE_H - 22) / 2);
            let (lh, _) = text::metrics(Font::Strong);
            let name = text::fit(Font::Strong, &self.app.title(), title.w - 160);
            text::draw(c, Font::Strong, title.x + 42, title.y + (TITLE_H - lh) / 2, &name, if focused { TEXT } else { ui::TEXT_DIM });
            for (i, (b, glyph)) in self.buttons().iter().zip(["g-down", "g-max", "g-close"]).enumerate() {
                let hover = self.hover == Some(i);
                let (bg, edge) = match (i, hover) {
                    (2, true) => (0xFFE0_662E, 0),
                    (_, true) => (0x33FF_FFFF, 0x40FF_FFFF),
                    _ => (0x0FFF_FFFF, 0x26FF_FFFF),
                };
                c.fill_circle(b.x + b.w / 2, b.y + b.h / 2, b.w / 2, bg);
                if edge != 0 {
                    c.stroke_round(*b, b.w / 2, 1, edge);
                }
                res::draw_icon(c, glyph, 14, b.x + (b.w - 14) / 2, b.y + (b.h - 14) / 2);
            }
        }
    }

    /// The window drawn on its own, with transparent surroundings (for
    /// animations): the picture and where it goes on the screen.
    pub fn snapshot(&mut self, blur: &Canvas, focused: bool) -> (Canvas, Rect) {
        let b = self.bounds();
        let local_blur = blur.crop(b);
        let mut layer = Canvas::new_layer(b.w, b.h);
        let saved = self.rect;
        self.rect = self.rect.offset(-b.x, -b.y);
        let all = layer.bounds();
        self.draw(&mut layer, &local_blur, focused, all);
        self.rect = saved;
        (layer, b)
    }

    pub fn toggle_maximize(&mut self, screen: Rect) {
        self.reveal = false;
        match self.restore.take() {
            Some(r) => self.rect = r,
            None => {
                self.restore = Some(self.rect);
                self.rect = screen;
            }
        }
    }

    /// The smallest size: what the app asked for, or less for resizable ones.
    pub fn min_size(&self) -> (i32, i32) {
        let (w, h) = self.app.size();
        if self.app.resizable() {
            ((w * 3 / 5).max(320), (h * 3 / 5).max(200) + TITLE_H + GAP)
        } else {
            (w, h + TITLE_H + GAP)
        }
    }
}
