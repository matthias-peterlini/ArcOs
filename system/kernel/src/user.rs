//! User programs: processes, their address spaces and the system calls.
//!
//! A program is a position-independent ELF image (`.app`). Each process has
//! its own address space (the kernel half is shared), one thread, a table of
//! handles and a current folder. System calls arrive through `int 0x80`; see
//! `arcdk::syscall` for the interface.

use crate::arch::{cpu, gdt, idt::TrapFrame};
use crate::fs::{self, Path};
use crate::mem::{self, frame, paging, PAGE_SIZE};
use crate::sync::IrqLock;
use crate::task::{self, Tid, WaitQueue};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use arcdk::syscall::{self as sys, Error};
use core::ffi::c_void;

const STACK_PAGES: u64 = 64;
const USER_TOP: u64 = 0x0000_8000_0000_0000;
const MAX_HANDLES: usize = 64;

#[derive(Clone)]
enum Handle {
    File { path: Path, pos: u64, write: bool },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Running,
    Exited(i64),
}

struct Process {
    name: String,
    cr3: u64,
    tid: Tid,
    handles: Vec<Option<Handle>>,
    heap_next: u64,
    cwd: Path,
    state: State,
    kill: bool,
}

static PROCS: IrqLock<BTreeMap<u32, Process>> = IrqLock::new(BTreeMap::new());
static NEXT_PID: IrqLock<u32> = IrqLock::new(1);
static FOREGROUND: IrqLock<Option<u32>> = IrqLock::new(None);
static EXITED: WaitQueue = WaitQueue::new();

fn fs_error(e: fs::Error) -> Error {
    match e {
        fs::Error::NotFound => Error::NotFound,
        fs::Error::Exists => Error::Exists,
        fs::Error::NotDir => Error::NotDir,
        fs::Error::IsDir => Error::IsDir,
        fs::Error::NotEmpty => Error::NotEmpty,
        fs::Error::NoSpace | fs::Error::NoNodes | fs::Error::TooFragmented => Error::NoSpace,
        fs::Error::NameTooLong | fs::Error::InvalidName => Error::InvalidName,
        fs::Error::ReadOnly => Error::ReadOnly,
        fs::Error::Corrupt | fs::Error::Io => Error::Io,
    }
}

/// Where a program name points: `edit` -> `sys:/apps/edit.app`, else
/// `edit.arc` or `edit.as` there (ArcScript programs); paths are taken as
/// they are.
pub fn find_program(name: &str, cwd: &Path) -> Option<Path> {
    if name.contains(':') || name.contains('/') || name.ends_with(".app") || is_script(name) {
        let p = Path::parse(name, cwd).ok()?;
        return fs::stat(&p).ok().filter(|s| s.kind == arcfs::Kind::File).map(|_| p);
    }
    ["app", "arc", "as"].iter().find_map(|ext| {
        let p = fs::abs(&alloc::format!("sys:/apps/{}.{ext}", name.to_ascii_lowercase())).ok()?;
        fs::exists(&p).then_some(p)
    })
}

/// ArcScript packages and sources, which `arcc` runs.
fn is_script(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".arc") || lower.ends_with(".as")
}

const SCRIPT_RUNNER: &str = "sys:/apps/arcc.app";

struct Start {
    image: Vec<u8>,
    args: String,
}

/// Starts a program. Returns its process id.
pub fn spawn(path: &Path, args: &str, cwd: Path) -> Result<u32, Error> {
    let name = path.name();
    if is_script(name) {
        // `arcc run <file> <args>`, under the script's own name.
        let runner = fs::abs(SCRIPT_RUNNER).map_err(fs_error)?;
        let image = fs::read_file(&runner).map_err(fs_error)?;
        let shown = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
        let file = alloc::format!("{path}");
        let file = if file.contains(char::is_whitespace) { alloc::format!("\"{file}\"") } else { file };
        return start(image, String::from(shown), &alloc::format!("run {file} {args}"), cwd);
    }
    let image = fs::read_file(path).map_err(fs_error)?;
    start(image, String::from(name.trim_end_matches(".app")), args, cwd)
}

