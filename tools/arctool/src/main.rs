//! arctool — builds arcOS: driver packages, bootable disk images, and access
//! to ArcFS volumes from the host.
//!
//! ```text
//! arctool pack <driver.manifest> <module> <out.adp>
//! arctool inspect <file.adp>
//! arctool image <out.img> --stage1 <bin> --stage2 <bin> --kernel <elf> --drivers <dir>
//!               [--efi <BOOTX64.efi>] [--size 128M] [--files <dir>]
//! arctool fs <image> ls [path] | cat <path> | put <local> <path> | get <path> <local>
//!               | mkdir <path> | rm <path> | check
//! ```

mod disk;
mod elf;
mod fat;

use arcdk::manifest::Manifest;
use arcdk::package::{self, Package};
use arcfs::{Fs, Kind, MemDisk, BLOCK_SIZE};
use disk::{FileBlocks, CLOCK};
use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

type Result<T> = std::result::Result<T, String>;

const KERNEL_LINK_BASE: u64 = 0xFFFF_FFFF_8010_0000;
const SECTOR: u64 = 512;
const STAGE2_MAX_SECTORS: u64 = 64;
const KERNEL_LBA: u64 = 64;
/// Size of the EFI system partition (FAT16).
const ESP_SIZE: u64 = 16 << 20;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("pack") => cmd_pack(&args[1..]),
        Some("inspect") => cmd_inspect(&args[1..]),
        Some("image") => cmd_image(&args[1..]),
        Some("fs") => cmd_fs(&args[1..]),
        _ => Err(String::from(
            "usage:\n  arctool pack <driver.manifest> <module> <out.adp>\n  arctool inspect <file.adp>\n  arctool image <out.img> --stage1 <bin> --stage2 <bin> --kernel <elf> --drivers <dir> [--efi <BOOTX64.efi>] [--size 128M] [--files <dir>]\n  arctool fs <image> ls [path] | cat <path> | put <local> <path> | get <path> <local> | mkdir <path> | rm <path> | check",
        )),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("arctool: {e}");
            ExitCode::FAILURE
        }
    }
}

fn read(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    let path = path.as_ref();
    fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

// ----------------------------------------------------------------------------
// pack / inspect

fn cmd_pack(args: &[String]) -> Result<()> {
    let [manifest, module, out] = args else {
        return Err("usage: arctool pack <driver.manifest> <module> <out.adp>".into());
    };
    let text = String::from_utf8(read(manifest)?).map_err(|_| format!("{manifest}: not UTF-8"))?;
    let m = Manifest::parse(&text).map_err(|e| format!("{manifest}: {e}"))?;
    if m.rules().next().is_none() {
        eprintln!("arctool: warning: {manifest} has no 'match' rule; the driver will never be installed automatically");
    }
    let image = read(module)?;
    let elf = elf::Elf::parse(&image).map_err(|e| format!("{module}: {e}"))?;
    elf.check_module().map_err(|e| format!("{module}: {e}"))?;
    let mut buf = vec![0u8; package::packed_size(&text, &image)];
    package::write(&mut buf, arcdk::ABI_VERSION, &text, &image);
    fs::write(out, &buf).map_err(|e| format!("{out}: {e}"))?;
    println!("{} {} -> {} ({} bytes)", m.name(), m.version(), out, buf.len());
    Ok(())
}

fn cmd_inspect(args: &[String]) -> Result<()> {
    let [file] = args else { return Err("usage: arctool inspect <file.adp>".into()) };
    let bytes = read(file)?;
    let p = Package::parse(&bytes).map_err(|e| format!("{file}: {e}"))?;
    println!("driver ABI {}, module {} bytes", p.abi_version, p.module.len());
    print!("{}", p.manifest.text());
    Ok(())
}

// ----------------------------------------------------------------------------
// image

fn parse_size(s: &str) -> Result<u64> {
    let s = s.trim();
    let (num, mult) = match s.chars().last() {
        Some('K' | 'k') => (&s[..s.len() - 1], 1 << 10),
        Some('M' | 'm') => (&s[..s.len() - 1], 1 << 20),
        Some('G' | 'g') => (&s[..s.len() - 1], 1 << 30),
        _ => (s, 1),
    };
    num.parse::<u64>().map(|n| n * mult).map_err(|_| format!("bad size {s}"))
}

struct DriverFile {
    name: String,
    file_name: String,
    bytes: Vec<u8>,
    boot: bool,
}

fn load_drivers(dir: &Path) -> Result<Vec<DriverFile>> {
    let mut out = Vec::new();
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "adp"))
        .collect();
    entries.sort();
    for path in entries {
        let bytes = read(&path)?;
        let p = Package::parse(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        out.push(DriverFile {
            name: p.manifest.name().to_string(),
            file_name: path.file_name().unwrap().to_string_lossy().to_lowercase(),
            boot: p.manifest.is_boot(),
            bytes: bytes.clone(),
        });
    }
    Ok(out)
}

