//! File: the file manager. Places on the left, the folder on the right.

use crate::fs::{self, Path};
use crate::gui::gfx::{Canvas, Rect};
use crate::gui::res;
use crate::gui::text::{self, Font};
use crate::gui::ui::{TEXT, TEXT_DIM, TEXT_FAINT};
use crate::gui::wm::{Action, App};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

const SIDEBAR: i32 = 180;
const TILE_W: i32 = 104;
const TILE_H: i32 = 104;
const HEADER: i32 = 52;

const PLACES: [(&str, &str); 9] = [
    ("Home", "sys:/home"),
    ("Scrivania", "sys:/home/Scrivania"),
    ("Documenti", "sys:/home/Documenti"),
    ("Scaricati", "sys:/home/Scaricati"),
    ("Immagini", "sys:/home/Immagini"),
    ("Musica", "sys:/home/Musica"),
    ("Video", "sys:/home/Video"),
    ("Cestino", "sys:/home/Cestino"),
    ("Sistema", "sys:/"),
];

struct Item {
    name: String,
    dir: bool,
}

pub struct Files {
    dir: Path,
    items: Vec<Item>,
    selected: Option<usize>,
    scroll: i32,
}

/// The icon of a file or folder.
pub fn icon_for(name: &str, dir: bool) -> &'static str {
    if dir {
        return match name {
            "Documenti" => "folder-documents",
            "Scaricati" => "folder-downloads",
            "Immagini" => "folder-pictures",
            "Musica" => "folder-music",
            "Video" => "folder-videos",
            "home" => "folder-home",
            "Scrivania" => "folder-desktop",
            "Cestino" => "trash",
            _ => "folder",
        };
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "pdf" => "file-pdf",
        "jpg" | "jpeg" | "png" | "qoi" | "bmp" | "gif" => "file-image",
        "txt" | "md" | "odt" | "doc" | "docx" | "rtf" => "file-text",
        "zip" | "tar" | "gz" | "xz" | "adp" | "7z" => "file-archive",
        "py" | "rs" | "c" | "h" | "js" | "sh" | "toml" | "json" | "html" | "css" => "file-code",
        "mp3" | "wav" | "ogg" | "flac" => "file-audio",
        "as" | "arc" => "file-script",
        "exe" => "file-exe",
        "app" => "window",
        _ => "file",
    }
}

impl Files {
    pub fn new(path: &str) -> Files {
        let mut f = Files { dir: fs::abs(path).unwrap_or_else(|_| Path::root("sys")), items: Vec::new(), selected: None, scroll: 0 };
        f.reload();
        f
    }

    fn reload(&mut self) {
        self.items = fs::list_dir(&self.dir)
            .map(|list| list.into_iter().map(|(e, st)| Item { name: e.name, dir: st.kind == arcfs::Kind::Dir }).collect())
            .unwrap_or_default();
        self.selected = None;
        self.scroll = 0;
    }

    fn go(&mut self, path: Path) {
        if fs::is_dir(&path) {
            self.dir = path;
            self.reload();
        }
    }

    fn content(area: Rect) -> Rect {
        Rect::new(area.x + SIDEBAR + 8, area.y + 10, area.w - SIDEBAR - 18, area.h - 20)
    }

    fn grid(area: Rect) -> (Rect, i32) {
        let c = Self::content(area);
        let g = Rect::new(c.x + 12, c.y + HEADER, c.w - 24, c.h - HEADER - 8);
        let cols = (g.w / TILE_W).max(1);
        (g, cols)
    }

    fn open_item(&mut self, i: usize) -> Action {
        let item = &self.items[i];
        let path = self.dir.join(&item.name);
        if item.dir {
            self.go(path);
            return Action::Redraw;
        }
        let name = item.name.to_ascii_lowercase();
        let quoted = format!("{path}");
        if name.ends_with(".qoi") {
            return Action::View(quoted);
        }
        let program =
            name.ends_with(".app") || name.ends_with(".exe") || name.ends_with(".as") || name.ends_with(".arc") || !name.contains('.');
        if program {
            return Action::Run(quoted);
        }
        if [".txt", ".md", ".toml", ".json", ".rs", ".c", ".sh", ".py"].iter().any(|e| name.ends_with(e)) {
            return Action::Run(format!("edit {quoted}"));
        }
        Action::None
    }

    fn title_of(&self) -> String {
        match PLACES.iter().find(|(_, p)| fs::abs(p).is_ok_and(|q| q == self.dir)) {
            Some((n, _)) => String::from(*n),
            None => {
                let n = self.dir.name();
                if n.is_empty() {
                    format!("{}:", self.dir.volume)
                } else {
                    String::from(n)
                }
            }
        }
    }
}

impl App for Files {
    fn title(&self) -> String {
        format!("File — {}", self.title_of())
    }