fn start(image: Vec<u8>, name: String, args: &str, cwd: Path) -> Result<u32, Error> {
    if image.len() < 64 || &image[..4] != b"\x7fELF" {
        return Err(Error::BadProgram);
    }
    let cr3 = paging::new_address_space().ok_or(Error::NoMemory)?;
    let pid = {
        let mut n = NEXT_PID.lock();
        let pid = *n;
        *n += 1;
        pid
    };
    PROCS.lock().insert(
        pid,
        Process {
            name: name.clone(),
            cr3,
            tid: 0,
            handles: Vec::new(),
            heap_next: sys::USER_HEAP_BASE,
            cwd,
            state: State::Running,
            kill: false,
        },
    );
    let start = Box::into_raw(Box::new(Start { image, args: String::from(args) }));
    match task::spawn_in(&name, user_start, start as *mut c_void, cr3, pid) {
        Some(tid) => {
            if let Some(p) = PROCS.lock().get_mut(&pid) {
                p.tid = tid;
            }
            Ok(pid)
        }
        None => {
            // SAFETY: the thread was not created; take the box back.
            drop(unsafe { Box::from_raw(start) });
            PROCS.lock().remove(&pid);
            paging::free_address_space(cr3);
            Err(Error::NoMemory)
        }
    }
}

/// Maps `pages` fresh user pages at `virt` in the running address space.
fn map_user(virt: u64, pages: u64) -> Option<()> {
    let cr3 = cpu::read_cr3();
    for i in 0..pages {
        let phys = frame::alloc()?;
        paging::map_in(cr3, virt + i * PAGE_SIZE, phys, paging::USER | paging::WRITE)?;
    }
    Some(())
}

/// First code of every process thread (already in the new address space).
unsafe extern "C" fn user_start(arg: *mut c_void) {
    // SAFETY: the Box leaked by spawn, used once.
    let start = unsafe { Box::from_raw(arg as *mut Start) };
    let loaded = crate::driver::elf::load_with(&start.image, |pages| map_user(sys::USER_BASE, pages).map(|_| sys::USER_BASE), |_| {});
    let image = match loaded {
        Ok(i) => i,
        Err(e) => {
            crate::println!("\x1b[91m{}: {e}\x1b[0m", current_name());
            terminate(-(Error::BadProgram as i64));
        }
    };
    let stack_bottom = sys::USER_STACK_TOP - STACK_PAGES * PAGE_SIZE;
    if map_user(stack_bottom, STACK_PAGES).is_none() {
        terminate(-(Error::NoMemory as i64));
    }
    // Arguments at the top of the stack.
    let args = start.args.as_bytes();
    let args_at = (sys::USER_STACK_TOP - args.len() as u64 - 1) & !15;
    // SAFETY: the stack pages were just mapped in this address space.
    unsafe { core::ptr::copy_nonoverlapping(args.as_ptr(), args_at as *mut u8, args.len()) };
    let rsp = args_at - 8; // as if called: rsp % 16 == 8 at entry
    let (entry, len) = (image.entry, args.len() as u64);
    drop(start);
    // SAFETY: jumps to ring 3 at the program's entry with a valid stack.
    unsafe {
        core::arch::asm!(
            "push {ss}",
            "push {rsp}",
            "push 0x202",
            "push {cs}",
            "push {rip}",
            "xor eax, eax",
            "xor ebx, ebx",
            "xor ecx, ecx",
            "xor edx, edx",
            "xor ebp, ebp",
            "xor r8d, r8d",
            "xor r9d, r9d",
            "xor r10d, r10d",
            "xor r11d, r11d",
            "xor r12d, r12d",
            "xor r13d, r13d",
            "xor r14d, r14d",
            "xor r15d, r15d",
            "iretq",
            ss = in(reg) gdt::USER_DATA as u64,
            rsp = in(reg) rsp,
            cs = in(reg) gdt::USER_CODE as u64,
            rip = in(reg) entry,
            in("rdi") args_at,
            in("rsi") len,
            options(noreturn)
        );
    }
}

