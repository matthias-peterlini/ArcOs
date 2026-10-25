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
const GAP: i32 = 8;

/// What an app asks the desktop to do after an event.
pub enum Action {
    None,
    /// Draw the content again.
    Redraw,
    /// Type a command line into the terminal.
    Run(String),
    /// Show a picture.
    View(String),
    /// The wallpaper changed: draw everything again.
    Wallpaper,
}

/// The content of a window.
pub trait App {
    fn title(&self) -> String;
    fn icon(&self) -> &'static str;
    /// The size of the content area the app would like.
    fn size(&self) -> (i32, i32);
    /// Draws the content; `blur` is the blurred wallpaper (for glass).
    fn draw(&mut self, c: &mut Canvas, area: Rect, blur: &Canvas, focused: bool);
    fn press(&mut self, _x: i32, _y: i32, _double: bool, _area: Rect) -> Action {
        Action::None
    }
    fn key(&mut self, _ev: KeyEvent) -> Action {
        Action::None
    }
    /// Called ten times a second; true if the content changed.
    fn tick(&mut self) -> bool {
        false
    }
    /// A short name, to find an open window of the same kind.
    fn kind(&self) -> &'static str;
    /// Mouse wheel over the content.
    fn scroll(&mut self, _lines: i32) -> bool {
        false
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
}

pub enum Hit {
    Title,
    Minimize,
    Maximize,
    Close,
    Body(i32, i32),
}

impl Window {
    pub fn new(id: u32, app: Box<dyn App>, x: i32, y: i32) -> Window {
        let (w, h) = app.size();
        Window { id, app, rect: Rect::new(x, y, w, h + TITLE_H + GAP), minimized: false, restore: None }
    }

    pub fn title_rect(&self) -> Rect {
        Rect::new(self.rect.x, self.rect.y, self.rect.w, TITLE_H)
    }

    pub fn body(&self) -> Rect {
        Rect::new(self.rect.x, self.rect.y + TITLE_H + GAP, self.rect.w, self.rect.h - TITLE_H - GAP)
    }

    /// Minimise, maximise and close, right to left at the end of the title.
    fn buttons(&self) -> [Rect; 3] {
        let t = self.title_rect();
        let s = 26;
        let y = t.y + (TITLE_H - s) / 2;
        let x = t.right() - 12 - s;
        [Rect::new(x - 2 * (s + 8), y, s, s), Rect::new(x - (s + 8), y, s, s), Rect::new(x, y, s, s)]
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
            for (b, glyph) in self.buttons().iter().zip(["g-down", "g-max", "g-close"]) {
                c.fill_circle(b.x + b.w / 2, b.y + b.h / 2, b.w / 2, 0x66_00_00_00);
                res::draw_icon(c, glyph, 14, b.x + (b.w - 14) / 2, b.y + (b.h - 14) / 2);
            }
        }
        if clip.overlaps(&ui::with_shadow(body)) {
            ui::glass(c, blur, body, 22, if focused { 0xA826_1C2E } else { 0x9028_2030 });
            let saved = c.clip();
            c.set_clip(saved.intersect(&body));
            self.app.draw(c, body, blur, focused);
            c.set_clip(saved);
        }
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
}
