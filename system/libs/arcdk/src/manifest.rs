//! Driver manifests.
//!
//! A manifest is a small text file of `key = value` lines (`#` starts a
//! comment). It says what a driver is and which hardware it drives, so the
//! kernel can pick and install the right driver for a device without loading
//! any driver code:
//!
//! ```text
//! name        = ahci
//! version     = 1.0.0
//! class       = storage
//! description = AHCI SATA controllers
//! boot        = yes                 # needed to reach the system disk
//! match       = pci-class 01:06:01  # class:subclass[:prog-if]
//! match       = pci 8086:2922       # vendor:device
//! match       = platform ps2-keyboard
//! ```
//!
//! Keys can repeat (`match`); unknown keys are ignored so newer manifests
//! still load on older kernels.

use crate::abi::{DeviceInfo, BUS_PCI, BUS_PLATFORM};

/// A hardware matching rule (`match = ...`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchRule<'a> {
    /// `pci VVVV:DDDD`
    PciId { vendor: u16, device: u16 },
    /// `pci-class CC[:SS[:PP]]`
    PciClass { class: u8, subclass: Option<u8>, prog_if: Option<u8> },
    /// `platform <id>`
    Platform(&'a str),
}

impl<'a> MatchRule<'a> {
    pub fn parse(text: &'a str) -> Result<MatchRule<'a>, &'static str> {
        let text = text.trim();
        let (kind, rest) = text.split_once(char::is_whitespace).ok_or("match: expected '<kind> <value>'")?;
        let rest = rest.trim();
        match kind {
            "pci" => {
                let (v, d) = rest.split_once(':').ok_or("match pci: expected vendor:device")?;
                let vendor = u16::from_str_radix(v.trim(), 16).map_err(|_| "match pci: bad vendor id")?;
                let device = u16::from_str_radix(d.trim(), 16).map_err(|_| "match pci: bad device id")?;
                Ok(MatchRule::PciId { vendor, device })
            }
            "pci-class" => {
                let mut parts = rest.split(':');
                let hex = |s: Option<&str>| -> Result<Option<u8>, &'static str> {
                    match s {
                        None => Ok(None),
                        Some(s) => u8::from_str_radix(s.trim(), 16).map(Some).map_err(|_| "match pci-class: bad number"),
                    }
                };
                let class = hex(parts.next())?.ok_or("match pci-class: missing class")?;
                let subclass = hex(parts.next())?;
                let prog_if = hex(parts.next())?;
                if parts.next().is_some() {
                    return Err("match pci-class: too many fields");
                }
                Ok(MatchRule::PciClass { class, subclass, prog_if })
            }
            "platform" => {
                if rest.is_empty() || rest.len() > 31 {
                    return Err("match platform: bad id");
                }
                Ok(MatchRule::Platform(rest))
            }
            _ => Err("match: unknown kind (pci, pci-class, platform)"),
        }
    }

    /// How well the rule fits a device: `None` if it does not, otherwise a
    /// score where more specific rules win (an exact id beats a class).
    pub fn score(&self, dev: &DeviceInfo) -> Option<u32> {
        match *self {
            MatchRule::PciId { vendor, device } => (dev.bus == BUS_PCI && dev.vendor == vendor && dev.device == device).then_some(100),
            MatchRule::PciClass { class, subclass, prog_if } => {
                if dev.bus != BUS_PCI || dev.class != class {
                    return None;
                }
                let mut score = 20;
                if let Some(s) = subclass {
                    if dev.subclass != s {
                        return None;
                    }
                    score = 40;
                }
                if let Some(p) = prog_if {
                    if dev.prog_if != p {
                        return None;
                    }
                    score = 60;
                }
                Some(score)
            }
            MatchRule::Platform(id) => (dev.bus == BUS_PLATFORM && dev.name() == id).then_some(100),
        }
    }
}

/// A parsed view over manifest text.
#[derive(Clone, Copy)]
pub struct Manifest<'a> {
    text: &'a str,
}

