//! Impostazioni: name, keyboard, wallpaper, screen resolution, sound and
//! the first-start setup.

use crate::dev::input::{Key, KeyEvent};
use crate::dev::keymap::Layout;
use crate::gui::gfx::{Canvas, Rect};
use crate::gui::res;
use crate::gui::settings;
use crate::gui::text::{self, Font};
use crate::gui::ui::{self, TEXT, TEXT_DIM, TEXT_FAINT};
use crate::gui::wm::{Action, App};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use arcdk::DisplayMode;

/// Resolutions offered when the display has them, in this order.
const COMMON_MODES: &[(u32, u32)] =
    &[(1024, 768), (1280, 720), (1280, 800), (1366, 768), (1440, 900), (1600, 900), (1680, 1050), (1920, 1080), (1920, 1200), (2560, 1440)];
/// Where the right column starts.
const RIGHT: i32 = 492;

pub struct Prefs {
    name: String,
    editing: bool,
    thumbs: Vec<Canvas>,
    note: &'static str,
    /// Resolutions the display driver offers (empty without one).
    modes: Vec<DisplayMode>,
}

impl Prefs {
    pub fn new() -> Prefs {
        let thumbs = res::WALLPAPERS.iter().map(|(n, _)| res::wallpaper_thumb(n, 140, 88)).collect();
        let offered = crate::dev::display::modes();
        let modes =
            COMMON_MODES.iter().map(|&(width, height)| DisplayMode { width, height }).filter(|m| offered.contains(m)).take(9).collect();
        Prefs { name: settings::get().name, editing: false, thumbs, note: "", modes }
    }

    fn mode_btn(area: Rect, i: usize) -> Rect {
        Rect::new(area.x + RIGHT + (i as i32 % 3) * 124, area.y + 72 + (i as i32 / 3) * 46, 116, 38)
    }

    fn volume_btns(area: Rect) -> [Rect; 2] {
        [Rect::new(area.x + RIGHT, area.y + 280, 44, 38), Rect::new(area.x + RIGHT + 320, area.y + 280, 44, 38)]
    }

    fn volume_bar(area: Rect) -> Rect {
        Rect::new(area.x + RIGHT + 58, area.y + 296, 248, 6)
    }

    fn chime_btn(area: Rect) -> Rect {
        Rect::new(area.x + RIGHT, area.y + 334, 174, 38)
    }

    fn test_btn(area: Rect) -> Rect {
        Rect::new(area.x + RIGHT + 186, area.y + 334, 178, 38)
    }

    fn set_volume(v: i32) {
        crate::gui::change_volume(v.clamp(0, 100) as u32);
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
        (880, 460)
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

        // Screen
        let x = area.x + RIGHT;
        text::draw(c, Font::Strong, x, area.y + 20, "Schermo", TEXT);
        let (w, h) = crate::console::pixel_size().unwrap_or((0, 0));
        let current = DisplayMode { width: w as u32, height: h as u32 };
        if self.modes.is_empty() {
            text::draw(c, Font::Small, x, area.y + 48, &format!("{w} × {h}"), TEXT_DIM);
            text::draw(c, Font::Small, x, area.y + 72, "Risoluzione scelta dal firmware:", TEXT_FAINT);
            text::draw(c, Font::Small, x, area.y + 92, "nessun driver video la può cambiare.", TEXT_FAINT);
        } else {
            let name = crate::dev::display::controller().unwrap_or_default();
            text::draw(c, Font::Small, x, area.y + 48, &text::fit(Font::Small, &name, 360), TEXT_DIM);
            for (i, m) in self.modes.iter().enumerate() {
                let label = format!("{}×{}", m.width, m.height);
                ui::button(c, Self::mode_btn(area, i), &label, *m == current, false);
            }
        }

        // Sound
        text::draw(c, Font::Strong, x, area.y + 236, "Suono", TEXT);
        if crate::dev::audio::available() {
            let v = crate::dev::audio::volume() as i32;
            let [minus, plus] = Self::volume_btns(area);
            ui::button(c, minus, "–", false, false);
            ui::button(c, plus, "+", false, false);
            let bar = Self::volume_bar(area);
            c.fill_round(bar, 3, 0x40FF_FFFF);
            c.fill_round(Rect::new(bar.x, bar.y, bar.w * v / 100, bar.h), 3, 0xFFFF_FFFF);
            text::draw_centered(c, Font::Small, bar.x + bar.w / 2, bar.y - 22, &format!("Volume {v}%"), TEXT_DIM);
            let chime = if s.startup_sound { "Suono all'avvio: sì" } else { "Suono all'avvio: no" };
            ui::button(c, Self::chime_btn(area), chime, s.startup_sound, false);
            ui::button(c, Self::test_btn(area), "Prova", false, false);
        } else {
            text::draw(c, Font::Small, x, area.y + 264, "Nessuna scheda audio trovata.", TEXT_FAINT);
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
        for (i, m) in self.modes.iter().enumerate() {
            if Self::mode_btn(area, i).contains(ax, ay) {
                self.note = match crate::gui::change_resolution(*m) {
                    Ok(()) => "",
                    Err(_) => "Risoluzione non disponibile",
                };
                return Action::Redraw;
            }
        }
        if crate::dev::audio::available() {
            let [minus, plus] = Self::volume_btns(area);
            let v = crate::dev::audio::volume() as i32;
            if minus.contains(ax, ay) {
                Self::set_volume(v - 10);
                return Action::Redraw;
            }
            if plus.contains(ax, ay) {
                Self::set_volume(v + 10);
                return Action::Redraw;
            }
            let bar = Self::volume_bar(area).grow(12);
            if bar.contains(ax, ay) {
                let b = Self::volume_bar(area);
                Self::set_volume(((ax - b.x) * 100 / b.w + 2) / 5 * 5);
                return Action::Redraw;
            }
            if Self::chime_btn(area).contains(ax, ay) {
                settings::update(|s| s.startup_sound = !s.startup_sound);
                return Action::Redraw;
            }
            if Self::test_btn(area).contains(ax, ay) {
                let _ = crate::dev::audio::play(crate::dev::audio::tone(660, 300));
                return Action::None;
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
