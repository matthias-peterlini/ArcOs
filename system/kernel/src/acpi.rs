//! Just enough ACPI to find the tables, to power off or restart, and to
//! turn the computer off cleanly when its power button is pressed.

use crate::arch::cpu::{self, inw, outb, outw};
use crate::mem::phys_to_virt;
use crate::sync::IrqLock;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Copy, Default)]
struct Fadt {
    smi_cmd: u32,
    acpi_enable: u8,
    pm1a_cnt: u32,
    pm1b_cnt: u32,
    dsdt: u64,
    reset_space: u8,
    reset_addr: u64,
    reset_value: u8,
    reset_supported: bool,
    /// Interrupt line of the SCI (System Control Interrupt).
    sci: u16,
    pm1a_evt: u32,
    pm1b_evt: u32,
    pm1_evt_len: u8,
    /// The power button is a fixed feature (not a control method one).
    fixed_power_button: bool,
}

struct Acpi {
    tables: Vec<([u8; 4], u64)>,
    fadt: Option<Fadt>,
    oem: String,
}

static ACPI: IrqLock<Acpi> = IrqLock::new(Acpi { tables: Vec::new(), fadt: None, oem: String::new() });

fn bytes(phys: u64, len: usize) -> &'static [u8] {
    // SAFETY: ACPI tables live in RAM or reserved memory, both reachable
    // through the direct map.
    unsafe { core::slice::from_raw_parts(phys_to_virt(phys) as *const u8, len) }
}

fn le16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn le32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn le64(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

fn checksum_ok(b: &[u8]) -> bool {
    b.iter().fold(0u8, |a, &x| a.wrapping_add(x)) == 0
}

fn find_rsdp() -> Option<u64> {
    let scan = |start: u64, len: u64| -> Option<u64> {
        (start..start + len).step_by(16).find(|&a| bytes(a, 8) == b"RSD PTR " && checksum_ok(bytes(a, 20)))
    };
    let ebda = (le16(bytes(0x40E, 2), 0) as u64) << 4;
    if ebda != 0 {
        if let Some(a) = scan(ebda, 1024) {
            return Some(a);
        }
    }
    scan(0xE0000, 0x20000)
}

fn table(phys: u64) -> Option<&'static [u8]> {
    let header = bytes(phys, 36);
    let len = le32(header, 4) as usize;
    if !(36..=1 << 20).contains(&len) {
        return None;
    }
    let t = bytes(phys, len);
    checksum_ok(t).then_some(t)
}

pub fn init(rsdp_hint: u64) {
    let Some(rsdp) = (if rsdp_hint != 0 { Some(rsdp_hint) } else { find_rsdp() }) else {
        crate::warn!("acpi", "no ACPI tables found");
        return;
    };
    let r = bytes(rsdp, 36);
    let revision = r[15];
    let oem = String::from_utf8_lossy(&r[9..15]).trim().into();
    let (root, entry_size) = if revision >= 2 && le64(r, 24) != 0 { (le64(r, 24), 8) } else { (le32(r, 16) as u64, 4) };
    let Some(root_table) = table(root) else {
        crate::warn!("acpi", "the root table is damaged");
        return;
    };
    let mut acpi = ACPI.lock();
    acpi.oem = oem;
    let count = (root_table.len() - 36) / entry_size;
    for i in 0..count {
        let at = 36 + i * entry_size;
        let phys = if entry_size == 8 { le64(root_table, at) } else { le32(root_table, at) as u64 };
        let Some(t) = table(phys) else { continue };
        let sig: [u8; 4] = t[..4].try_into().unwrap();
        acpi.tables.push((sig, phys));
        if &sig == b"FACP" && t.len() >= 116 {
            let mut f = Fadt {
                smi_cmd: le32(t, 48),
                acpi_enable: t[52],
                pm1a_cnt: le32(t, 64),
                pm1b_cnt: le32(t, 68),
                dsdt: le32(t, 40) as u64,
                ..Fadt::default()
            };
            f.sci = le16(t, 46);
            f.pm1a_evt = le32(t, 56);
            f.pm1b_evt = le32(t, 60);
            f.pm1_evt_len = t[88];
            f.fixed_power_button = le32(t, 112) & (1 << 4) == 0;
            if t.len() >= 129 {
                f.reset_supported = le32(t, 112) & (1 << 10) != 0;
                f.reset_space = t[116];
                f.reset_addr = le64(t, 120);
                f.reset_value = t[128];
            }
            if t.len() >= 148 && le64(t, 140) != 0 {
                f.dsdt = le64(t, 140);
            }
            acpi.fadt = Some(f);
        }
    }
    let names: Vec<&str> = acpi.tables.iter().map(|(s, _)| core::str::from_utf8(s).unwrap_or("?")).collect();
    crate::debug!("acpi", "revision {revision}, tables: {}", names.join(" "));
}

/// Signatures of the ACPI tables, with the OEM id.
pub fn summary() -> (String, Vec<String>) {
    let a = ACPI.lock();
    (a.oem.clone(), a.tables.iter().map(|(s, _)| String::from_utf8_lossy(s).into()).collect())
}