fn current_name() -> String {
    let pid = task::current_pid();
    PROCS.lock().get(&pid).map(|p| p.name.clone()).unwrap_or_default()
}

/// Ends the calling process.
fn terminate(code: i64) -> ! {
    let pid = task::current_pid();
    cpu::disable_irq();
    let cr3 = {
        let mut procs = PROCS.lock();
        match procs.get_mut(&pid) {
            Some(p) => {
                p.state = State::Exited(code);
                p.handles.clear();
                core::mem::replace(&mut p.cr3, 0)
            }
            None => 0,
        }
    };
    if cr3 != 0 {
        task::leave_address_space();
        paging::free_address_space(cr3);
    }
    EXITED.notify_all();
    task::exit();
}

// ----------------------------------------------------------------------------
// Process control for the shell

/// Waits for a process to end and returns its exit code.
pub fn wait(pid: u32) -> Option<i64> {
    let code = EXITED.wait_for(|| match PROCS.lock().get(&pid) {
        Some(p) => match p.state {
            State::Exited(c) => Some(Some(c)),
            State::Running => None,
        },
        None => Some(None),
    });
    PROCS.lock().remove(&pid);
    code
}

/// Runs a program in the foreground: Ctrl+C stops it.
pub fn run_foreground(path: &Path, args: &str, cwd: Path) -> Result<i64, Error> {
    let pid = spawn(path, args, cwd)?;
    *FOREGROUND.lock() = Some(pid);
    let code = wait(pid);
    *FOREGROUND.lock() = None;
    Ok(code.unwrap_or(-1))
}

/// Ctrl+C: asks the foreground program to stop. Returns whether there was one.
pub fn interrupt_foreground() -> bool {
    let Some(pid) = *FOREGROUND.lock() else { return false };
    kill(pid)
}

pub fn kill(pid: u32) -> bool {
    match PROCS.lock().get_mut(&pid) {
        Some(p) if p.state == State::Running => {
            p.kill = true;
            true
        }
        _ => false,
    }
}

fn kill_requested() -> bool {
    let pid = task::current_pid();
    PROCS.lock().get(&pid).is_some_and(|p| p.kill)
}

/// Called on timer interrupts that hit user mode.
pub fn check_kill(frame: &TrapFrame) {
    if frame.is_user() && kill_requested() {
        cpu::enable_irq();
        terminate(-130);
    }
}

pub fn count() -> usize {
    PROCS.lock().values().filter(|p| p.state == State::Running).count()
}

// ----------------------------------------------------------------------------
// User memory

/// Checks that `len` bytes at `ptr` are mapped user memory of the running
/// process and returns them.
fn user_slice<'a>(ptr: u64, len: u64) -> Result<&'a mut [u8], Error> {
    if len == 0 {
        return Ok(&mut []);
    }
    let end = ptr.checked_add(len).ok_or(Error::BadAddress)?;
    if ptr == 0 || end > USER_TOP {
        return Err(Error::BadAddress);
    }
    let cr3 = cpu::read_cr3();
    let mut page = mem::align_down(ptr, PAGE_SIZE);
    while page < end {
        if !paging::user_accessible(cr3, page) {
            return Err(Error::BadAddress);
        }
        page += PAGE_SIZE;
    }
    // SAFETY: every page of the range is mapped for user access in the
    // current address space, and the process has a single thread.
    Ok(unsafe { core::slice::from_raw_parts_mut(ptr as *mut u8, len as usize) })
}

