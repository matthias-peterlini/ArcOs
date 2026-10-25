//! The apps of the desktop, and the catalogue the launcher shows.

mod about;
mod files;
mod prefs;
mod store;
mod terminal;
mod viewer;

use super::wm::App;
use crate::fs;
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

pub use files::Files;
pub use viewer::Viewer;

/// Opens an app by id.
pub fn open(id: &str) -> Option<Box<dyn App>> {
    Some(match id {
        "files" => Box::new(Files::new("sys:/home")),
        "terminal" => Box::new(terminal::Terminal::new()),
        "settings" => Box::new(prefs::Prefs::new()),
        "about" => Box::new(about::About),
        "store" => Box::new(store::Store::new()),
        "trash" => {
            if let Ok(p) = fs::abs("sys:/home/Cestino") {
                if !fs::exists(&p) {
                    let _ = fs::make_dir(&p);
                }
            }
            Box::new(Files::new("sys:/home/Cestino"))
        }
        _ => return None,
    })
}

/// What a launcher entry does.
#[derive(Clone)]
pub enum Launch {
    App(&'static str),
    /// A text program, typed into the terminal.
    Run(String),
}

pub struct Entry {
    pub name: String,
    pub icon: &'static str,
    pub launch: Launch,
}

fn entry(name: &str, icon: &'static str, launch: Launch) -> Entry {
    Entry { name: name.to_string(), icon, launch }
}

/// Everything the launcher lists: the built-in apps, the text programs and
/// whatever is installed in `sys:/apps`.
pub fn catalog() -> Vec<Entry> {
    let mut v = alloc::vec![
        entry("File", "files", Launch::App("files")),
        entry("Terminale", "terminal", Launch::App("terminal")),
        entry("Impostazioni", "settings", Launch::App("settings")),
        entry("Negozio", "store", Launch::App("store")),
        entry("Info sistema", "info", Launch::App("about")),
        entry("Editor di testo", "text-editor", Launch::Run("edit".into())),
        entry("Calcolatrice", "calculator", Launch::Run("calc".into())),
        entry("Note", "music", Launch::Run("notes list".into())),
        entry("Snake", "games", Launch::Run("snake".into())),
        entry("Monitor di sistema", "activity", Launch::Run("ps".into())),
        entry("Dispositivi", "computer", Launch::Run("dev".into())),
        entry("Cestino", "trash", Launch::App("trash")),
    ];
    v[7].icon = "text-editor";
    let known = ["edit", "calc", "snake", "notes", "info", "hello", "arcc", "winrun"];
    if let Ok(list) = fs::abs("sys:/apps").and_then(|p| fs::list_dir(&p)) {
        for (e, _) in list {
            let name = e.name.clone();
            let (stem, icon) = if let Some(s) = name.strip_suffix(".exe") {
                (s, "window")
            } else if let Some(s) = name.strip_suffix(".as").or_else(|| name.strip_suffix(".arc")) {
                (s, "code")
            } else if name.ends_with(".app") {
                continue;
            } else {
                (name.as_str(), "terminal")
            };
            if known.contains(&stem) {
                continue;
            }
            let (label, cmd, icon) = match stem {
                "busybox" => (String::from("Shell BusyBox"), String::from("ash"), "terminal"),
                "lua" => (String::from("Lua"), String::from("lua"), "code"),
                s => (String::from(s), String::from(s), icon),
            };
            v.push(Entry { name: label, icon, launch: Launch::Run(cmd) });
        }
    }
    v
}
