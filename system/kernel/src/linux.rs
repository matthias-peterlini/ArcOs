//! Running Linux programs.
//!
//! arcOS can load a statically linked Linux executable (an ELF built against
//! musl, with no shared libraries) and serve the Linux system calls it makes.
//! Programs arrive on the `syscall` instruction (see [`crate::arch::syscall`])
//! with the call number in `rax` and arguments in `rdi, rsi, rdx, r10, r8,
//! r9`, exactly as on Linux for x86-64.
//!
//! Only what command-line programs need is implemented: the console as the
//! three standard streams, files and folders (the Linux file tree is a view
//! of the running process's current arcOS volume), memory (`brk` and
//! anonymous `mmap`), the clock, randomness and thread-local storage. Network,
//! users, signals and processes are stubbed: calls return a plausible value or
//! `ENOSYS`, so a program that can do without them keeps going.

use crate::arch::idt::TrapFrame;
use crate::fs::{self, Path};
use crate::mem::{self, PAGE_SIZE};
use crate::sync::IrqLock;
use crate::task;
use crate::user;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use arcdk::syscall::Error as ArcError;

/// Seconds between the Unix epoch (1970) and arcOS's epoch (2000).
const UNIX_2000: u64 = 946_684_800;

// ----------------------------------------------------------------------------
// Errors (Linux errno values, returned negated)

mod err {
    pub const ENOENT: i64 = 2;
    pub const EBADF: i64 = 9;
    pub const ENOMEM: i64 = 12;
    pub const EACCES: i64 = 13;
    pub const EFAULT: i64 = 14;
    pub const EEXIST: i64 = 17;
    pub const ENOTDIR: i64 = 20;
    pub const EISDIR: i64 = 21;
    pub const EINVAL: i64 = 22;
    pub const ENOSPC: i64 = 28;
    pub const ENOTTY: i64 = 25;
    pub const ERANGE: i64 = 34;
    pub const ENOSYS: i64 = 38;
}

fn errno(e: ArcError) -> i64 {
    match e {
        ArcError::NotFound => err::ENOENT,
        ArcError::Exists => err::EEXIST,
        ArcError::NotDir => err::ENOTDIR,
        ArcError::IsDir => err::EISDIR,
        ArcError::NotEmpty => err::ENOTDIR,
        ArcError::NoSpace => err::ENOSPC,
        ArcError::InvalidName => err::EINVAL,
        ArcError::ReadOnly => err::EACCES,
        ArcError::Io => err::EACCES,
        ArcError::BadHandle => err::EBADF,
        ArcError::BadAddress => err::EFAULT,
        ArcError::NoMemory => err::ENOMEM,
        ArcError::BadProgram => err::ENOENT,
        ArcError::Unknown => err::EINVAL,
    }
}

// ----------------------------------------------------------------------------
// Per-process state: open files, the program break and the mmap area

struct OpenFile {
    path: Path,
    pos: u64,
    /// A folder read with `getdents64`: the names still to return.
    dir: Option<Vec<String>>,
    append: bool,
}

struct LinuxState {
    files: BTreeMap<i32, OpenFile>,
    next_fd: i32,
    brk_start: u64,
    brk_cur: u64,
    mmap_next: u64,
}

const BRK_BASE: u64 = 0x1000_0000_0000;
const MMAP_BASE: u64 = 0x2000_0000_0000;

impl LinuxState {
    fn new() -> LinuxState {
        LinuxState { files: BTreeMap::new(), next_fd: 3, brk_start: BRK_BASE, brk_cur: BRK_BASE, mmap_next: MMAP_BASE }
    }
}

static STATES: IrqLock<BTreeMap<u32, LinuxState>> = IrqLock::new(BTreeMap::new());

/// Drops the Linux state of a process that is ending.
pub fn forget(pid: u32) {
    STATES.lock().remove(&pid);
}

fn with_state<T>(f: impl FnOnce(&mut LinuxState) -> T) -> T {
    let pid = task::current_pid();
    let mut states = STATES.lock();
    f(states.entry(pid).or_insert_with(LinuxState::new))
}

// ----------------------------------------------------------------------------
// Paths: a Linux path names a place in the running process's arcOS volume.

fn linux_path(text: &str) -> Result<Path, i64> {
    let cwd = user::cwd().map_err(errno)?;
    // An absolute Linux path is rooted at the volume; a relative one hangs
    // off the current folder.
    let base = if text.starts_with('/') { Path::root(&cwd.volume) } else { cwd };
    Path::parse(text, &base).map_err(|e| errno(user::fs_error(e)))
}

/// Turns an arcOS path back into the Linux path a program sees ("/a/b").
fn show_path(path: &Path) -> String {
    path.inner.clone()
}

