//! The [`KernelApi`] table handed to every driver module.

use crate::dev;
use crate::mem::{self, frame, paging, region, PAGE_SIZE};
use alloc::alloc::Layout;
use arcdk::*;
use core::ffi::c_void;

unsafe extern "C" fn api_log(level: u32, msg: *const u8, len: usize) {
    // SAFETY: the module passes a valid buffer.
    let bytes = unsafe { core::slice::from_raw_parts(msg, len.min(1024)) };
    let text = core::str::from_utf8(bytes).unwrap_or("<invalid UTF-8>");
    let (source, text) = text.split_once(": ").unwrap_or(("driver", text));
    let level = match level {
        LOG_ERROR => crate::console::LEVEL_ERROR,
        LOG_WARN => crate::console::LEVEL_WARN,
        LOG_INFO => crate::console::LEVEL_INFO,
        _ => crate::console::LEVEL_DEBUG,
    };
    crate::console::log(level, source, format_args!("{text}"));
}

unsafe extern "C" fn api_alloc(size: usize, align: usize) -> *mut u8 {
    match Layout::from_size_align(size.max(1), align.max(1)) {
        // SAFETY: non-zero size.
        Ok(l) => unsafe { alloc::alloc::alloc(l) },
        Err(_) => core::ptr::null_mut(),
    }
}

unsafe extern "C" fn api_free(ptr: *mut u8, size: usize, align: usize) {
    if ptr.is_null() {
        return;
    }
    if let Ok(l) = Layout::from_size_align(size.max(1), align.max(1)) {
        // SAFETY: the module frees what api_alloc returned with this layout.
        unsafe { alloc::alloc::dealloc(ptr, l) };
    }
}

unsafe extern "C" fn api_map_mmio(phys: u64, size: usize) -> *mut u8 {
    region::map_device(phys, size as u64, paging::NO_CACHE).map(|v| v as *mut u8).unwrap_or(core::ptr::null_mut())
}

unsafe extern "C" fn api_dma_alloc(size: usize, phys: *mut u64) -> *mut u8 {
    let pages = mem::align_up(size.max(1) as u64, PAGE_SIZE) / PAGE_SIZE;
    match frame::alloc_contiguous(pages, 4 << 30) {
        Some(p) => {
            if !phys.is_null() {
                // SAFETY: the module passes a valid out pointer.
                unsafe { *phys = p };
            }
            mem::phys_to_virt(p) as *mut u8
        }
        None => core::ptr::null_mut(),
    }
}

unsafe extern "C" fn api_dma_free(ptr: *mut u8, size: usize) {
    if ptr.is_null() {
        return;
    }
    let pages = mem::align_up(size.max(1) as u64, PAGE_SIZE) / PAGE_SIZE;
    frame::free_contiguous(ptr as u64 - mem::HHDM, pages);
}

unsafe extern "C" fn api_virt_to_phys(ptr: *const u8) -> u64 {
    mem::virt_to_phys(ptr as u64).unwrap_or(0)
}

fn pci_location(device: u32) -> Option<u32> {
    let info = dev::info(device)?;
    (info.bus == BUS_PCI).then_some(info.location)
}

unsafe extern "C" fn api_pci_read(device: u32, offset: u16, width: u8) -> u32 {
    match pci_location(device) {
        Some(loc) if offset < 256 => dev::pci::read(loc, offset, width),
        _ => u32::MAX,
    }
}

unsafe extern "C" fn api_pci_write(device: u32, offset: u16, width: u8, value: u32) {
    if let Some(loc) = pci_location(device) {
        if offset < 256 {
            dev::pci::write(loc, offset, width, value);
        }
    }
}

unsafe extern "C" fn api_irq_attach(device: u32, irq: u8, handler: IrqHandler, ctx: *mut c_void) -> Status {
    if dev::irq::attach_driver(device, irq, handler, ctx) {
        OK
    } else {
        E_INVAL
    }
}

unsafe extern "C" fn api_irq_detach(device: u32, irq: u8) {
    dev::irq::detach(device, Some(irq));
}

unsafe extern "C" fn api_time_ms() -> u64 {
    crate::time::uptime_ms()
}

unsafe extern "C" fn api_sleep_ms(ms: u64) {
    crate::task::sleep_ms(ms);
}

unsafe extern "C" fn api_yield() {
    crate::task::yield_now();
}

unsafe extern "C" fn api_spawn(name: *const u8, len: usize, entry: ThreadEntry, ctx: *mut c_void) -> Status {
    // SAFETY: the module passes a valid name.
    let name = unsafe { core::str::from_utf8(core::slice::from_raw_parts(name, len.min(64))).unwrap_or("driver") };
    match crate::task::spawn_raw(name, entry, ctx) {
        Some(tid) => tid as Status,
        None => E_NOMEM,
    }
}

unsafe extern "C" fn api_block_register(device: u32, desc: *const BlockDesc) -> Status {
    if desc.is_null() {
        return E_INVAL;
    }
    dev::block::register(device, desc)
}

unsafe extern "C" fn api_input_event(device: u32, kind: u32, code: u32, value: i32) {
    dev::input::driver_event(device, kind, code, value);
}

unsafe extern "C" fn api_net_register(device: u32, desc: *const NetDesc) -> Status {
    if desc.is_null() {
        return E_INVAL;
    }
    dev::net::register(device, desc)
}

unsafe extern "C" fn api_net_receive(iface: u32, frame: *const u8, len: usize) {
    if frame.is_null() || len > 65536 {
        return;
    }
    // SAFETY: the driver passes a valid frame.
    dev::net::receive(iface, unsafe { core::slice::from_raw_parts(frame, len) });
}

pub static KERNEL_API: KernelApi = KernelApi {
    abi_version: ABI_VERSION,
    size: core::mem::size_of::<KernelApi>() as u32,
    log: api_log,
    alloc: api_alloc,
    free: api_free,
    map_mmio: api_map_mmio,
    dma_alloc: api_dma_alloc,
    dma_free: api_dma_free,
    virt_to_phys: api_virt_to_phys,
    pci_read: api_pci_read,
    pci_write: api_pci_write,
    irq_attach: api_irq_attach,
    irq_detach: api_irq_detach,
    time_ms: api_time_ms,
    sleep_ms: api_sleep_ms,
    yield_now: api_yield,
    spawn: api_spawn,
    block_register: api_block_register,
    input_event: api_input_event,
    net_register: api_net_register,
    net_receive: api_net_receive,
};
