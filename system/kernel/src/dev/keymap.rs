//! Keyboard layouts: USB HID usage codes to characters.
//!
//! Drivers report physical keys as HID usages, so every keyboard (PS/2, USB)
//! shares these tables.

/// (usage, normal, shift, altgr)
type Entry = (u8, char, char, char);

const N: char = '\0';

/// Keys that are the same on every layout.
const COMMON: &[Entry] = &[(0x2C, ' ', ' ', ' '), (0x54, '/', '/', N), (0x55, '*', '*', N), (0x56, '-', '-', N), (0x57, '+', '+', N)];

const US: &[Entry] = &[
    (0x1E, '1', '!', N),
    (0x1F, '2', '@', N),
    (0x20, '3', '#', N),
    (0x21, '4', '$', N),
    (0x22, '5', '%', N),
    (0x23, '6', '^', N),
    (0x24, '7', '&', N),
    (0x25, '8', '*', N),
    (0x26, '9', '(', N),
    (0x27, '0', ')', N),
    (0x2D, '-', '_', N),
    (0x2E, '=', '+', N),
    (0x2F, '[', '{', N),
    (0x30, ']', '}', N),
    (0x31, '\\', '|', N),
    (0x32, '\\', '|', N),
    (0x33, ';', ':', N),
    (0x34, '\'', '"', N),
    (0x35, '`', '~', N),
    (0x36, ',', '<', N),
    (0x37, '.', '>', N),
    (0x38, '/', '?', N),
    (0x64, '\\', '|', N),
];

const IT: &[Entry] = &[
    (0x1E, '1', '!', N),
    (0x1F, '2', '"', N),
    (0x20, '3', '£', N),
    (0x21, '4', '$', N),
    (0x22, '5', '%', '€'),
    (0x23, '6', '&', N),
    (0x24, '7', '/', '{'),
    (0x25, '8', '(', '['),
    (0x26, '9', ')', ']'),
    (0x27, '0', '=', '}'),
    (0x2D, '\'', '?', '`'),
    (0x2E, 'ì', '^', '~'),
    (0x2F, 'è', 'é', '['),
    (0x30, '+', '*', ']'),
    (0x31, 'ù', '§', N),
    (0x32, 'ù', '§', N),
    (0x33, 'ò', 'ç', '@'),
    (0x34, 'à', '°', '#'),
    (0x35, '\\', '|', N),
    (0x36, ',', ';', N),
    (0x37, '.', ':', N),
    (0x38, '-', '_', N),
    (0x64, '<', '>', N),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layout {
    Us,
    It,
}

impl Layout {
    pub fn from_name(name: &str) -> Option<Layout> {
        match name {
            "us" | "en" => Some(Layout::Us),
            "it" => Some(Layout::It),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Layout::Us => "us",
            Layout::It => "it",
        }
    }

    pub const ALL: [Layout; 2] = [Layout::Us, Layout::It];
}

/// The character a key produces, if any.
pub fn translate(layout: Layout, usage: u8, shift: bool, altgr: bool, caps: bool) -> Option<char> {
    if (0x04..=0x1D).contains(&usage) {
        let base = (b'a' + usage - 0x04) as char;
        if altgr {
            return match (layout, base) {
                (Layout::It, 'e') => Some('€'),
                _ => None,
            };
        }
        return Some(if shift ^ caps { base.to_ascii_uppercase() } else { base });
    }
    if (0x59..=0x62).contains(&usage) {
        // Keypad digits (always as digits: there is no Num Lock state).
        return Some(if usage == 0x62 { '0' } else { (b'1' + usage - 0x59) as char });
    }
    if usage == 0x63 {
        return Some('.');
    }
    let table = match layout {
        Layout::Us => US,
        Layout::It => IT,
    };
    let &(_, normal, shifted, alt) = table.iter().chain(COMMON).find(|e| e.0 == usage)?;
    let c = if altgr {
        alt
    } else if shift {
        shifted
    } else {
        normal
    };
    (c != N).then_some(c)
}