// ----------------------------------------------------------------------------
// Loading a static executable

/// Loads a Linux program into the current (empty) address space and returns
/// its entry point and the initial stack pointer. The stack holds argc, the
/// argument and environment pointers and the auxiliary vector, as the System
/// V ABI lays them out.
pub fn load(elf: &[u8], name: &str, args: &str) -> Result<(u64, u64), ArcError> {
    let info = map_segments(elf).map_err(|_| ArcError::BadProgram)?;
    let argv = core::iter::once(name.to_string()).chain(arcrt_split(args)).collect::<Vec<_>>();
    let stack = build_stack(&info, &argv)?;
    Ok((info.entry, stack))
}

struct Loaded {
    entry: u64,
    phdr: u64,
    phent: u64,
    phnum: u64,
}

const PT_LOAD: u32 = 1;
const PT_PHDR: u32 = 6;

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}
fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}
fn u64_at(b: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(o..o + 8)?.try_into().ok()?))
}

/// Maps the loadable segments of a static ELF (ET_EXEC at its own addresses,
/// or a static PIE rebased above 0) and returns where the headers landed.
fn map_segments(elf: &[u8]) -> Result<Loaded, ()> {
    if elf.len() < 64 || &elf[..4] != b"\x7fELF" || elf[4] != 2 || elf[5] != 1 {
        return Err(());
    }
    let etype = u16_at(elf, 16).ok_or(())?;
    if etype != 2 && etype != 3 {
        return Err(()); // only ET_EXEC and ET_DYN (static PIE)
    }
    if u16_at(elf, 18) != Some(0x3E) {
        return Err(()); // not x86-64
    }
    // A PIE is rebased so its lowest page sits here; an ET_EXEC keeps its
    // own addresses (bias 0).
    const PIE_BASE: u64 = 0x10_0000;
    let entry = u64_at(elf, 24).ok_or(())?;
    let phoff = u64_at(elf, 32).ok_or(())?;
    let phentsize = u16_at(elf, 54).ok_or(())? as u64;
    let phnum = u16_at(elf, 56).ok_or(())? as u64;
    if phentsize < 56 || phnum == 0 || phnum > 64 {
        return Err(());
    }

    let mut min_vaddr = u64::MAX;
    for i in 0..phnum {
        let p = (phoff + i * phentsize) as usize;
        if u32_at(elf, p).ok_or(())? == PT_LOAD {
            min_vaddr = min_vaddr.min(u64_at(elf, p + 16).ok_or(())?);
        }
    }
    let bias = if etype == 3 { PIE_BASE.wrapping_sub(min_vaddr & !(PAGE_SIZE - 1)) } else { 0 };

    let mut phdr = 0;
    for i in 0..phnum {
        let p = (phoff + i * phentsize) as usize;
        let kind = u32_at(elf, p).ok_or(())?;
        let offset = u64_at(elf, p + 8).ok_or(())?;
        let vaddr = u64_at(elf, p + 16).ok_or(())?.wrapping_add(bias);
        let filesz = u64_at(elf, p + 32).ok_or(())?;
        let memsz = u64_at(elf, p + 40).ok_or(())?;
        if kind == PT_PHDR {
            phdr = vaddr;
        }
        if kind != PT_LOAD || memsz == 0 {
            continue;
        }
        if filesz > memsz || offset.checked_add(filesz).is_none_or(|e| e > elf.len() as u64) {
            return Err(());
        }
        // Map whole pages covering [vaddr, vaddr + memsz), then copy the file
        // part in. Fresh pages are zero, so .bss needs nothing.
        let start = vaddr & !(PAGE_SIZE - 1);
        let end = mem::align_up(vaddr + memsz, PAGE_SIZE);
        map_maybe(start, end - start).map_err(|_| ())?;
        // SAFETY: the range was just mapped writable in this address space.
        unsafe {
            core::ptr::copy_nonoverlapping(elf.as_ptr().add(offset as usize), vaddr as *mut u8, filesz as usize);
        }
    }
    // If there was no PT_PHDR, the headers are still in the first segment at
    // file offset phoff, which the ABI places at min_vaddr + phoff for a
    // standard layout.
    if phdr == 0 {
        phdr = min_vaddr.wrapping_add(bias).wrapping_add(phoff);
    }
    Ok(Loaded { entry: entry.wrapping_add(bias), phdr, phent: phentsize, phnum })
}

/// Maps pages for a range, skipping any page already present (segments can
/// share a page at their boundary).
fn map_maybe(addr: u64, len: u64) -> Result<(), ArcError> {
    let mut page = addr;
    let end = addr + len;
    while page < end {
        if crate::mem::paging::translate_in(crate::arch::cpu::read_cr3(), page).is_none() {
            user::map_fixed(page, PAGE_SIZE)?;
        }
        page += PAGE_SIZE;
    }
    Ok(())
}