/// The boot pack: a small read-only ArcFS image with the boot drivers.
fn build_boot_pack(drivers: &[DriverFile]) -> Result<Vec<u8>> {
    let payload: usize = drivers.iter().filter(|d| d.boot).map(|d| d.bytes.len().div_ceil(BLOCK_SIZE) + 2).sum();
    let blocks = (payload + 32).max(64);
    let mut fs = Fs::format(MemDisk::new(blocks), "Boot", &CLOCK).map_err(|e| format!("boot pack: {e}"))?;
    fs.create_dirs("drivers").map_err(|e| format!("boot pack: {e}"))?;
    for d in drivers.iter().filter(|d| d.boot) {
        fs.write_file(&format!("drivers/{}", d.file_name), &d.bytes).map_err(|e| format!("boot pack: {}: {e}", d.name))?;
    }
    fs.sync().map_err(|e| format!("boot pack: {e}"))?;
    Ok(fs.into_device().data)
}

/// Copies a host folder into the volume, recursively.
fn copy_tree(fs: &mut Fs<FileBlocks>, host: &Path, inner: &str) -> Result<()> {
    let mut entries: Vec<PathBuf> =
        fs::read_dir(host).map_err(|e| format!("{}: {e}", host.display()))?.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let target = format!("{inner}/{name}");
        if path.is_dir() {
            fs.create_dirs(&target).map_err(|e| format!("{target}: {e}"))?;
            copy_tree(fs, &path, &target)?;
        } else {
            fs.write_file(&target, &read(&path)?).map_err(|e| format!("{target}: {e}"))?;
        }
    }
    Ok(())
}

