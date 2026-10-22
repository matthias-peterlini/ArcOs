//! Loader for driver modules: ELF64 position-independent images.
//!
//! A module is linked as a static PIE: no imports, only `R_X86_64_RELATIVE`
//! relocations. The loader maps its segments into the module region, applies
//! the relocations and returns the entry point.

use crate::mem::{self, region, PAGE_SIZE};
use alloc::string::String;

pub struct Image {
    pub base: u64,
    pub pages: u64,
    pub entry: u64,
}

impl Image {
    pub fn size(&self) -> u64 {
        self.pages * PAGE_SIZE
    }

    /// Unmaps the image. Its code must not run any more.
    pub fn unload(self) {
        region::free_pages(&region::MODULES, self.base, self.pages, 1);
    }
}

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}
fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}
fn u64_at(b: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(o..o + 8)?.try_into().ok()?))
}

const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const DT_NULL: u64 = 0;
const DT_RELA: u64 = 7;
const DT_RELASZ: u64 = 8;
const DT_RELAENT: u64 = 9;
const DT_RELR: u64 = 36;
const DT_RELRSZ: u64 = 35;
const R_X86_64_NONE: u32 = 0;
const R_X86_64_RELATIVE: u32 = 8;

struct Segment {
    vaddr: u64,
    offset: u64,
    filesz: u64,
    memsz: u64,
}

/// Loads a module into the module region.
pub fn load(elf: &[u8]) -> Result<Image, String> {
    load_with(elf, |pages| region::alloc_pages(&region::MODULES, pages, 1), |img| img.unload())
}

/// Loads an image at an address chosen by `alloc` (given the page count,
/// it maps that many zeroed pages and returns the base); `undo` releases
/// them if relocation fails.
pub fn load_with(elf: &[u8], alloc: impl FnOnce(u64) -> Option<u64>, undo: impl FnOnce(Image)) -> Result<Image, String> {
    let err = |m: &str| String::from(m);
    if elf.len() < 64 || &elf[..4] != b"\x7fELF" || elf[4] != 2 || elf[5] != 1 {
        return Err(err("not a 64-bit little-endian ELF file"));
    }
    if u16_at(elf, 16) != Some(3) {
        return Err(err("the image is not position-independent (ET_DYN)"));
    }
    if u16_at(elf, 18) != Some(0x3E) {
        return Err(err("the image is not built for x86-64"));
    }
    let entry = u64_at(elf, 24).ok_or("bad header")?;
    let phoff = u64_at(elf, 32).ok_or("bad header")? as usize;
    let phentsize = u16_at(elf, 54).ok_or("bad header")? as usize;
    let phnum = u16_at(elf, 56).ok_or("bad header")? as usize;
    if phentsize < 56 || phnum > 64 {
        return Err(err("bad program headers"));
    }

    let mut segments = alloc::vec::Vec::new();
    let mut dynamic = None;
    let mut end = 0u64;
    for i in 0..phnum {
        let p = phoff + i * phentsize;
        let kind = u32_at(elf, p).ok_or("bad program header")?;
        let offset = u64_at(elf, p + 8).ok_or("bad program header")?;
        let vaddr = u64_at(elf, p + 16).ok_or("bad program header")?;
        let filesz = u64_at(elf, p + 32).ok_or("bad program header")?;
        let memsz = u64_at(elf, p + 40).ok_or("bad program header")?;
        match kind {
            PT_LOAD => {
                if filesz > memsz || offset.checked_add(filesz).is_none_or(|e| e > elf.len() as u64) {
                    return Err(err("segment outside the file"));
                }
                end = end.max(vaddr.checked_add(memsz).ok_or("bad segment")?);
                segments.push(Segment { vaddr, offset, filesz, memsz });
            }
            PT_DYNAMIC => dynamic = Some((vaddr, memsz)),
            _ => {}
        }
    }
    if segments.is_empty() || end > 64 << 20 {
        return Err(err("the image has no loadable segments or is too big"));
    }
    if entry >= end {
        return Err(err("entry point outside the module"));
    }

    let pages = mem::align_up(end, PAGE_SIZE) / PAGE_SIZE;
    let base = alloc(pages).ok_or("out of memory")?;
    let image = Image { base, pages, entry: base + entry };
    for s in &segments {
        // SAFETY: the destination lies inside the freshly mapped image
        // (vaddr + memsz <= end) and the source inside the file.
        unsafe {
            core::ptr::copy_nonoverlapping(elf.as_ptr().add(s.offset as usize), (base + s.vaddr) as *mut u8, s.filesz as usize);
        }
        let _ = s.memsz; // .bss is already zero: the pages are fresh.
    }

    if let Some((dyn_addr, dyn_size)) = dynamic {
        if let Err(e) = relocate(&image, end, dyn_addr, dyn_size) {
            undo(image);
            return Err(e);
        }
    }
    Ok(image)
}