fn arcrt_split(args: &str) -> Vec<String> {
    // The same splitting arcrt::split_args does, kept here to avoid a cycle.
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote = None;
    let mut any = false;
    for c in args.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => cur.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                any = true;
            }
            None if c.is_whitespace() => {
                if any {
                    out.push(core::mem::take(&mut cur));
                    any = false;
                }
            }
            None => {
                cur.push(c);
                any = true;
            }
        }
    }
    if any {
        out.push(cur);
    }
    out
}

const AT_NULL: u64 = 0;
const AT_PHDR: u64 = 3;
const AT_PHENT: u64 = 4;
const AT_PHNUM: u64 = 5;
const AT_PAGESZ: u64 = 6;
const AT_ENTRY: u64 = 9;
const AT_RANDOM: u64 = 25;

/// Builds the initial stack: argc, argv, envp, the auxiliary vector and the
/// strings they point at. Returns the stack pointer to start the program on.
fn build_stack(info: &Loaded, argv: &[String]) -> Result<u64, ArcError> {
    const PAGES: u64 = 64;
    let top = arcdk::syscall::USER_STACK_TOP;
    let bottom = top - PAGES * PAGE_SIZE;
    user::map_fixed(bottom, PAGES * PAGE_SIZE)?;

    let envp: [&str; 2] = ["PATH=/apps", "TERM=arcos"];

    // String area at the top; collect the addresses as we go.
    let mut p = top;
    let mut push_str = |s: &str| {
        p -= s.len() as u64 + 1;
        // SAFETY: inside the mapped stack; we stay well below its size.
        unsafe {
            core::ptr::copy_nonoverlapping(s.as_ptr(), p as *mut u8, s.len());
            *(p as *mut u8).add(s.len()) = 0;
        }
        p
    };
    let arg_ptrs: Vec<u64> = argv.iter().map(|a| push_str(a)).collect();
    let env_ptrs: Vec<u64> = envp.iter().map(|e| push_str(e)).collect();
    // 16 random bytes for AT_RANDOM.
    let seed = crate::time::now().wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ crate::time::uptime_ms().wrapping_mul(0xD1B5_4A32_D192_ED03);
    p -= 16;
    let rand_at = p;
    // SAFETY: inside the mapped stack.
    unsafe { core::ptr::copy_nonoverlapping(seed.to_le_bytes().as_ptr(), p as *mut u8, 8) };

    // The word array: argc, argv[...], 0, envp[...], 0, auxv..., must end
    // 16-byte aligned so that after `argc` the stack meets the ABI.
    let mut words: Vec<u64> = Vec::new();
    words.push(arg_ptrs.len() as u64);
    words.extend_from_slice(&arg_ptrs);
    words.push(0);
    words.extend_from_slice(&env_ptrs);
    words.push(0);
    for (k, v) in [
        (AT_PHDR, info.phdr),
        (AT_PHENT, info.phent),
        (AT_PHNUM, info.phnum),
        (AT_PAGESZ, PAGE_SIZE),
        (AT_ENTRY, info.entry),
        (AT_RANDOM, rand_at),
        (AT_NULL, 0),
    ] {
        words.push(k);
        words.push(v);
    }
    let mut sp = p & !15;
    if (sp - words.len() as u64 * 8) & 15 != 0 {
        sp -= 8;
    }
    sp -= words.len() as u64 * 8;
    // SAFETY: inside the mapped stack, which is much larger than this.
    unsafe { core::ptr::copy_nonoverlapping(words.as_ptr(), sp as *mut u64, words.len()) };
    Ok(sp)
}

// ----------------------------------------------------------------------------
// System call dispatch

pub fn syscall(frame: &mut TrapFrame) {
    crate::arch::cpu::enable_irq();
    let n = frame.rax;
    let a = [frame.rdi, frame.rsi, frame.rdx, frame.r10, frame.r8, frame.r9];
    let ret = dispatch(n, a);
    if ret == -err::ENOSYS {
        crate::debug!("linux", "unimplemented syscall {n}");
    }
    frame.rax = ret as u64;
    if user::kill_requested() {
        user::terminate(-130);
    }
    crate::arch::cpu::disable_irq();
}