impl<'a> Manifest<'a> {
    /// Wraps and validates manifest text.
    pub fn parse(text: &'a str) -> Result<Manifest<'a>, &'static str> {
        let m = Manifest { text };
        for (key, value) in m.entries() {
            if key.is_empty() {
                return Err("manifest: line without a key");
            }
            if key == "match" {
                MatchRule::parse(value)?;
            }
        }
        let name = m.get("name").ok_or("manifest: missing 'name'")?;
        if name.is_empty() || name.len() > 31 || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
            return Err("manifest: 'name' must be 1-31 characters of a-z, 0-9, '-' or '_'");
        }
        m.get("version").ok_or("manifest: missing 'version'")?;
        Ok(m)
    }

    pub fn text(&self) -> &'a str {
        self.text
    }

    /// Every `key = value` pair, in order.
    pub fn entries(&self) -> impl Iterator<Item = (&'a str, &'a str)> + 'a {
        self.text.lines().filter_map(|line| {
            let line = match line.find('#') {
                Some(i) => &line[..i],
                None => line,
            }
            .trim();
            if line.is_empty() {
                return None;
            }
            match line.split_once('=') {
                Some((k, v)) => Some((k.trim(), v.trim())),
                None => Some(("", line)),
            }
        })
    }

    /// The first value of `key`.
    pub fn get(&self, key: &str) -> Option<&'a str> {
        self.entries().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    pub fn name(&self) -> &'a str {
        self.get("name").unwrap_or("")
    }

    pub fn version(&self) -> &'a str {
        self.get("version").unwrap_or("")
    }

    pub fn description(&self) -> &'a str {
        self.get("description").unwrap_or("")
    }

    pub fn class(&self) -> &'a str {
        self.get("class").unwrap_or("other")
    }

    /// `boot = yes`: the driver is needed before the system disk is mounted.
    pub fn is_boot(&self) -> bool {
        matches!(self.get("boot"), Some("yes" | "true" | "1"))
    }

    pub fn rules(&self) -> impl Iterator<Item = MatchRule<'a>> + 'a {
        self.entries().filter(|(k, _)| *k == "match").filter_map(|(_, v)| MatchRule::parse(v).ok())
    }

    /// The best score of any rule for `dev`, or `None` if no rule matches.
    pub fn score(&self, dev: &DeviceInfo) -> Option<u32> {
        self.rules().filter_map(|r| r.score(dev)).max()
    }
}

/// Compares two `major.minor.patch` versions numerically.
pub fn compare_versions(a: &str, b: &str) -> core::cmp::Ordering {
    let mut a = a.split('.');
    let mut b = b.split('.');
    loop {
        match (a.next(), b.next()) {
            (None, None) => return core::cmp::Ordering::Equal,
            (x, y) => {
                let x: u64 = x.and_then(|s| s.trim().parse().ok()).unwrap_or(0);
                let y: u64 = y.and_then(|s| s.trim().parse().ok()).unwrap_or(0);
                if x != y {
                    return x.cmp(&y);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "name = ahci\nversion = 1.2.0 # comment\nmatch = pci-class 01:06:01\nmatch = pci 8086:2922\n";

    #[test]
    fn parses_and_scores() {
        let m = Manifest::parse(TEXT).unwrap();
        assert_eq!(m.name(), "ahci");
        assert_eq!(m.version(), "1.2.0");
        let mut dev = DeviceInfo::empty();
        dev.bus = BUS_PCI;
        dev.class = 1;
        dev.subclass = 6;
        dev.prog_if = 1;
        assert_eq!(m.score(&dev), Some(60));
        dev.vendor = 0x8086;
        dev.device = 0x2922;
        assert_eq!(m.score(&dev), Some(100));
        dev.class = 2;
        dev.vendor = 0;
        assert_eq!(m.score(&dev), None);
    }

    #[test]
    fn rejects_bad_manifests() {
        assert!(Manifest::parse("version = 1").is_err());
        assert!(Manifest::parse("name = a b\nversion = 1").is_err());
        assert!(Manifest::parse("name = x\nversion = 1\nmatch = pci zz:00").is_err());
    }

    #[test]
    fn versions() {
        use core::cmp::Ordering::*;
        assert_eq!(compare_versions("1.10.0", "1.9.3"), Greater);
        assert_eq!(compare_versions("1.0", "1.0.0"), Equal);
        assert_eq!(compare_versions("0.9", "1"), Less);
    }
}