fn user_str<'a>(ptr: u64, len: u64) -> Result<&'a str, Error> {
    if len > 4096 {
        return Err(Error::InvalidName);
    }
    core::str::from_utf8(user_slice(ptr, len)?).map_err(|_| Error::InvalidName)
}

fn user_path(ptr: u64, len: u64) -> Result<Path, Error> {
    let text = user_str(ptr, len)?;
    let pid = task::current_pid();
    let cwd = PROCS.lock().get(&pid).map(|p| p.cwd.clone()).ok_or(Error::Unknown)?;
    Path::parse(text, &cwd).map_err(fs_error)
}

// ----------------------------------------------------------------------------
// System calls

fn key_code(ev: crate::dev::input::KeyEvent) -> u64 {
    use crate::dev::input::Key;
    let code = match ev.key {
        Key::Char(c) => c as u32,
        Key::Enter => sys::KEY_ENTER,
        Key::Backspace => sys::KEY_BACKSPACE,
        Key::Delete => sys::KEY_DELETE,
        Key::Tab => sys::KEY_TAB,
        Key::Escape => sys::KEY_ESCAPE,
        Key::Up => sys::KEY_UP,
        Key::Down => sys::KEY_DOWN,
        Key::Left => sys::KEY_LEFT,
        Key::Right => sys::KEY_RIGHT,
        Key::Home => sys::KEY_HOME,
        Key::End => sys::KEY_END,
        Key::PageUp => sys::KEY_PAGE_UP,
        Key::PageDown => sys::KEY_PAGE_DOWN,
        Key::Insert => sys::KEY_INSERT,
        Key::F(n) => sys::KEY_F + n as u32,
    };
    let mut v = code as u64;
    if ev.ctrl {
        v |= sys::MOD_CTRL;
    }
    if ev.alt {
        v |= sys::MOD_ALT;
    }
    if ev.shift {
        v |= sys::MOD_SHIFT;
    }
    v
}

fn with_handle<T>(h: u64, f: impl FnOnce(&mut Handle) -> T) -> Result<T, Error> {
    let pid = task::current_pid();
    let mut procs = PROCS.lock();
    let p = procs.get_mut(&pid).ok_or(Error::Unknown)?;
    let slot = h.checked_sub(1).and_then(|i| p.handles.get_mut(i as usize)).ok_or(Error::BadHandle)?;
    slot.as_mut().map(f).ok_or(Error::BadHandle)
}