/// SLP_TYPa/b for the S5 (soft off) state from the DSDT's `_S5_` package.
fn s5_sleep_type(dsdt: u64) -> Option<(u16, u16)> {
    let t = table(dsdt)?;
    let at = t.windows(4).position(|w| w == b"_S5_")?;
    let mut i = at + 4;
    if *t.get(i)? != 0x12 {
        return None; // PackageOp
    }
    i += 1;
    i += ((t[i] >> 6) & 3) as usize + 1; // PkgLength
    i += 1; // NumElements
    let read = |i: &mut usize| -> Option<u16> {
        let v = match *t.get(*i)? {
            0x0A => {
                *i += 1;
                *t.get(*i)? as u16
            } // BytePrefix
            0x00 => 0,
            0x01 => 1,
            x => x as u16,
        };
        *i += 1;
        Some(v)
    };
    let a = read(&mut i)?;
    let b = read(&mut i).unwrap_or(0);
    Some((a, b))
}

/// Switches the chipset from legacy (SMI) to ACPI mode if the firmware left
/// it in the former.
fn enable_acpi_mode(f: &Fadt) {
    if f.pm1a_cnt == 0 || inw(f.pm1a_cnt as u16) & 1 != 0 || f.smi_cmd == 0 || f.acpi_enable == 0 {
        return;
    }
    outb(f.smi_cmd as u16, f.acpi_enable);
    for _ in 0..1_000_000 {
        if inw(f.pm1a_cnt as u16) & 1 != 0 {
            break;
        }
    }
}

/// PM1 status: the power button was pressed.
const PWRBTN: u16 = 1 << 8;

/// Set by the SCI when the power button was pressed.
static POWER_BUTTON: crate::task::Event = crate::task::Event::new();

/// The PM1 status and enable registers of the first and second block.
fn pm1_regs(f: &Fadt) -> impl Iterator<Item = (u16, u16)> {
    let half = (f.pm1_evt_len / 2) as u32;
    [f.pm1a_evt, f.pm1b_evt].into_iter().filter(|&b| b != 0 && b <= 0xFFFF).map(move |b| (b as u16, (b + half) as u16))
}

fn sci() {
    let Some(f) = ACPI.lock().fadt else { return };
    for (status, enable) in pm1_regs(&f) {
        let pending = inw(status) & inw(enable);
        if pending != 0 {
            outw(status, pending); // write 1 to clear
            if pending & PWRBTN != 0 {
                POWER_BUTTON.signal();
            }
        }
    }
}

/// The interrupt line of ACPI events (for the device list), if there is one.
pub fn sci_line() -> Option<u8> {
    ACPI.lock().fadt.filter(|f| f.fixed_power_button && f.pm1a_evt != 0 && f.sci < 16).map(|f| f.sci as u8)
}

/// Listens for the power button: pressing it turns the computer off cleanly
/// (the file systems are written out first).
pub fn start_events() {
    let Some(f) = ACPI.lock().fadt else { return };
    let Some(line) = sci_line() else { return };
    if f.pm1_evt_len < 4 {
        return;
    }
    enable_acpi_mode(&f);
    for (status, enable) in pm1_regs(&f) {
        outw(status, 0xFFFF);
        outw(enable, (inw(enable) | PWRBTN) & PWRBTN);
    }
    crate::dev::irq::attach_kernel(line, sci);
    crate::task::spawn("acpi", || loop {
        POWER_BUTTON.wait(None);
        crate::info!("acpi", "power button pressed: turning off");
        power_off();
    });
    crate::debug!("acpi", "power button on line {line}");
}

/// Turns the machine off. Returns only if every method failed.
pub fn power_off() {
    crate::fs::sync_all();
    cpu::disable_irq();
    let fadt = ACPI.lock().fadt;
    if let Some(f) = fadt {
        if f.pm1a_cnt != 0 {
            enable_acpi_mode(&f);
            if let Some((a, b)) = s5_sleep_type(f.dsdt) {
                outw(f.pm1a_cnt as u16, (a << 10) | (1 << 13));
                if f.pm1b_cnt != 0 {
                    outw(f.pm1b_cnt as u16, (b << 10) | (1 << 13));
                }
            }
        }
    }
    // Emulators: QEMU (new and old), VirtualBox.
    outw(0x604, 0x2000);
    outw(0xB004, 0x2000);
    outw(0x4004, 0x3400);
    // Power-off takes effect asynchronously; give it a moment.
    crate::time::spin_us(500_000);
}

/// Restarts the machine.
pub fn reboot() -> ! {
    crate::fs::sync_all();
    cpu::disable_irq();
    let fadt = ACPI.lock().fadt;
    if let Some(f) = fadt {
        if f.reset_supported && f.reset_addr != 0 {
            match f.reset_space {
                1 => outb(f.reset_addr as u16, f.reset_value),
                0 => {
                    if let Some(v) = crate::mem::region::map_device(f.reset_addr, 1, crate::mem::paging::NO_CACHE) {
                        // SAFETY: the FADT names this register for resets.
                        unsafe { (v as *mut u8).write_volatile(f.reset_value) };
                    }
                }
                _ => {}
            }
            crate::time::spin_us(50_000);
        }
    }
    // The keyboard controller's reset line.
    for _ in 0..10 {
        if cpu::inb(0x64) & 2 == 0 {
            break;
        }
    }
    outb(0x64, 0xFE);
    crate::time::spin_us(50_000);
    // Triple fault: an empty IDT and an exception.
    #[repr(C, packed)]
    struct Empty {
        limit: u16,
        base: u64,
    }
    let empty = Empty { limit: 0, base: 0 };
    // SAFETY: deliberately crashes the CPU into a reset.
    unsafe { core::arch::asm!("lidt [{}]", "int3", in(reg) &empty) };
    cpu::halt_forever();
}