fn dispatch(n: u64, a: [u64; 6]) -> i64 {
    match n {
        0 => sys_read(a[0] as i32, a[1], a[2]),
        1 => sys_write(a[0] as i32, a[1], a[2]),
        2 => sys_open(a[0], a[1], true),
        3 => sys_close(a[0] as i32),
        4 => sys_stat(a[0], a[1]),           // stat
        5 => sys_fstat(a[0] as i32, a[1]),   // fstat
        6 => sys_stat(a[0], a[1]),           // lstat (no links here)
        8 => sys_lseek(a[0] as i32, a[1] as i64, a[2] as i32),
        9 => sys_mmap(a[1], a[3] as i32, a[4] as i32),
        10 => 0,                              // mprotect: everything is rwx
        11 => sys_munmap(a[0], a[1]),
        12 => sys_brk(a[0]),
        13 | 14 => 0,                         // rt_sigaction / rt_sigprocmask
        16 => sys_ioctl(a[0] as i32, a[1], a[2]),
        17 => sys_pread(a[0] as i32, a[1], a[2], a[3]),
        19 => sys_readv(a[0] as i32, a[1], a[2]),
        20 => sys_writev(a[0] as i32, a[1], a[2]),
        21 => sys_access(a[0]),
        25 | 28 | 273 | 334 => 0,             // mremap?/madvise/set_robust_list/rseq: harmless
        35 => sys_nanosleep(a[0]),
        39 | 110 => task::current_pid() as i64, // getpid / getppid
        60 | 231 => user::terminate(a[0] as i64), // exit / exit_group
        63 => sys_uname(a[0]),
        72 => sys_fcntl(a[0] as i32, a[1], a[2]),
        79 => sys_getcwd(a[0], a[1]),
        80 => sys_chdir(a[0]),
        89 => -err::EINVAL,                   // readlink: nothing is a link
        96 => sys_gettimeofday(a[0]),
        102 | 104 | 107 | 108 => 0,           // getuid/getgid/geteuid/getegid: root
        158 => sys_arch_prctl(a[0], a[1]),
        202 => -err::ENOSYS,                  // futex: single-threaded, unused
        218 => task::current_pid() as i64,    // set_tid_address
        228 => sys_clock_gettime(a[0], a[1]),
        257 => sys_openat(a[0] as i32, a[1], a[2]),
        262 => sys_newfstatat(a[0] as i32, a[1], a[2], a[3]),
        217 => sys_getdents64(a[0] as i32, a[1], a[2]),
        302 => sys_prlimit(a[2], a[3]),
        318 => sys_getrandom(a[0], a[1]),
        _ => -err::ENOSYS,
    }
}

// ----------------------------------------------------------------------------
// Memory

fn sys_brk(req: u64) -> i64 {
    with_state(|st| {
        if req == 0 || req < st.brk_start {
            return st.brk_cur as i64;
        }
        let new = mem::align_up(req, PAGE_SIZE);
        let old = mem::align_up(st.brk_cur, PAGE_SIZE);
        if new > old {
            if user::map_fixed(old, new - old).is_err() {
                return st.brk_cur as i64;
            }
        } else if new < old {
            let _ = user::unmap_range(new, old - new);
        }
        st.brk_cur = req;
        req as i64
    })
}

const MAP_ANONYMOUS: i32 = 0x20;

fn sys_mmap(len: u64, _prot: i32, flags: i32) -> i64 {
    if flags & MAP_ANONYMOUS == 0 {
        return -err::ENOSYS; // only anonymous memory (static binaries need no file maps)
    }
    let len = mem::align_up(len.max(1), PAGE_SIZE);
    with_state(|st| {
        let at = st.mmap_next;
        match user::map_fixed(at, len) {
            Ok(()) => {
                st.mmap_next += len;
                at as i64
            }
            Err(e) => -errno(e),
        }
    })
}

fn sys_munmap(addr: u64, len: u64) -> i64 {
    match user::unmap_range(addr, mem::align_up(len.max(1), PAGE_SIZE)) {
        Ok(()) => 0,
        Err(e) => -errno(e),
    }
}

// ----------------------------------------------------------------------------
// Thread-local storage

const ARCH_SET_FS: u64 = 0x1002;
const ARCH_SET_GS: u64 = 0x1001;

fn sys_arch_prctl(code: u64, addr: u64) -> i64 {
    let which = match code {
        ARCH_SET_FS => arcdk::syscall::TLS_FS,
        ARCH_SET_GS => arcdk::syscall::TLS_GS,
        _ => return -err::EINVAL,
    };
    match user::set_tls(which, addr) {
        Ok(()) => 0,
        Err(e) => -errno(e),
    }
}

// ----------------------------------------------------------------------------
// Console and files

fn user_bytes<'a>(ptr: u64, len: u64) -> Result<&'a mut [u8], i64> {
    user::user_slice(ptr, len).map_err(errno)
}

