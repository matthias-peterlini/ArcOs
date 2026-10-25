//! The store: programs on the system volume (`sys:/store`) that are
//! installed into `sys:/apps` with a click (the Negozio app) or with
//! `install` in the shell.
//!
//! Each package is a folder `sys:/store/<id>` with:
//!
//! - `info`: `key=value` lines (name, category, about, version, developer,
//!   type `native` or `linux`, gui `yes` for programs with a window, command
//!   for the terminal);
//! - `app`: the program;
//! - `icon.qoi` (optional): its icon, 128 pixels.
//!
//! Installing copies the program to `sys:/apps/<id>.app` (arcOS programs)
//! or `sys:/apps/<id>` (Linux ones), next to `<id>.info` and `<id>.qoi`.

use crate::fs::{self, Error, Path};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

const STORE: &str = "sys:/store";
const APPS: &str = "sys:/apps";

/// What a package's `info` says.
#[derive(Clone, Default)]
pub struct PackageInfo {
    pub id: String,
    pub name: String,
    pub category: String,
    pub about: String,
    pub version: String,
    pub developer: String,
    /// More about it, for its page in the store.
    pub details: String,
    /// An arcOS program (else a Linux one).
    pub native: bool,
    /// Has a window (else it runs in the terminal).
    pub gui: bool,
    /// What to type in the terminal to start it (text programs).
    pub command: String,
    /// Size of the program in bytes.
    pub size: u64,
}

pub fn parse_info(text: &str, id: &str) -> PackageInfo {
    let mut p = PackageInfo { id: id.to_string(), name: id.to_string(), native: true, ..Default::default() };
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        let v = v.trim().to_string();
        match k.trim() {
            "name" => p.name = v,
            "category" => p.category = v,
            "about" => p.about = v,
            "version" => p.version = v,
            "developer" => p.developer = v,
            "details" => p.details = v,
            "type" => p.native = v != "linux",
            "gui" => p.gui = v == "yes",
            "command" => p.command = v,
            _ => {}
        }
    }
    if p.command.is_empty() {
        p.command = id.to_string();
    }
    p
}

fn path(s: &str) -> Result<Path, Error> {
    fs::abs(s)
}

fn read_info(file: &str, id: &str) -> Option<PackageInfo> {
    let bytes = fs::read_file(&path(file).ok()?).ok()?;
    Some(parse_info(&String::from_utf8_lossy(&bytes), id))
}

/// The packages in the store.
pub fn available() -> Vec<PackageInfo> {
    let Ok(dir) = path(STORE) else { return Vec::new() };
    let mut v: Vec<PackageInfo> = fs::list_dir(&dir)
        .unwrap_or_default()
        .into_iter()
        .filter(|(_, st)| st.kind == arcfs::Kind::Dir)
        .filter_map(|(e, _)| {
            let mut p = read_info(&format!("{STORE}/{}/info", e.name), &e.name)?;
            p.size = fs::stat(&dir.join(&e.name).join("app")).map(|s| s.size).unwrap_or(0);
            Some(p)
        })
        .collect();
    v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    v
}

/// Where an installed package's program is.
pub fn program_path(p: &PackageInfo) -> String {
    if p.native {
        format!("{APPS}/{}.app", p.id)
    } else {
        format!("{APPS}/{}", p.id)
    }
}

/// The installed package `id`, if it is.
pub fn installed(id: &str) -> Option<PackageInfo> {
    read_info(&format!("{APPS}/{id}.info"), id)
}

/// Every installed package.
pub fn installed_all() -> Vec<PackageInfo> {
    let Ok(dir) = path(APPS) else { return Vec::new() };
    fs::list_dir(&dir).unwrap_or_default().iter().filter_map(|(e, _)| e.name.strip_suffix(".info")).filter_map(installed).collect()
}

/// The icon of a package: "@<file>" if it has a picture of its own.
pub fn icon(id: &str) -> Option<String> {
    let file = format!("{APPS}/{id}.qoi");
    if path(&file).is_ok_and(|p| fs::exists(&p)) {
        return Some(format!("@{file}"));
    }
    let file = format!("{STORE}/{id}/icon.qoi");
    path(&file).is_ok_and(|p| fs::exists(&p)).then(|| format!("@{file}"))
}

pub fn install(id: &str) -> Result<PackageInfo, Error> {
    let p = read_info(&format!("{STORE}/{id}/info"), id).ok_or(Error::NotFound)?;
    fs::copy_file(&path(&format!("{STORE}/{id}/app"))?, &path(&program_path(&p))?)?;
    fs::copy_file(&path(&format!("{STORE}/{id}/info"))?, &path(&format!("{APPS}/{id}.info"))?)?;
    if let Ok(icon) = path(&format!("{STORE}/{id}/icon.qoi")) {
        if fs::exists(&icon) {
            fs::copy_file(&icon, &path(&format!("{APPS}/{id}.qoi"))?)?;
        }
    }
    Ok(p)
}

pub fn remove(id: &str) -> Result<(), Error> {
    let p = installed(id).ok_or(Error::NotFound)?;
    for f in [program_path(&p), format!("{APPS}/{id}.info"), format!("{APPS}/{id}.qoi")] {
        if let Ok(f) = path(&f) {
            if fs::exists(&f) {
                fs::remove(&f)?;
            }
        }
    }
    Ok(())
}
