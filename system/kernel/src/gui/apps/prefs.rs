//! Impostazioni: name, keyboard, wallpaper and the first-start setup.

use crate::dev::input::{Key, KeyEvent};
use crate::dev::keymap::Layout;
use crate::gui::gfx::{Canvas, Rect};
use crate::gui::res;
use crate::gui::settings;
use crate::gui::text::{self, Font};
use crate::gui::ui::{self, TEXT, TEXT_DIM, TEXT_FAINT};
use crate::gui::wm::{Action, App};
use alloc::string::String;
use alloc::vec::Vec;

pub struct Prefs {
    name: String,
    editing: bool,
    thumbs: Vec<Canvas>,
    note: &'static str,
}

impl Prefs {
    pub fn new() -> Prefs {
        let thumbs = res::WALLPAPERS.iter().map(|(n, _)| res::wallpaper_thumb(n, 140, 88)).collect();
        Prefs { name: settings::get().name, editing: false, thumbs, note: "" }
    }

    fn field(area: Rect) -> Rect {
        Rect::new(area.x + 24, area.y + 48, 300, 46)
    }

    fn save_btn(area: Rect) -> Rect {
        Rect::new(area.x + 336, area.y + 52, 110, 38)
    }

    fn layout_btns(area: Rect) -> [Rect; 2] {
        [Rect::new(area.x + 24, area.y + 146, 150, 38), Rect::new(area.x + 184, area.y + 146, 150, 38)]
    }

    fn thumb(area: Rect, i: usize) -> Rect {
        Rect::new(area.x + 24 + i as i32 * 156, area.y + 238, 140, 88)
    }

    fn redo_btn(area: Rect) -> Rect {
        Rect::new(area.x + 24, area.y + 396, 300, 38)
    }

    fn save_name(&mut self) {
        let name = String::from(self.name.trim());
        settings::update(|s| s.name = name);
        self.editing = false;
        self.note = "Nome salvato";
    }
}

impl App for Prefs {
    fn title(&self) -> String {
        String::from("Impostazioni")
    }

    fn icon(&self) -> &'static str {
        "settings"
    }

    fn kind(&self) -> &'static str {
        "settings"
    }

    fn size(&self) -> (i32, i32) {
        (500, 460)
    }

    fn draw(&mut self, c: &mut Canvas, area: Rect, _blur: &Canvas, _focused: bool) {
        let s = settings::get();
        text::draw(c, Font::Strong, area.x + 24, area.y + 20, "Il tuo nome", TEXT);
        ui::text_field(c, Self::field(area), &self.name, "Nome", self.editing);
        ui::button(c, Self::save_btn(area), "Salva", true, false);

        text::draw(c, Font::Strong, area.x + 24, area.y + 118, "Tastiera", TEXT);
        for (i, b) in Self::layout_btns(area).iter().enumerate() {
            let sel = (i == 0) == (s.layout == Layout::It);
            ui::button(c, *b, if i == 0 { "Italiano" } else { "English (US)" }, sel, false);
        }

        text::draw(c, Font::Strong, area.x + 24, area.y + 210, "Sfondo", TEXT);
        for (i, (name, label)) in res::WALLPAPERS.iter().enumerate() {
            let r = Self::thumb(area, i);
            if *name == s.wallpaper {
                c.stroke_round(r.grow(3), 14, 2, 0xFFFF_FFFF);
            }
            c.blit_round_from(&self.thumbs[i], 0, 0, r, 12, 0);
            text::draw_centered(c, Font::Small, r.x + r.w / 2, r.bottom() + 8, label, TEXT_DIM);
        }

        text::draw(c, Font::Strong, area.x + 24, area.y + 368, "Avvio", TEXT);
        ui::button(c, Self::redo_btn(area), "Rifai la configurazione iniziale", false, false);
        if !self.note.is_empty() {
            text::draw(c, Font::Small, area.x + 340, area.y + 408, self.note, TEXT_FAINT);
        }
    }

    fn press(&mut self, x: i32, y: i32, _double: bool, area: Rect) -> Action {
        let (ax, ay) = (area.x + x, area.y + y);
        self.note = "";
        if Self::field(area).contains(ax, ay) {
            self.editing = true;
            return Action::Redraw;
        }
        if Self::save_btn(area).contains(ax, ay) {
            self.save_name();
            return Action::Redraw;
        }
        self.editing = false;
        for (i, b) in Self::layout_btns(area).iter().enumerate() {
            if b.contains(ax, ay) {
                settings::update(|s| s.layout = if i == 0 { Layout::It } else { Layout::Us });
                self.note = "Tastiera cambiata";
                return Action::Redraw;
            }
        }
        for (i, (name, _)) in res::WALLPAPERS.iter().enumerate() {
            if Self::thumb(area, i).contains(ax, ay) {
                settings::update(|s| s.wallpaper = String::from(*name));
                return Action::Wallpaper;
            }
        }
        if Self::redo_btn(area).contains(ax, ay) {
            settings::update(|s| s.setup_done = false);
            self.note = "Al prossimo avvio";
            return Action::Redraw;
        }
        Action::Redraw
    }

    fn key(&mut self, ev: KeyEvent) -> Action {
        if !self.editing {
            return Action::None;
        }
        if ev.key == Key::Enter {
            self.save_name();
        } else {
            ui::edit_text(&mut self.name, &ev, 32);
        }
        Action::Redraw
    }
}