fn sys_write(fd: i32, ptr: u64, len: u64) -> i64 {
    let data = match user_bytes(ptr, len) {
        Ok(d) => d,
        Err(e) => return e,
    };
    if fd == 1 || fd == 2 {
        crate::print!("{}", String::from_utf8_lossy(data));
        return len as i64;
    }
    file_write(fd, data)
}

fn sys_read(fd: i32, ptr: u64, len: u64) -> i64 {
    if fd == 0 {
        return console_read(ptr, len);
    }
    let buf = match user_bytes(ptr, len) {
        Ok(b) => b,
        Err(e) => return e,
    };
    file_read(fd, buf)
}

/// Reads a line from the console, echoing it, as a cooked terminal would.
fn console_read(ptr: u64, len: u64) -> i64 {
    let buf = match user_bytes(ptr, len) {
        Ok(b) => b,
        Err(e) => return e,
    };
    if buf.is_empty() {
        return 0;
    }
    let mut line: Vec<u8> = Vec::new();
    loop {
        if user::kill_requested() {
            user::terminate(-130);
        }
        let Some(ev) = crate::dev::input::try_read_key() else {
            task::sleep_ms(5);
            continue;
        };
        use crate::dev::input::Key;
        match ev.key {
            Key::Char('d') | Key::Char('D') if ev.ctrl && line.is_empty() => return 0, // EOF
            Key::Char(c) => {
                let mut b = [0u8; 4];
                let s = c.encode_utf8(&mut b);
                line.extend_from_slice(s.as_bytes());
                crate::print!("{s}");
            }
            Key::Enter => {
                line.push(b'\n');
                crate::print!("\n");
                break;
            }
            Key::Backspace if !line.is_empty() => {
                // Drop one UTF-8 scalar.
                while line.pop().is_some_and(|b| b & 0xC0 == 0x80) {}
                crate::print!("\x08 \x08");
            }
            _ => {}
        }
        if line.len() >= buf.len() {
            break;
        }
    }
    let n = line.len().min(buf.len());
    buf[..n].copy_from_slice(&line[..n]);
    n as i64
}

const O_WRONLY: u64 = 1;
const O_RDWR: u64 = 2;
const O_CREAT: u64 = 0o100;
const O_TRUNC: u64 = 0o1000;
const O_APPEND: u64 = 0o2000;
const O_DIRECTORY: u64 = 0o200000;
const AT_FDCWD: i32 = -100;

fn sys_open(path_ptr: u64, flags: u64, _at: bool) -> i64 {
    let text = match user::user_cstr(path_ptr, 4096) {
        Ok(t) => t,
        Err(e) => return -errno(e),
    };
    let path = match linux_path(&text) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let writable = flags & (O_WRONLY | O_RDWR) != 0;
    match fs::stat(&path) {
        Ok(st) if st.kind == arcfs::Kind::Dir => {
            if writable {
                return -err::EISDIR;
            }
            let names = match fs::list_dir(&path) {
                Ok(list) => list.into_iter().map(|(e, _)| e.name).collect(),
                Err(e) => return -errno(user::fs_error(e)),
            };
            return open_slot(OpenFile { path, pos: 0, dir: Some(names), append: false });
        }
        Ok(_) => {
            if flags & O_TRUNC != 0 && writable {
                if let Err(e) = fs::write_file(&path, &[]) {
                    return -errno(user::fs_error(e));
                }
            }
        }
        Err(fs::Error::NotFound) if flags & O_CREAT != 0 => {
            if let Err(e) = fs::write_file(&path, &[]) {
                return -errno(user::fs_error(e));
            }
        }
        Err(e) => return -errno(user::fs_error(e)),
    }
    if flags & O_DIRECTORY != 0 {
        return -err::ENOTDIR;
    }
    let append = flags & O_APPEND != 0;
    let pos = if append { fs::stat(&path).map(|s| s.size).unwrap_or(0) } else { 0 };
    open_slot(OpenFile { path, pos, dir: None, append })
}

fn sys_openat(dirfd: i32, path_ptr: u64, flags: u64) -> i64 {
    if dirfd != AT_FDCWD {
        // Only "relative to cwd" is supported; absolute paths ignore it.
        let text = user::user_cstr(path_ptr, 4096);
        if !matches!(text, Ok(ref t) if t.starts_with('/')) {
            return -err::ENOSYS;
        }
    }
    sys_open(path_ptr, flags, true)
}

fn open_slot(file: OpenFile) -> i64 {
    with_state(|st| {
        let fd = st.next_fd;
        st.next_fd += 1;
        st.files.insert(fd, file);
        fd as i64
    })
}