    fn icon(&self) -> &'static str {
        "files"
    }

    fn kind(&self) -> &'static str {
        "files"
    }

    fn size(&self) -> (i32, i32) {
        (SIDEBAR + 18 + 24 + TILE_W * 4 + 8, 430)
    }

    fn draw(&mut self, c: &mut Canvas, area: Rect, _blur: &Canvas, _focused: bool) {
        // Places.
        text::draw(c, Font::Small, area.x + 22, area.y + 18, "Risorse", TEXT_FAINT);
        for (i, (name, path)) in PLACES.iter().enumerate() {
            let r = Rect::new(area.x + 10, area.y + 40 + i as i32 * 36, SIDEBAR - 12, 32);
            if fs::abs(path).is_ok_and(|p| p == self.dir) {
                c.fill_round(r, 10, 0x2EFF_FFFF);
            }
            let icon = if *name == "Home" {
                "folder-home"
            } else if *name == "Sistema" {
                "computer"
            } else {
                icon_for(name, true)
            };
            res::draw_icon(c, icon, 20, r.x + 10, r.y + 6);
            text::draw(c, Font::Body, r.x + 40, r.y + 7, name, TEXT);
        }
        // The folder.
        let content = Self::content(area);
        c.fill_round(content, 16, 0xE023_1A2B);
        let back = Rect::new(content.x + 12, content.y + 12, 28, 28);
        c.fill_circle(back.x + 14, back.y + 14, 14, 0x26FF_FFFF);
        res::draw_icon(c, "g-back", 14, back.x + 7, back.y + 7);
        text::draw(c, Font::Heading, content.x + 52, content.y + 14, &self.title_of(), TEXT);
        let count = format!("{} elementi", self.items.len());
        let cw = text::width(Font::Small, &count);
        text::draw(c, Font::Small, content.right() - cw - 16, content.y + 20, &count, TEXT_DIM);
        let (grid, cols) = Self::grid(area);
        let saved = c.clip();
        c.set_clip(saved.intersect(&grid));
        if self.items.is_empty() {
            text::draw_centered(c, Font::Body, grid.x + grid.w / 2, grid.y + 60, "La cartella è vuota", TEXT_DIM);
        }
        for (i, item) in self.items.iter().enumerate() {
            let (col, row) = (i as i32 % cols, i as i32 / cols);
            let t = Rect::new(grid.x + col * TILE_W, grid.y + row * TILE_H - self.scroll, TILE_W, TILE_H);
            if t.bottom() < grid.y || t.y > grid.bottom() {
                continue;
            }
            if self.selected == Some(i) {
                c.fill_round(t.grow(-4), 14, 0x33FF_FFFF);
            }
            res::draw_icon(c, icon_for(&item.name, item.dir), 52, t.x + (TILE_W - 52) / 2, t.y + 10);
            let label = text::fit(Font::Small, &item.name, TILE_W - 10);
            text::draw_centered(c, Font::Small, t.x + TILE_W / 2, t.y + 70, &label, TEXT);
        }
        c.set_clip(saved);
    }

    fn press(&mut self, x: i32, y: i32, double: bool, area: Rect) -> Action {
        let (ax, ay) = (area.x + x, area.y + y);
        for (i, (_, path)) in PLACES.iter().enumerate() {
            let r = Rect::new(area.x + 10, area.y + 40 + i as i32 * 36, SIDEBAR - 12, 32);
            if r.contains(ax, ay) {
                if let Ok(p) = fs::abs(path) {
                    if !fs::exists(&p) {
                        let _ = fs::make_dir(&p);
                    }
                    self.go(p);
                }
                return Action::Redraw;
            }
        }
        let content = Self::content(area);
        let back = Rect::new(content.x + 12, content.y + 12, 28, 28);
        if back.contains(ax, ay) {
            let parent = Path::parse("..", &self.dir).unwrap_or_else(|_| self.dir.clone());
            self.go(parent);
            return Action::Redraw;
        }
        let (grid, cols) = Self::grid(area);
        if grid.contains(ax, ay) {
            let col = (ax - grid.x) / TILE_W;
            let row = (ay - grid.y + self.scroll) / TILE_H;
            let i = (row * cols + col) as usize;
            if col < cols && i < self.items.len() {
                self.selected = Some(i);
                if double {
                    return self.open_item(i);
                }
            } else {
                self.selected = None;
            }
            return Action::Redraw;
        }
        Action::None
    }

    fn key(&mut self, ev: crate::dev::input::KeyEvent) -> Action {
        use crate::dev::input::Key;
        let n = self.items.len();
        match ev.key {
            Key::Enter => match self.selected {
                Some(i) => self.open_item(i),
                None => Action::None,
            },
            Key::Backspace => {
                let parent = Path::parse("..", &self.dir).unwrap_or_else(|_| self.dir.clone());
                self.go(parent);
                Action::Redraw
            }
            Key::Right | Key::Down if n > 0 => {
                self.selected = Some(self.selected.map_or(0, |i| (i + 1).min(n - 1)));
                Action::Redraw
            }
            Key::Left | Key::Up if n > 0 => {
                self.selected = Some(self.selected.map_or(0, |i| i.saturating_sub(1)));
                Action::Redraw
            }
            _ => Action::None,
        }
    }

    fn scroll(&mut self, lines: i32) -> bool {
        let rows = (self.items.len() as i32 + 3) / 4;
        let max = (rows * TILE_H - 300).max(0);
        let old = self.scroll;
        self.scroll = (self.scroll + lines * 40).clamp(0, max);
        old != self.scroll
    }
}
