//! The apps of the desktop, the catalogue the launcher shows, and which app
//! opens which file.

mod about;
mod calculator;
mod calendar;
mod clock;
mod files;
mod monitor;
mod music;
mod notes;
mod photos;
mod picker;
mod prefs;
mod reminders;
pub mod store;
mod terminal;
mod textedit;
mod ticly;
mod ticly_ai;
mod ticly_do;
mod viewer;

use super::wm::App;
use crate::fs;
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

pub use files::Files;
pub use viewer::Viewer;

/// The built-in apps: id, name, icon, window kind.
pub const BUILT_IN: [(&str, &str, &str); 18] = [
    ("ticly-do", "ticly Do", "ticly-do"),
    ("ticly-ai", "ticly AI", "ticly-ai"),
    ("files", "File", "files"),
    ("textedit", "Editor di testo", "text-editor"),
    ("notes", "Note", "notes"),
    ("reminders", "Promemoria", "reminders"),
    ("calendar", "Calendario", "calendar"),
    ("clock", "Orologio", "clock"),
    ("calculator", "Calcolatrice", "calculator"),
    ("photos", "Foto", "photos"),
    ("music", "Musica", "music"),
    ("store", "Negozio", "store"),
    ("settings", "Impostazioni", "settings"),
    ("monitor", "Monitor attività", "activity"),
    ("about", "Info sistema", "info"),
    ("terminal", "Terminale", "terminal"),
    ("trash", "Cestino", "trash"),
    ("viewer", "Anteprima", "image-viewer"),
];

/// Opens an app by id.
pub fn open(id: &str) -> Option<Box<dyn App>> {
    Some(match id {
        "files" => Box::new(Files::new("sys:/home")),
        "terminal" => Box::new(terminal::Terminal::new()),
        "settings" => Box::new(prefs::Prefs::new()),
        "about" => Box::new(about::About::new()),
        "store" => Box::new(store::Store::new()),
        "calculator" => Box::new(calculator::Calculator::new()),
        "textedit" | "textedit-new" => Box::new(textedit::TextEdit::new(None)),
        "notes" => Box::new(notes::Notes::new()),
        "reminders" => Box::new(reminders::Reminders::new()),
        "calendar" => Box::new(calendar::Calendar::new()),
        "clock" => Box::new(clock::Clock::new()),
        "photos" => Box::new(photos::Photos::new()),
        "music" => Box::new(music::Music::new()),
        "monitor" => Box::new(monitor::Monitor::new()),
        "ticly-do" => Box::new(ticly_do::TiclyDo::new()),
        "ticly-ai" => Box::new(ticly_ai::TiclyAi::new()),
        "trash" => {
            if let Ok(p) = fs::abs("sys:/home/Cestino") {
                if !fs::exists(&p) {
                    let _ = fs::make_dir(&p);
                }
            }
            Box::new(Files::trash())
        }
        _ => return None,
    })
}

/// Does the app have one window at most?
pub fn single(id: &str) -> bool {
    !matches!(id, "files" | "textedit" | "textedit-new" | "viewer")
}

/// The window kind of an app id.
pub fn kind_of(id: &str) -> &str {
    match id {
        "launcher" => "launcher",
        id => id,
    }
}

/// The name shown for a window: the app's, for the built-in ones.
pub fn app_name(kind: &str, title: &str) -> String {
    match BUILT_IN.iter().find(|(id, _, _)| *id == kind) {
        Some((_, name, _)) => String::from(*name),
        None => String::from(title),
    }
}

/// A program's name from its file name.
pub fn program_name(file: &str) -> String {
    let stem = file.rsplit('/').next().unwrap_or(file);
    let stem = stem.strip_suffix(".app").unwrap_or(stem);
    match crate::store::installed(stem) {
        Some(info) => info.name,
        None => String::from(stem),
    }
}

/// What a launcher entry does.
#[derive(Clone)]
pub enum Launch {
    App(&'static str),
    /// An installed program with a window of its own.
    Spawn(String),
    /// A text program, typed into the terminal.
    Run(String),
}

pub struct Entry {
    pub name: String,
    pub icon: String,
    pub launch: Launch,
}

/// The icon of an installed package: its own picture, if it has one.
pub fn package_icon(id: &str) -> String {
    crate::store::icon(id).unwrap_or_else(|| String::from("window"))
}

/// Everything the launcher lists: the built-in apps and the installed
/// packages.
pub fn catalog() -> Vec<Entry> {
    let mut v: Vec<Entry> = BUILT_IN
        .iter()
        .filter(|(id, _, _)| *id != "viewer")
        .map(|&(id, name, icon)| Entry { name: name.to_string(), icon: icon.to_string(), launch: Launch::App(id) })
        .collect();
    for p in crate::store::installed_all() {
        let launch = if p.gui { Launch::Spawn(crate::store::program_path(&p)) } else { Launch::Run(p.open.clone()) };
        v.push(Entry { icon: package_icon(&p.id), name: p.name, launch });
    }
    v
}

/// How a file opens.
pub enum Opener {
    /// In a new window of this app.
    App(Box<dyn App>),
    /// A program with a window.
    Spawn(String),
    /// A text program, in the terminal.
    Run(String),
    None,
}

pub fn for_file(path: &str) -> Opener {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    let quoted = if path.contains(' ') { alloc::format!("\"{path}\"") } else { String::from(path) };
    match ext {
        "qoi" => Opener::App(Box::new(Viewer::new(path))),
        "wav" => Opener::App(Box::new(music::Music::with_file(path))),
        "txt" | "md" | "toml" | "json" | "rs" | "c" | "h" | "sh" | "py" | "ini" | "cfg" | "log" | "csv" | "html" | "css" | "js" => {
            Opener::App(Box::new(textedit::TextEdit::new(Some(path))))
        }
        "app" => {
            let stem = name.strip_suffix(".app").unwrap_or(&name);
            if crate::store::installed(stem).is_some_and(|p| p.gui) {
                Opener::Spawn(String::from(path))
            } else {
                Opener::Run(quoted)
            }
        }
        "exe" | "as" | "arc" | "" => Opener::Run(quoted),
        _ => Opener::None,
    }
}