fn sys_close(fd: i32) -> i64 {
    with_state(|st| if st.files.remove(&fd).is_some() || (0..=2).contains(&fd) { 0 } else { -err::EBADF })
}

fn file_write(fd: i32, data: &[u8]) -> i64 {
    let (path, pos, append) = match with_state(|st| st.files.get(&fd).map(|f| (f.path.clone(), f.pos, f.append))) {
        Some(v) => v,
        None => return -err::EBADF,
    };
    let at = if append { fs::stat(&path).map(|s| s.size).unwrap_or(pos) } else { pos };
    match fs::write_at(&path, at, data) {
        Ok(()) => {
            with_state(|st| {
                if let Some(f) = st.files.get_mut(&fd) {
                    f.pos = at + data.len() as u64;
                }
            });
            data.len() as i64
        }
        Err(e) => -errno(user::fs_error(e)),
    }
}

fn file_read(fd: i32, buf: &mut [u8]) -> i64 {
    let (path, pos) = match with_state(|st| st.files.get(&fd).map(|f| (f.path.clone(), f.pos))) {
        Some(v) => v,
        None => return -err::EBADF,
    };
    let mut tmp = vec![0u8; buf.len().min(1 << 20)];
    match fs::read_at(&path, pos, &mut tmp) {
        Ok(n) => {
            buf[..n].copy_from_slice(&tmp[..n]);
            with_state(|st| {
                if let Some(f) = st.files.get_mut(&fd) {
                    f.pos += n as u64;
                }
            });
            n as i64
        }
        Err(e) => -errno(user::fs_error(e)),
    }
}

fn sys_pread(fd: i32, ptr: u64, len: u64, off: u64) -> i64 {
    let path = match with_state(|st| st.files.get(&fd).map(|f| f.path.clone())) {
        Some(p) => p,
        None => return -err::EBADF,
    };
    let buf = match user_bytes(ptr, len) {
        Ok(b) => b,
        Err(e) => return e,
    };
    let mut tmp = vec![0u8; buf.len().min(1 << 20)];
    match fs::read_at(&path, off, &mut tmp) {
        Ok(n) => {
            buf[..n].copy_from_slice(&tmp[..n]);
            n as i64
        }
        Err(e) => -errno(user::fs_error(e)),
    }
}

fn sys_lseek(fd: i32, off: i64, whence: i32) -> i64 {
    with_state(|st| {
        let Some(f) = st.files.get_mut(&fd) else { return -err::EBADF };
        let size = fs::stat(&f.path).map(|s| s.size).unwrap_or(0) as i64;
        let base = match whence {
            0 => 0,           // SEEK_SET
            1 => f.pos as i64, // SEEK_CUR
            2 => size,        // SEEK_END
            _ => return -err::EINVAL,
        };
        let new = base + off;
        if new < 0 {
            return -err::EINVAL;
        }
        f.pos = new as u64;
        new
    })
}

fn sys_readv(fd: i32, iov: u64, count: u64) -> i64 {
    iov_each(iov, count, |ptr, len| sys_read(fd, ptr, len))
}

fn sys_writev(fd: i32, iov: u64, count: u64) -> i64 {
    iov_each(iov, count, |ptr, len| sys_write(fd, ptr, len))
}

/// Runs `f` on each `struct iovec { base, len }`, summing the bytes until one
/// transfer falls short or fails.
fn iov_each(iov: u64, count: u64, mut f: impl FnMut(u64, u64) -> i64) -> i64 {
    let bytes = match user_bytes(iov, count.saturating_mul(16)) {
        Ok(b) => b.to_vec(),
        Err(e) => return e,
    };
    let mut total = 0i64;
    let (chunks, _) = bytes.as_chunks::<16>();
    for chunk in chunks {
        let base = u64::from_le_bytes(chunk[..8].try_into().unwrap());
        let len = u64::from_le_bytes(chunk[8..].try_into().unwrap());
        if len == 0 {
            continue;
        }
        let r = f(base, len);
        if r < 0 {
            return if total > 0 { total } else { r };
        }
        total += r;
        if (r as u64) < len {
            break;
        }
    }
    total
}

// ----------------------------------------------------------------------------
// stat

fn write_stat(ptr: u64, st: &fs::Stat) -> i64 {
    let buf = match user_bytes(ptr, 144) {
        Ok(b) => b,
        Err(e) => return e,
    };
    buf.fill(0);
    let mode: u32 = if st.kind == arcfs::Kind::Dir { 0o040755 } else { 0o100644 };
    let secs = st.modified as i64;
    let put = |buf: &mut [u8], off: usize, v: u64| buf[off..off + 8].copy_from_slice(&v.to_le_bytes());
    put(buf, 16, 1); // st_nlink
    buf[24..28].copy_from_slice(&mode.to_le_bytes());
    put(buf, 48, st.size); // st_size
    put(buf, 56, 512); // st_blksize
    put(buf, 64, st.size.div_ceil(512)); // st_blocks
    for off in [72, 88, 104] {
        put(buf, off, secs as u64); // atime/mtime/ctime seconds
    }
    0
}

