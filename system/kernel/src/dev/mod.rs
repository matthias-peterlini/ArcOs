//! Devices: everything the kernel found on the machine, and which driver
//! owns each one.
//!
//! Devices come from bus scans (PCI) and from platform detection (PS/2
//! controller and other fixed hardware). Drivers are attached to them by
//! [`crate::driver`], automatically.

pub mod block;
pub mod input;
pub mod irq;
pub mod keymap;
pub mod net;
pub mod pci;
pub mod platform;

use crate::sync::IrqLock;
use alloc::string::String;
use alloc::vec::Vec;
use arcdk::{DeviceInfo, BUS_PCI};

#[derive(Clone)]
pub struct Device {
    pub info: DeviceInfo,
    /// Driver module bound to the device.
    pub driver: Option<String>,
    /// Kernel component that handles the device itself (no module needed).
    pub builtin: Option<&'static str>,
    /// Why the last attempt to find a driver failed, if it did.
    pub problem: Option<String>,
}

impl Device {
    pub fn description(&self) -> String {
        if self.info.bus == BUS_PCI {
            let class = pci::class_name(self.info.class, self.info.subclass, self.info.prog_if);
            match pci::vendor_name(self.info.vendor) {
                Some(v) => alloc::format!("{v} {class}"),
                None => String::from(class),
            }
        } else {
            String::from(platform::describe(self.info.name()))
        }
    }

    /// "pci 00:1f.2" or "platform ps2-keyboard"
    pub fn location(&self) -> String {
        if self.info.bus == BUS_PCI {
            let l = self.info.location;
            alloc::format!("pci {:02x}:{:02x}.{}", l >> 8, (l >> 3) & 0x1F, l & 7)
        } else {
            alloc::format!("platform {}", self.info.name())
        }
    }

    pub fn has_driver(&self) -> bool {
        self.driver.is_some() || self.builtin.is_some()
    }

    /// Bridges and other devices that need no driver.
    pub fn needs_driver(&self) -> bool {
        !(self.info.bus == BUS_PCI
            && (self.info.class == 0x06 || (self.info.class == 0x05) || (self.info.class == 0x08 && self.info.subclass == 0x80)))
    }
}

static DEVICES: IrqLock<Vec<Device>> = IrqLock::new(Vec::new());

/// Adds a device unless one with the same bus location already exists.
/// Returns its id and whether it is new.
pub fn add(mut info: DeviceInfo, builtin: Option<&'static str>) -> (u32, bool) {
    let mut devs = DEVICES.lock();
    if let Some(d) = devs.iter().find(|d| d.info.bus == info.bus && d.info.location == info.location && d.info.name == info.name) {
        return (d.info.id, false);
    }
    info.id = devs.len() as u32 + 1;
    let id = info.id;
    devs.push(Device { info, driver: None, builtin, problem: None });
    (id, true)
}

pub fn get(id: u32) -> Option<Device> {
    DEVICES.lock().get(id.checked_sub(1)? as usize).cloned()
}

pub fn info(id: u32) -> Option<DeviceInfo> {
    get(id).map(|d| d.info)
}

pub fn set_driver(id: u32, driver: Option<&str>) {
    if let Some(d) = DEVICES.lock().get_mut(id as usize - 1) {
        d.driver = driver.map(String::from);
        if driver.is_some() {
            d.problem = None;
        }
    }
}

pub fn set_problem(id: u32, problem: Option<String>) {
    if let Some(d) = DEVICES.lock().get_mut(id as usize - 1) {
        d.problem = problem;
    }
}

pub fn list() -> Vec<Device> {
    DEVICES.lock().clone()
}

/// Finds hardware: PCI functions and platform devices. Returns the ids of
/// devices seen for the first time.
pub fn scan() -> Vec<u32> {
    let mut new = Vec::new();
    for info in pci::scan() {
        let (id, fresh) = add(info, None);
        if fresh {
            new.push(id);
        }
    }
    for (info, builtin) in platform::detect() {
        let (id, fresh) = add(info, builtin);
        if fresh {
            new.push(id);
        }
    }
    new
}
