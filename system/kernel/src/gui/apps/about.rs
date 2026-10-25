//! Info sistema: what this computer is and how arcOS is doing.

use crate::gui::gfx::{Canvas, Rect};
use crate::gui::splash::draw_logo;
use crate::gui::text::{self, Font};
use crate::gui::ui::{TEXT, TEXT_DIM};
use crate::gui::wm::App;
use crate::util::Size;
use alloc::format;
use alloc::string::{String, ToString};

pub struct About;

impl App for About {
    fn title(&self) -> String {
        String::from("Info sistema")
    }

    fn icon(&self) -> &'static str {
        "info"
    }

    fn kind(&self) -> &'static str {
        "about"
    }

    fn size(&self) -> (i32, i32) {
        (440, 370)
    }

    fn draw(&mut self, c: &mut Canvas, area: Rect, _blur: &Canvas, _focused: bool) {
        let cx = area.x + area.w / 2;
        draw_logo(c, cx, area.y + 44, 28, 0xFFFF_FFFF);
        text::draw_centered(c, Font::Title, cx, area.y + 70, "arcOS", TEXT);
        text::draw_centered(c, Font::Body, cx, area.y + 112, &format!("Versione {}", crate::VERSION), TEXT_DIM);
        let mut brand = [0u8; 48];
        let cpu = crate::arch::cpu::brand(&mut brand).trim().to_string();
        let (free, total) = crate::mem::frame::stats();
        let up = crate::time::uptime_ms() / 1000;
        let (sw, sh) = crate::console::pixel_size().unwrap_or((0, 0));
        let rows = [
            ("Processore", cpu),
            ("Memoria", format!("{} libera su {}", Size(free), Size(total))),
            ("Schermo", format!("{sw} x {sh}")),
            ("Acceso da", format!("{} h {} min {} s", up / 3600, up / 60 % 60, up % 60)),
            ("Dispositivi", format!("{}", crate::dev::list().len())),
            ("Programmi attivi", format!("{}", crate::user::count())),
        ];
        let mut y = area.y + 150;
        for (k, v) in rows {
            text::draw(c, Font::Body, area.x + 32, y, k, TEXT_DIM);
            let v = text::fit(Font::Strong, &v, area.w - 200);
            let w = text::width(Font::Strong, &v);
            text::draw(c, Font::Strong, area.right() - 32 - w, y, &v, TEXT);
            y += 30;
        }
    }

    fn tick(&mut self) -> bool {
        // The uptime changes every second.
        crate::time::uptime_ms() % 1000 < 100
    }
}