fn sys_stat(path_ptr: u64, buf: u64) -> i64 {
    let text = match user::user_cstr(path_ptr, 4096) {
        Ok(t) => t,
        Err(e) => return -errno(e),
    };
    let path = match linux_path(&text) {
        Ok(p) => p,
        Err(e) => return e,
    };
    match fs::stat(&path) {
        Ok(st) => write_stat(buf, &st),
        Err(e) => -errno(user::fs_error(e)),
    }
}

fn sys_fstat(fd: i32, buf: u64) -> i64 {
    if (0..=2).contains(&fd) {
        // A character device (the console): mode S_IFCHR.
        let b = match user_bytes(buf, 144) {
            Ok(b) => b,
            Err(e) => return e,
        };
        b.fill(0);
        b[24..28].copy_from_slice(&0o020620u32.to_le_bytes());
        b[16..24].copy_from_slice(&1u64.to_le_bytes());
        return 0;
    }
    let path = match with_state(|st| st.files.get(&fd).map(|f| f.path.clone())) {
        Some(p) => p,
        None => return -err::EBADF,
    };
    match fs::stat(&path) {
        Ok(st) => write_stat(buf, &st),
        Err(e) => -errno(user::fs_error(e)),
    }
}

fn sys_newfstatat(dirfd: i32, path_ptr: u64, buf: u64, flags: u64) -> i64 {
    const AT_EMPTY_PATH: u64 = 0x1000;
    let text = user::user_cstr(path_ptr, 4096).unwrap_or_default();
    if flags & AT_EMPTY_PATH != 0 || text.is_empty() {
        return sys_fstat(dirfd, buf);
    }
    sys_stat(path_ptr, buf)
}

// ----------------------------------------------------------------------------
// Directories (getdents64)

fn sys_getdents64(fd: i32, ptr: u64, cap: u64) -> i64 {
    let buf = match user_bytes(ptr, cap) {
        Ok(b) => b,
        Err(e) => return e,
    };
    with_state(|st| {
        let Some(f) = st.files.get_mut(&fd) else { return -err::EBADF };
        let Some(names) = f.dir.as_mut() else { return -err::ENOTDIR };
        let mut off = 0usize;
        while let Some(name) = names.first() {
            // struct linux_dirent64: ino(8) off(8) reclen(2) type(1) name(z)
            let reclen = (19 + name.len() + 1 + 7) & !7;
            if off + reclen > buf.len() {
                if off == 0 {
                    return -err::EINVAL; // buffer too small even for one
                }
                break;
            }
            let e = &mut buf[off..off + reclen];
            e.fill(0);
            e[0..8].copy_from_slice(&1u64.to_le_bytes()); // d_ino
            e[8..16].copy_from_slice(&((off + reclen) as u64).to_le_bytes()); // d_off
            e[16..18].copy_from_slice(&(reclen as u16).to_le_bytes());
            e[18] = 0; // DT_UNKNOWN
            e[19..19 + name.len()].copy_from_slice(name.as_bytes());
            off += reclen;
            names.remove(0);
        }
        off as i64
    })
}

// ----------------------------------------------------------------------------
// ioctl / fcntl / access

fn sys_ioctl(fd: i32, req: u64, arg: u64) -> i64 {
    const TCGETS: u64 = 0x5401;
    const TIOCGWINSZ: u64 = 0x5413;
    if !(0..=2).contains(&fd) {
        return -err::ENOTTY;
    }
    match req {
        TCGETS => 0, // it is a terminal (isatty); the exact modes don't matter here
        TIOCGWINSZ => {
            let (cols, rows) = crate::console::size();
            let buf = match user_bytes(arg, 8) {
                Ok(b) => b,
                Err(e) => return e,
            };
            buf[0..2].copy_from_slice(&(rows as u16).to_le_bytes());
            buf[2..4].copy_from_slice(&(cols as u16).to_le_bytes());
            buf[4..8].fill(0);
            0
        }
        _ => -err::ENOTTY,
    }
}

fn sys_fcntl(fd: i32, cmd: u64, _arg: u64) -> i64 {
    const F_GETFL: u64 = 3;
    const F_SETFL: u64 = 4;
    const F_GETFD: u64 = 1;
    const F_SETFD: u64 = 2;
    let exists = (0..=2).contains(&fd) || with_state(|st| st.files.contains_key(&fd));
    if !exists {
        return -err::EBADF;
    }
    match cmd {
        F_GETFL | F_GETFD => 0,
        F_SETFL | F_SETFD => 0,
        _ => -err::EINVAL,
    }
}