fn cmd_image(args: &[String]) -> Result<()> {
    let mut out = None;
    let mut size = 128u64 << 20;
    let (mut stage1, mut stage2, mut kernel, mut drivers, mut files, mut efi) = (None, None, None, None, None, None);
    let mut i = 0;
    while i < args.len() {
        let next = || args.get(i + 1).cloned().ok_or(format!("{} needs a value", args[i]));
        match args[i].as_str() {
            "--size" => size = parse_size(&next()?)?,
            "--stage1" => stage1 = Some(next()?),
            "--stage2" => stage2 = Some(next()?),
            "--kernel" => kernel = Some(next()?),
            "--drivers" => drivers = Some(next()?),
            "--files" => files = Some(next()?),
            "--efi" => efi = Some(next()?),
            a if a.starts_with("--") => return Err(format!("unknown option {a}")),
            a => {
                out = Some(a.to_string());
                i += 1;
                continue;
            }
        }
        i += 2;
    }
    let out = out.ok_or("missing output file")?;
    let stage1 = read(stage1.ok_or("missing --stage1")?)?;
    let stage2 = read(stage2.ok_or("missing --stage2")?)?;
    let kernel_path = kernel.ok_or("missing --kernel")?;
    let kernel_elf = read(&kernel_path)?;
    let drivers = load_drivers(Path::new(&drivers.ok_or("missing --drivers")?))?;
    let efi = efi.map(read).transpose()?;

    if stage1.len() != 512 {
        return Err("stage 1 must be exactly 512 bytes".into());
    }
    let stage2_sectors = (stage2.len() as u64).div_ceil(SECTOR);
    if stage2_sectors > STAGE2_MAX_SECTORS {
        return Err(format!("stage 2 is {} bytes, the limit is {}", stage2.len(), STAGE2_MAX_SECTORS * SECTOR));
    }

    let elf = elf::Elf::parse(&kernel_elf).map_err(|e| format!("{kernel_path}: {e}"))?;
    let (base, flat, memsz) = elf.flatten().map_err(|e| format!("{kernel_path}: {e}"))?;
    if base != KERNEL_LINK_BASE {
        return Err(format!("{kernel_path}: linked at {base:#x}, expected {KERNEL_LINK_BASE:#x}"));
    }
    let kernel_sectors = (flat.len() as u64).div_ceil(SECTOR);
    let pack = build_boot_pack(&drivers)?;
    let pack_lba = (KERNEL_LBA + kernel_sectors).next_multiple_of(8);
    let pack_sectors = (pack.len() as u64).div_ceil(SECTOR);
    // With a UEFI loader: the EFI system partition, then the system volume.
    let esp_lba = (pack_lba + pack_sectors).next_multiple_of(2048).max(2048);
    let esp_sectors = if efi.is_some() { ESP_SIZE / SECTOR } else { 0 };
    let part_lba = esp_lba + esp_sectors;
    let total_sectors = size / SECTOR;
    if part_lba + 4096 > total_sectors {
        return Err(format!("the image is too small: the system needs at least {} MiB", ((part_lba + 4096) * SECTOR) >> 20));
    }
    if memsz > (1 << 30) {
        return Err("kernel too big".into());
    }

    let mut f = OpenOptions::new().read(true).write(true).create(true).truncate(true).open(&out).map_err(|e| format!("{out}: {e}"))?;
    f.set_len(size).map_err(|e| format!("{out}: {e}"))?;

    // MBR: boot code, boot table, the ArcFS partition and the ESP.
    let mut mbr = stage1.clone();
    mbr[0x190..0x194].copy_from_slice(b"ARCB");
    mbr[0x194..0x196].copy_from_slice(&(stage2_sectors as u16).to_le_bytes());
    mbr[0x198..0x19C].copy_from_slice(&(KERNEL_LBA as u32).to_le_bytes());
    mbr[0x19C..0x1A0].copy_from_slice(&(kernel_sectors as u32).to_le_bytes());
    mbr[0x1A0..0x1A4].copy_from_slice(&(memsz as u32).to_le_bytes());
    mbr[0x1A4..0x1A8].copy_from_slice(&(pack_lba as u32).to_le_bytes());
    mbr[0x1A8..0x1AC].copy_from_slice(&(pack_sectors as u32).to_le_bytes());
    mbr[0x1AC..0x1B4].copy_from_slice(&elf.entry.to_le_bytes());
    let signature: u32 = 0x4152_4300 | (std::process::id() & 0xFF);
    mbr[0x1B8..0x1BC].copy_from_slice(&signature.to_le_bytes());
    let part_sectors = total_sectors - part_lba;
    let mut partition = |slot: usize, active: bool, kind: u8, lba: u64, sectors: u64| {
        let entry = &mut mbr[0x1BE + slot * 16..0x1CE + slot * 16];
        entry.fill(0);
        entry[0] = if active { 0x80 } else { 0 };
        entry[1..4].copy_from_slice(&[0xFE, 0xFF, 0xFF]);
        entry[4] = kind;
        entry[5..8].copy_from_slice(&[0xFE, 0xFF, 0xFF]);
        entry[8..12].copy_from_slice(&(lba as u32).to_le_bytes());
        entry[12..16].copy_from_slice(&(sectors as u32).to_le_bytes());
    };
    partition(0, true, 0xAC, part_lba, part_sectors);
    if esp_sectors > 0 {
        partition(1, false, 0xEF, esp_lba, esp_sectors);
    }

    let mut write_at = |lba: u64, data: &[u8]| -> Result<()> {
        f.seek(SeekFrom::Start(lba * SECTOR)).and_then(|_| f.write_all(data)).map_err(|e| format!("{out}: {e}"))
    };
    write_at(0, &mbr)?;
    write_at(1, &stage2)?;
    write_at(KERNEL_LBA, &flat)?;
    write_at(pack_lba, &pack)?;
    if let Some(loader) = efi {
        // KERNEL.BIN: magic, entry, memory size, file size, flat image.
        let mut kernel_bin = Vec::with_capacity(32 + flat.len());
        kernel_bin.extend_from_slice(b"ARCKERN1");
        for v in [elf.entry, memsz, flat.len() as u64] {
            kernel_bin.extend_from_slice(&v.to_le_bytes());
        }
        kernel_bin.extend_from_slice(&flat);
        let files = vec![("EFI/BOOT/BOOTX64.EFI", loader), ("ARCOS/KERNEL.BIN", kernel_bin), ("ARCOS/BOOTPACK.IMG", pack.clone())];
        write_at(esp_lba, &fat::build(ESP_SIZE, esp_lba, files)?)?;
    }

    // The system volume.
    let file = f.try_clone().map_err(|e| e.to_string())?;
    let dev = FileBlocks { file, offset: part_lba * SECTOR, blocks: part_sectors * SECTOR / BLOCK_SIZE as u64 };
    let mut fs = Fs::format(dev, "System", &CLOCK).map_err(|e| format!("system volume: {e}"))?;
    let fserr = |e: arcfs::Error| format!("system volume: {e}");
    for dir in ["system/drivers", "inbox", "home"] {
        fs.create_dirs(dir).map_err(fserr)?;
    }
    for d in &drivers {
        fs.write_file(&format!("system/drivers/{}", d.file_name), &d.bytes).map_err(fserr)?;
    }
    if let Some(dir) = files {
        copy_tree(&mut fs, Path::new(&dir), "")?;
    }
    let sys = fs.lookup("system").map_err(fserr)?;
    fs.set_flags(sys, arcfs::FLAG_SYSTEM).map_err(fserr)?;
    fs.sync().map_err(fserr)?;

    let boot: Vec<&str> = drivers.iter().filter(|d| d.boot).map(|d| d.name.as_str()).collect();
    println!(
        "{out}: {} MiB, {} — kernel {} KiB, boot pack {} KiB ({}), {} drivers in the store",
        size >> 20,
        if esp_sectors > 0 { "BIOS and UEFI" } else { "BIOS" },
        flat.len() >> 10,
        pack.len() >> 10,
        boot.join(", "),
        drivers.len()
    );
    Ok(())
}

