//! Windows: a floating title pill above a glass body, as in the arc design,
//! with buttons to minimise, maximise and close. Their content is an [`App`].

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
        Window { id, app, rect: Rect::new(x, y, w, h + TITLE_H + GAP), minimized: false, restore: None, hover: None }
    }

    pub fn title_rect(&self) -> Rect {
        Rect::new(self.rect.x, self.rect.y, self.rect.w, TITLE_H)
    }

    pub fn body(&self) -> Rect {
        Rect::new(self.rect.x, self.rect.y + TITLE_H + GAP, self.rect.w, self.rect.h - TITLE_H - GAP)
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
        ui::with_shadow(self.rect)
    }

    pub fn hit(&self, x: i32, y: i32) -> Option<Hit> {
        let [min, max, close] = self.buttons();
        if close.contains(x, y) {
            Some(Hit::Close)
        } else if max.contains(x, y) {
            Some(Hit::Maximize)
        } else if min.contains(x, y) {
            Some(Hit::Minimize)
        } else if self.title_rect().contains(x, y) {
            Some(Hit::Title)
        } else if self.app.resizable() && self.restore.is_none() && self.grip().contains(x, y) {
            Some(Hit::Resize)
        } else if self.body().contains(x, y) {
            let b = self.body();
            Some(Hit::Body(x - b.x, y - b.y))
        } else {
            None
        }
    }

    pub fn draw(&mut self, c: &mut Canvas, blur: &Canvas, focused: bool, clip: Rect) {
        let title = self.title_rect();
        let body = self.body();
        if clip.overlaps(&ui::with_shadow(title)) {
            ui::glass(c, blur, title, TITLE_H / 2, if focused { 0xB82A_2032 } else { 0x9030_2838 });
            res::draw_icon(c, self.app.icon(), 22, title.x + 14, title.y + (TITLE_H - 22) / 2);
            let (lh, _) = text::metrics(Font::Strong);
            let name = text::fit(Font::Strong, &self.app.title(), title.w - 170);
            text::draw(c, Font::Strong, title.x + 44, title.y + (TITLE_H - lh) / 2, &name, if focused { TEXT } else { ui::TEXT_DIM });
            for (i, (b, glyph)) in self.buttons().iter().zip(["g-down", "g-max", "g-close"]).enumerate() {
                let hover = self.hover == Some(i);
                let bg = match (i, hover) {
                    (2, true) => 0xFFE8_4C4C,
                    (_, true) => 0x50FF_FFFF,
                    _ => 0x66_00_00_00,
                };
                c.fill_circle(b.x + b.w / 2, b.y + b.h / 2, b.w / 2, bg);
                res::draw_icon(c, glyph, 14, b.x + (b.w - 14) / 2, b.y + (b.h - 14) / 2);
            }
        }
        if clip.overlaps(&ui::with_shadow(body)) {
            ui::glass(c, blur, body, 22, if focused { 0xA826_1C2E } else { 0x9028_2030 });
            let saved = c.clip();
            c.set_clip(saved.intersect(&body));
            self.app.draw(c, body, blur, focused);
            if self.app.resizable() && self.restore.is_none() {
                // Three short diagonal strokes in the corner.
                let (gx, gy) = (body.right() - 8, body.bottom() - 8);
                for k in [4, 8, 12] {
                    c.line(gx - k, gy, gx, gy - k, 1, 0x50FF_FFFF);
                }
            }
            c.set_clip(saved);
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