/// Reads a u64 inside the loaded image.
fn read_image(img: &Image, end: u64, off: u64) -> Result<u64, String> {
    if off.checked_add(8).is_none_or(|e| e > end) {
        return Err(String::from("relocation outside the module"));
    }
    // SAFETY: bounds checked against the mapped image.
    Ok(unsafe { ((img.base + off) as *const u64).read_unaligned() })
}

fn write_image(img: &Image, end: u64, off: u64, v: u64) -> Result<(), String> {
    if off.checked_add(8).is_none_or(|e| e > end) {
        return Err(String::from("relocation outside the module"));
    }
    // SAFETY: bounds checked against the mapped image.
    unsafe { ((img.base + off) as *mut u64).write_unaligned(v) };
    Ok(())
}

fn relocate(img: &Image, end: u64, dyn_addr: u64, dyn_size: u64) -> Result<(), String> {
    let (mut rela, mut relasz, mut relaent) = (0, 0, 24);
    let (mut relr, mut relrsz) = (0, 0);
    let mut off = dyn_addr;
    while off + 16 <= dyn_addr + dyn_size {
        let tag = read_image(img, end, off)?;
        let val = read_image(img, end, off + 8)?;
        match tag {
            DT_NULL => break,
            DT_RELA => rela = val,
            DT_RELASZ => relasz = val,
            DT_RELAENT => relaent = val,
            DT_RELR => relr = val,
            DT_RELRSZ => relrsz = val,
            _ => {}
        }
        off += 16;
    }
    if relaent < 24 {
        return Err(String::from("bad relocation entry size"));
    }
    let mut r = rela;
    while r + relaent <= rela + relasz {
        let at = read_image(img, end, r)?;
        let info = read_image(img, end, r + 8)?;
        let addend = read_image(img, end, r + 16)?;
        match info as u32 {
            R_X86_64_NONE => {}
            R_X86_64_RELATIVE => write_image(img, end, at, img.base.wrapping_add(addend))?,
            t => return Err(alloc::format!("unsupported relocation type {t} (the module must not import symbols)")),
        }
        r += relaent;
    }
    // Packed relative relocations (lld -z pack-relative-relocs).
    let mut where_ = 0u64;
    let mut r = relr;
    while r + 8 <= relr + relrsz {
        let entry = read_image(img, end, r)?;
        if entry & 1 == 0 {
            let v = read_image(img, end, entry)?;
            write_image(img, end, entry, v.wrapping_add(img.base))?;
            where_ = entry + 8;
        } else {
            let mut bits = entry >> 1;
            let mut at = where_;
            while bits != 0 {
                if bits & 1 != 0 {
                    let v = read_image(img, end, at)?;
                    write_image(img, end, at, v.wrapping_add(img.base))?;
                }
                bits >>= 1;
                at += 8;
            }
            where_ += 63 * 8;
        }
        r += 8;
    }
    Ok(())
}
