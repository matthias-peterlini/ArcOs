//! Small helpers shared across the kernel.

use core::fmt;

/// Formats a byte count as "512 B", "4.0 KiB", "1.5 GiB".
pub struct Size(pub u64);

impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const UNITS: [&str; 5] = ["KiB", "MiB", "GiB", "TiB", "PiB"];
        if self.0 < 1024 {
            return write!(f, "{} B", self.0);
        }
        let mut unit = 0;
        let mut div = 1024u64;
        while self.0 / div >= 1024 && unit < UNITS.len() - 1 {
            div *= 1024;
            unit += 1;
        }
        let tenths = (self.0 as u128 * 10 + div as u128 / 2) / div as u128;
        write!(f, "{}.{} {}", tenths / 10, tenths % 10, UNITS[unit])
    }
}