fn call(n: u64, a: [u64; 5]) -> Result<u64, Error> {
    match n {
        sys::WRITE => {
            let data = user_slice(a[1], a[2])?;
            if a[0] == sys::CONSOLE {
                crate::print!("{}", String::from_utf8_lossy(data));
                return Ok(a[2]);
            }
            let (path, pos, write) = with_handle(a[0], |h| match h {
                Handle::File { path, pos, write } => (path.clone(), *pos, *write),
            })?;
            if !write {
                return Err(Error::ReadOnly);
            }
            let data = data.to_vec();
            fs::write_at(&path, pos, &data).map_err(fs_error)?;
            with_handle(a[0], |h| match h {
                Handle::File { pos, .. } => *pos += data.len() as u64,
            })?;
            Ok(data.len() as u64)
        }
        sys::READ => {
            let (path, pos) = with_handle(a[0], |h| match h {
                Handle::File { path, pos, .. } => (path.clone(), *pos),
            })?;
            let buf = user_slice(a[1], a[2])?;
            let mut tmp = alloc::vec![0u8; buf.len().min(1 << 20)];
            let n = fs::read_at(&path, pos, &mut tmp).map_err(fs_error)?;
            buf[..n].copy_from_slice(&tmp[..n]);
            with_handle(a[0], |h| match h {
                Handle::File { pos, .. } => *pos += n as u64,
            })?;
            Ok(n as u64)
        }
        sys::READ_KEY => {
            let until = if a[0] == u64::MAX { u64::MAX } else { crate::time::uptime_ms().saturating_add(a[0]) };
            loop {
                if let Some(ev) = crate::dev::input::try_read_key() {
                    return Ok(key_code(ev));
                }
                if kill_requested() {
                    terminate(-130);
                }
                if crate::time::uptime_ms() >= until {
                    return Ok(0);
                }
                task::sleep_ms(5);
            }
        }
        sys::OPEN => {
            let path = user_path(a[0], a[1])?;
            let mode = a[2];
            match fs::stat(&path) {
                Ok(st) if st.kind == arcfs::Kind::Dir => return Err(Error::IsDir),
                Ok(_) => {
                    if mode == sys::MODE_WRITE {
                        fs::write_file(&path, &[]).map_err(fs_error)?;
                    }
                }
                Err(fs::Error::NotFound) if mode != sys::MODE_READ => fs::write_file(&path, &[]).map_err(fs_error)?,
                Err(e) => return Err(fs_error(e)),
            }
            let pos = if mode == sys::MODE_APPEND { fs::stat(&path).map(|s| s.size).unwrap_or(0) } else { 0 };
            let pid = task::current_pid();
            let mut procs = PROCS.lock();
            let p = procs.get_mut(&pid).ok_or(Error::Unknown)?;
            let handle = Handle::File { path, pos, write: mode != sys::MODE_READ };
            let slot = match p.handles.iter().position(|h| h.is_none()) {
                Some(i) => i,
                None if p.handles.len() < MAX_HANDLES => {
                    p.handles.push(None);
                    p.handles.len() - 1
                }
                None => return Err(Error::NoMemory),
            };
            p.handles[slot] = Some(handle);
            Ok(slot as u64 + 1)
        }
        sys::CLOSE => {
            let pid = task::current_pid();
            let mut procs = PROCS.lock();
            let p = procs.get_mut(&pid).ok_or(Error::Unknown)?;
            let slot = a[0].checked_sub(1).and_then(|i| p.handles.get_mut(i as usize)).ok_or(Error::BadHandle)?;
            slot.take().ok_or(Error::BadHandle)?;
            Ok(0)
        }
        sys::SEEK => with_handle(a[0], |h| match h {
            Handle::File { pos, .. } => {
                *pos = a[1];
                0
            }
        }),
        sys::FILE_SIZE => {
            let path = with_handle(a[0], |h| match h {
                Handle::File { path, .. } => path.clone(),
            })?;
            fs::stat(&path).map(|s| s.size).map_err(fs_error)
        }
        sys::LIST_DIR => {
            let path = user_path(a[0], a[1])?;
            let mut text = String::new();
            for (e, st) in fs::list_dir(&path).map_err(fs_error)? {
                let kind = if st.kind == arcfs::Kind::Dir { 'd' } else { 'f' };
                text.push_str(&alloc::format!("{}\t{}\t{}\n", e.name, kind, st.size));
            }
            let buf = user_slice(a[2], a[3])?;
            let n = text.len().min(buf.len());
            buf[..n].copy_from_slice(&text.as_bytes()[..n]);
            Ok(text.len() as u64)
        }
        sys::REMOVE => fs::remove(&user_path(a[0], a[1])?).map(|_| 0).map_err(fs_error),
        sys::MAKE_DIR => fs::make_dir(&user_path(a[0], a[1])?).map(|_| 0).map_err(fs_error),
        sys::RENAME => {
            let from = user_path(a[0], a[1])?;
            let to = user_path(a[2], a[3])?;
            fs::rename(&from, &to).map(|_| 0).map_err(fs_error)
        }
        sys::UPTIME => Ok(crate::time::uptime_ms()),
        sys::SLEEP => {
            task::sleep_ms(a[0].min(3_600_000));
            Ok(0)
        }
        sys::NOW => Ok(crate::time::now()),
        sys::YIELD => {
            task::yield_now();
            Ok(0)
        }
        sys::SPAWN => {
            let name = user_str(a[0], a[1])?.to_string();
            let args = user_str(a[2], a[3])?.to_string();
            let pid = task::current_pid();
            let cwd = PROCS.lock().get(&pid).map(|p| p.cwd.clone()).ok_or(Error::Unknown)?;
            let path = find_program(&name, &cwd).ok_or(Error::NotFound)?;
            spawn(&path, &args, cwd).map(|p| p as u64)
        }
        sys::WAIT => wait(a[0] as u32).map(|c| c as u64).ok_or(Error::NotFound),
        sys::MAP => {
            let pages = mem::align_up(a[0].max(1), PAGE_SIZE) / PAGE_SIZE;
            if pages > 1 << 18 {
                return Err(Error::NoMemory);
            }
            let pid = task::current_pid();
            let at = {
                let mut procs = PROCS.lock();
                let p = procs.get_mut(&pid).ok_or(Error::Unknown)?;
                let at = p.heap_next;
                p.heap_next += pages * PAGE_SIZE;
                at
            };
            map_user(at, pages).ok_or(Error::NoMemory)?;
            Ok(at)
        }
        sys::SYSINFO => {
            let out = user_slice(a[0], (sys::SYSINFO_WORDS * 8) as u64)?;
            let (free, total) = frame::stats();
            let devs = crate::dev::list();
            let words = [
                total,
                free,
                crate::time::uptime_ms(),
                task::list().len() as u64,
                count() as u64,
                devs.len() as u64,
                devs.iter().filter(|d| d.has_driver()).count() as u64,
                crate::dev::block::list().iter().filter(|d| d.parent.is_none()).count() as u64,
            ];
            for (i, w) in words.iter().enumerate() {
                out[i * 8..i * 8 + 8].copy_from_slice(&w.to_le_bytes());
            }
            Ok(sys::SYSINFO_WORDS as u64)
        }
        sys::SCREEN => {
            let (cols, rows) = crate::console::size();
            Ok((cols as u64) << 32 | rows as u64)
        }
        sys::GET_CWD => {
            let pid = task::current_pid();
            let cwd = PROCS.lock().get(&pid).map(|p| p.cwd.to_string()).ok_or(Error::Unknown)?;
            let buf = user_slice(a[0], a[1])?;
            let n = cwd.len().min(buf.len());
            buf[..n].copy_from_slice(&cwd.as_bytes()[..n]);
            Ok(cwd.len() as u64)
        }
        sys::SET_CWD => {
            let path = user_path(a[0], a[1])?;
            if !fs::is_dir(&path) {
                return Err(Error::NotDir);
            }
            let pid = task::current_pid();
            if let Some(p) = PROCS.lock().get_mut(&pid) {
                p.cwd = path;
            }
            Ok(0)
        }
        _ => Err(Error::Unknown),
    }
}

pub fn syscall(frame: &mut TrapFrame) {
    if !frame.is_user() {
        frame.rax = (-(Error::Unknown as i64)) as u64;
        return;
    }
    // System calls may block or take long: let interrupts in.
    cpu::enable_irq();
    let n = frame.rax;
    if n == sys::EXIT {
        terminate(frame.rdi as i64);
    }
    let args = [frame.rdi, frame.rsi, frame.rdx, frame.r10, frame.r8];
    frame.rax = match call(n, args) {
        Ok(v) => v,
        Err(e) => (-(e as i64)) as u64,
    };
    if kill_requested() {
        terminate(-130);
    }
    cpu::disable_irq();
}

pub fn fault(frame: &mut TrapFrame, what: &str) {
    cpu::enable_irq();
    let extra = if frame.vector == 14 { alloc::format!(" (address {:#x})", cpu::read_cr2()) } else { String::new() };
    crate::println!("\x1b[91m{} stopped: {what} at {:#x}{extra}\x1b[0m", current_name(), frame.rip);
    terminate(-139);
}