// ----------------------------------------------------------------------------
// fs

fn open_volume(image: &str) -> Result<Fs<FileBlocks>> {
    let mut file = OpenOptions::new().read(true).write(true).open(image).map_err(|e| format!("{image}: {e}"))?;
    let (offset, len) = disk::locate(&mut file).map_err(|e| format!("{image}: {e}"))?;
    let dev = FileBlocks { file, offset, blocks: len / BLOCK_SIZE as u64 };
    Fs::mount(dev, &CLOCK).map_err(|e| format!("{image}: {e}"))
}

fn fs_err(path: &str) -> impl Fn(arcfs::Error) -> String + '_ {
    move |e| format!("{path}: {e}")
}

fn cmd_fs(args: &[String]) -> Result<()> {
    let (Some(image), Some(cmd)) = (args.first(), args.get(1)) else {
        return Err("usage: arctool fs <image> <ls|cat|put|get|mkdir|rm|check> ...".into());
    };
    let mut fs = open_volume(image)?;
    let a = |i: usize| args.get(i).map(String::as_str).ok_or_else(|| format!("fs {cmd}: missing argument"));
    match cmd.as_str() {
        "ls" => {
            let path = args.get(2).map(String::as_str).unwrap_or("/");
            let id = fs.lookup(path).map_err(fs_err(path))?;
            for e in fs.read_dir(id).map_err(fs_err(path))? {
                let n = fs.node(e.node).map_err(fs_err(path))?;
                if e.kind == Kind::Dir {
                    println!("{:>10}  {}/", "", e.name);
                } else {
                    println!("{:>10}  {}", n.size, e.name);
                }
            }
        }
        "cat" => {
            let path = a(2)?;
            let data = fs.read_file(path).map_err(fs_err(path))?;
            std::io::stdout().write_all(&data).map_err(|e| e.to_string())?;
        }
        "put" => {
            let (local, path) = (a(2)?, a(3)?);
            let data = read(local)?;
            let target = if path.ends_with('/') || fs.lookup(path).is_ok_and(|id| fs.node(id).is_ok_and(|n| n.is_dir())) {
                format!("{}/{}", path.trim_end_matches('/'), Path::new(local).file_name().unwrap().to_string_lossy())
            } else {
                path.to_string()
            };
            fs.write_file(&target, &data).map_err(fs_err(&target))?;
            println!("{local} -> {target} ({} bytes)", data.len());
        }
        "get" => {
            let (path, local) = (a(2)?, a(3)?);
            let data = fs.read_file(path).map_err(fs_err(path))?;
            fs::write(local, data).map_err(|e| format!("{local}: {e}"))?;
        }
        "mkdir" => {
            let path = a(2)?;
            fs.create_dirs(path).map_err(fs_err(path))?;
        }
        "rm" => {
            let path = a(2)?;
            let (dir, name) = fs.lookup_parent(path).map_err(fs_err(path))?;
            fs.remove(dir, name).map_err(fs_err(path))?;
        }
        "check" => {
            let n = fs.check().map_err(fs_err(image))?;
            let (total, free) = fs.space();
            println!("{image}: {n} problems, {} of {} KiB free, label {:?}", free >> 10, total >> 10, fs.superblock().label);
            if n != 0 {
                return Err("the volume has errors".into());
            }
        }
        other => return Err(format!("fs: unknown command {other}")),
    }
    fs.sync().map_err(fs_err(image))
}
