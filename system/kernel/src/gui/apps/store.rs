//! Negozio: the programs in `sys:/store`, installed with a click.

use crate::fs;
use crate::gui::gfx::{Canvas, Rect};
use crate::gui::res;
use crate::gui::text::{self, Font};
use crate::gui::ui::{self, TEXT, TEXT_DIM};
use crate::gui::wm::{Action, App};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

struct Package {
    name: String,
    file: String,
    about: String,
}

pub struct Store {
    packages: Vec<Package>,
    note: String,
}

fn installed(name: &str) -> bool {
    fs::abs(&format!("sys:/apps/{name}")).is_ok_and(|p| fs::exists(&p))
}

impl Store {
    pub fn new() -> Store {
        let index = fs::abs("sys:/store/index").and_then(|p| fs::read_file(&p)).unwrap_or_default();
        let packages = String::from_utf8_lossy(&index)
            .lines()
            .filter_map(|l| {
                let mut it = l.splitn(3, '\t');
                Some(Package {
                    name: it.next()?.trim().into(),
                    file: it.next()?.trim().into(),
                    about: it.next().unwrap_or("").trim().into(),
                })
            })
            .filter(|p| !p.name.is_empty())
            .collect();
        Store { packages, note: String::new() }
    }

    fn row(area: Rect, i: usize) -> Rect {
        Rect::new(area.x + 16, area.y + 64 + i as i32 * 84, area.w - 32, 74)
    }

    fn button(row: Rect) -> Rect {
        Rect::new(row.right() - 128, row.y + 18, 112, 38)
    }
}

impl App for Store {
    fn title(&self) -> String {
        String::from("Negozio")
    }

    fn icon(&self) -> &'static str {
        "store"
    }

    fn kind(&self) -> &'static str {
        "store"
    }

    fn size(&self) -> (i32, i32) {
        (540, 64 + 84 * (self.packages.len() as i32).max(2) + 56)
    }

    fn draw(&mut self, c: &mut Canvas, area: Rect, _blur: &Canvas, _focused: bool) {
        text::draw(c, Font::Heading, area.x + 20, area.y + 18, "Programmi da installare", TEXT);
        if self.packages.is_empty() {
            text::draw(c, Font::Body, area.x + 20, area.y + 70, "Il negozio è vuoto.", TEXT_DIM);
        }
        for (i, p) in self.packages.iter().enumerate() {
            let r = Self::row(area, i);
            c.fill_round(r, 16, 0x1FFF_FFFF);
            let icon = match p.name.as_str() {
                "lua" => "code",
                _ => "terminal",
            };
            res::draw_icon(c, icon, 48, r.x + 12, r.y + 13);
            text::draw(c, Font::Strong, r.x + 72, r.y + 16, &p.name, TEXT);
            let about = text::fit(Font::Small, &p.about, r.w - 72 - 150);
            text::draw(c, Font::Small, r.x + 72, r.y + 40, &about, TEXT_DIM);
            let done = installed(&p.name);
            ui::button(c, Self::button(r), if done { "Apri" } else { "Installa" }, !done, false);
        }
        if !self.note.is_empty() {
            text::draw(c, Font::Small, area.x + 20, area.bottom() - 34, &self.note, TEXT_DIM);
        }
    }

    fn press(&mut self, x: i32, y: i32, _double: bool, area: Rect) -> Action {
        let (ax, ay) = (area.x + x, area.y + y);
        for (i, p) in self.packages.iter().enumerate() {
            if !Self::button(Self::row(area, i)).contains(ax, ay) {
                continue;
            }
            if installed(&p.name) {
                let cmd = if p.name == "busybox" { String::from("ash") } else { p.name.clone() };
                return Action::Run(cmd);
            }
            let (from, to) = (fs::abs(&format!("sys:/store/{}", p.file)), fs::abs(&format!("sys:/apps/{}", p.name)));
            self.note = match (from, to) {
                (Ok(f), Ok(t)) => match fs::copy_file(&f, &t) {
                    Ok(()) => format!("{} installato: lo trovi tra le applicazioni.", p.name),
                    Err(e) => format!("{}: {e}", p.name),
                },
                _ => format!("{}: percorso non valido", p.name),
            };
            return Action::Redraw;
        }
        Action::None
    }
}