fn sys_access(path_ptr: u64) -> i64 {
    let text = match user::user_cstr(path_ptr, 4096) {
        Ok(t) => t,
        Err(e) => return -errno(e),
    };
    match linux_path(&text) {
        Ok(p) if fs::exists(&p) => 0,
        Ok(_) => -err::ENOENT,
        Err(e) => e,
    }
}

// ----------------------------------------------------------------------------
// cwd

fn sys_getcwd(ptr: u64, cap: u64) -> i64 {
    let cwd = match user::cwd() {
        Ok(c) => c,
        Err(e) => return -errno(e),
    };
    let text = show_path(&cwd);
    let bytes = text.as_bytes();
    if bytes.len() + 1 > cap as usize {
        return -err::ERANGE;
    }
    let buf = match user_bytes(ptr, bytes.len() as u64 + 1) {
        Ok(b) => b,
        Err(e) => return e,
    };
    buf[..bytes.len()].copy_from_slice(bytes);
    buf[bytes.len()] = 0;
    bytes.len() as i64 + 1
}

fn sys_chdir(path_ptr: u64) -> i64 {
    let text = match user::user_cstr(path_ptr, 4096) {
        Ok(t) => t,
        Err(e) => return -errno(e),
    };
    let path = match linux_path(&text) {
        Ok(p) => p,
        Err(e) => return e,
    };
    if !fs::is_dir(&path) {
        return -err::ENOTDIR;
    }
    user::set_cwd(path);
    0
}

// ----------------------------------------------------------------------------
// time / random / uname / limits

fn sys_nanosleep(req: u64) -> i64 {
    let buf = match user_bytes(req, 16) {
        Ok(b) => b,
        Err(e) => return e,
    };
    let secs = u64::from_le_bytes(buf[0..8].try_into().unwrap());
    let nsec = u64::from_le_bytes(buf[8..16].try_into().unwrap());
    task::sleep_ms((secs.saturating_mul(1000) + nsec / 1_000_000).min(3_600_000));
    0
}

fn sys_gettimeofday(tv: u64) -> i64 {
    if tv == 0 {
        return 0;
    }
    let buf = match user_bytes(tv, 16) {
        Ok(b) => b,
        Err(e) => return e,
    };
    let now = crate::time::now() + UNIX_2000;
    buf[0..8].copy_from_slice(&now.to_le_bytes());
    let us = (crate::time::uptime_ms() % 1000) * 1000;
    buf[8..16].copy_from_slice(&us.to_le_bytes());
    0
}

fn sys_clock_gettime(_id: u64, tp: u64) -> i64 {
    let buf = match user_bytes(tp, 16) {
        Ok(b) => b,
        Err(e) => return e,
    };
    let ms = crate::time::uptime_ms();
    let secs = crate::time::now() + UNIX_2000;
    buf[0..8].copy_from_slice(&secs.to_le_bytes());
    buf[8..16].copy_from_slice(&((ms % 1000) * 1_000_000).to_le_bytes());
    0
}

fn sys_getrandom(ptr: u64, len: u64) -> i64 {
    let buf = match user_bytes(ptr, len) {
        Ok(b) => b,
        Err(e) => return e,
    };
    let mut state = crate::time::now().wrapping_mul(6364136223846793005).wrapping_add(crate::time::uptime_ms() | 1);
    for b in buf.iter_mut() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        *b = (state >> 33) as u8;
    }
    len as i64
}

fn sys_uname(buf: u64) -> i64 {
    // struct utsname: six fixed 65-byte fields.
    let out = match user_bytes(buf, 65 * 6) {
        Ok(b) => b,
        Err(e) => return e,
    };
    out.fill(0);
    let fields = ["Linux", "arcos", "6.0.0-arcos", "#1 arcOS", "x86_64", "(none)"];
    for (i, f) in fields.iter().enumerate() {
        let at = i * 65;
        out[at..at + f.len()].copy_from_slice(f.as_bytes());
    }
    0
}

fn sys_prlimit(new: u64, old: u64) -> i64 {
    if new != 0 {
        return 0; // ignore changes
    }
    if old != 0 {
        let buf = match user_bytes(old, 16) {
            Ok(b) => b,
            Err(e) => return e,
        };
        // A generous stack/fd limit; RLIM_INFINITY for the rest reads fine too.
        buf[0..8].copy_from_slice(&(8 * 1024 * 1024u64).to_le_bytes());
        buf[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
    }
    0
}
