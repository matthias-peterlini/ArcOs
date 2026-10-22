//! Threads and the scheduler.
//!
//! Preemptive round robin on a single CPU: the timer gives every thread a
//! 10 ms slice. Threads sleep with [`sleep_ms`], wait for events with
//! [`WaitQueue`], and end with [`exit`] (or by returning). A thread's stack
//! is freed by the next thread that runs, never by the thread itself.

use crate::arch::{cpu, gdt};
use crate::mem::{self, region};
use crate::sync::IrqLock;
use alloc::boxed::Box;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::vec::Vec;
use core::arch::global_asm;
use core::ffi::c_void;

pub type Tid = u32;

const STACK_PAGES: u64 = 16; // 64 KiB
const SLICE_TICKS: u32 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Ready,
    Running,
    Sleeping,
    Blocked,
    Dead,
}

impl State {
    pub fn name(self) -> &'static str {
        match self {
            State::Ready => "ready",
            State::Running => "running",
            State::Sleeping => "sleeping",
            State::Blocked => "waiting",
            State::Dead => "ended",
        }
    }
}

struct Thread {
    tid: Tid,
    name: String,
    state: State,
    rsp: u64,
    /// Lowest address of the stack (0 for the boot thread's static stack).
    stack: u64,
    wake_at: u64,
    cpu_ticks: u64,
    /// Address space (0 = kernel only).
    cr3: u64,
    /// The process this thread belongs to (0 = kernel).
    pid: u32,
}

impl Thread {
    fn stack_top(&self) -> u64 {
        self.stack + STACK_PAGES * mem::PAGE_SIZE
    }
}

struct Sched {
    threads: BTreeMap<Tid, Box<Thread>>,
    ready: VecDeque<Tid>,
    sleepers: Vec<Tid>,
    current: Tid,
    idle: Tid,
    next_tid: Tid,
    zombies: Vec<Box<Thread>>,
    slice: u32,
    started: bool,
}

static SCHED: IrqLock<Sched> = IrqLock::new(Sched {
    threads: BTreeMap::new(),
    ready: VecDeque::new(),
    sleepers: Vec::new(),
    current: 0,
    idle: 0,
    next_tid: 1,
    zombies: Vec::new(),
    slice: SLICE_TICKS,
    started: false,
});

global_asm!(
    r#"
    .global context_switch
context_switch:
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15
    mov [rdi], rsp
    mov rsp, rsi
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    ret

    .global thread_trampoline
thread_trampoline:
    mov rdi, r12
    mov rsi, r13
    call thread_start
    ud2
"#
);

extern "C" {
    fn context_switch(old_rsp: *mut u64, new_rsp: u64);
    fn thread_trampoline();
}

type Entry = unsafe extern "C" fn(*mut c_void);

#[no_mangle]
extern "C" fn thread_start(entry: Entry, arg: *mut c_void) -> ! {
    after_switch();
    cpu::enable_irq();
    // SAFETY: entry/arg were given to spawn_raw together.
    unsafe { entry(arg) };
    exit();
}

/// Turns the boot code into thread 1 and creates the idle thread.
pub fn init() {
    let mut s = SCHED.lock();
    let tid = s.next_tid;
    s.next_tid += 1;
    s.threads.insert(
        tid,
        Box::new(Thread {
            tid,
            name: String::from("init"),
            state: State::Running,
            rsp: 0,
            stack: 0,
            wake_at: 0,
            cpu_ticks: 0,
            cr3: 0,
            pid: 0,
        }),
    );
    s.current = tid;
    drop(s);
    let idle = spawn_raw("idle", idle_main, core::ptr::null_mut()).expect("cannot create the idle thread");
    let mut s = SCHED.lock();
    s.ready.retain(|&t| t != idle);
    s.idle = idle;
    s.started = true;
}

unsafe extern "C" fn idle_main(_: *mut c_void) {
    loop {
        cpu::enable_and_halt();
    }
}

/// Starts a thread running `entry(arg)`.
pub fn spawn_raw(name: &str, entry: Entry, arg: *mut c_void) -> Option<Tid> {
    spawn_in(name, entry, arg, 0, 0)
}

pub(crate) fn spawn_in(name: &str, entry: Entry, arg: *mut c_void, cr3: u64, pid: u32) -> Option<Tid> {
    let stack = region::alloc_pages(&region::STACKS, STACK_PAGES, 1)?;
    let top = stack + STACK_PAGES * mem::PAGE_SIZE;
    let rsp = top - 9 * 8;
    let frame = [0u64, 0, arg as u64, entry as usize as u64, 0, 0, thread_trampoline as usize as u64, 0, 0];
    // SAFETY: the stack was just mapped and is owned by the new thread.
    unsafe { core::ptr::copy_nonoverlapping(frame.as_ptr(), rsp as *mut u64, frame.len()) };
    let mut s = SCHED.lock();
    let tid = s.next_tid;
    s.next_tid += 1;
    s.threads.insert(
        tid,
        Box::new(Thread { tid, name: String::from(name), state: State::Ready, rsp, stack, wake_at: 0, cpu_ticks: 0, cr3, pid }),
    );
    s.ready.push_back(tid);
    Some(tid)
}

/// Starts a thread running a closure.
pub fn spawn<F: FnOnce() + Send + 'static>(name: &str, f: F) -> Option<Tid> {
    unsafe extern "C" fn run<F: FnOnce()>(arg: *mut c_void) {
        // SAFETY: arg is the Box leaked below, used exactly once.
        let f = unsafe { Box::from_raw(arg as *mut F) };
        f();
    }
    let arg = Box::into_raw(Box::new(f)) as *mut c_void;
    let tid = spawn_raw(name, run::<F>, arg);
    if tid.is_none() {
        // SAFETY: the thread was not created; reclaim the closure.
        drop(unsafe { Box::from_raw(arg as *mut F) });
    }
    tid
}

/// Frees the stacks of threads that have ended (never the running one).
fn after_switch() {
    let zombies = core::mem::take(&mut SCHED.lock().zombies);
    for z in zombies {
        if z.stack != 0 {
            region::free_pages(&region::STACKS, z.stack, STACK_PAGES, 1);
        }
    }
}

/// Picks the next thread and switches to it.
fn schedule() {
    let irq = cpu::irq_save();
    let switch = {
        let mut s = SCHED.lock();
        if !s.started {
            None
        } else {
            let cur = s.current;
            let idle = s.idle;
            let state = s.threads.get(&cur).map(|t| t.state);
            if state == Some(State::Running) {
                s.threads.get_mut(&cur).unwrap().state = State::Ready;
                if cur != idle {
                    s.ready.push_back(cur);
                }
            }
            let next = loop {
                match s.ready.pop_front() {
                    Some(t) if s.threads.get(&t).is_some_and(|t| t.state == State::Ready) => break t,
                    Some(_) => continue,
                    None => break idle,
                }
            };
            s.slice = SLICE_TICKS;
            let n = s.threads.get_mut(&next).unwrap();
            n.state = State::Running;
            if next == cur {
                None
            } else {
                let (new_rsp, top, cr3) = (n.rsp, n.stack_top(), n.cr3);
                if n.stack != 0 {
                    gdt::set_kernel_stack(top);
                }
                let target = if cr3 != 0 { cr3 } else { mem::paging::kernel_pml4() };
                if cpu::read_cr3() != target {
                    // SAFETY: every address space maps the kernel half.
                    unsafe { cpu::write_cr3(target) };
                }
                s.current = next;
                let old_ptr: *mut u64 = if state == Some(State::Dead) {
                    let z = s.threads.remove(&cur).unwrap();
                    s.zombies.push(z);
                    &mut s.zombies.last_mut().unwrap().rsp
                } else {
                    &mut s.threads.get_mut(&cur).unwrap().rsp
                };
                Some((old_ptr, new_rsp))
            }
        }
    };
    if let Some((old, new)) = switch {
        // SAFETY: interrupts are off; both stacks are valid and the old
        // thread's Box stays in place until after_switch frees it.
        unsafe { context_switch(old, new) };
        after_switch();
    }
    cpu::irq_restore(irq);
}

/// Gives the CPU to another ready thread, if any.
pub fn yield_now() {
    if SCHED.lock().ready.is_empty() {
        return;
    }
    schedule();
}

/// Ends the calling thread.
pub fn exit() -> ! {
    cpu::disable_irq();
    {
        let mut s = SCHED.lock();
        let cur = s.current;
        if let Some(t) = s.threads.get_mut(&cur) {
            t.state = State::Dead;
        }
    }
    schedule();
    unreachable!("a dead thread was scheduled");
}

pub fn sleep_ms(ms: u64) {
    if ms == 0 {
        yield_now();
        return;
    }
    let irq = cpu::irq_save();
    {
        let mut s = SCHED.lock();
        if !s.started {
            drop(s);
            cpu::irq_restore(irq);
            let until = crate::time::uptime_ms() + ms;
            while crate::time::uptime_ms() < until {
                cpu::halt();
            }
            return;
        }
        let cur = s.current;
        let t = s.threads.get_mut(&cur).unwrap();
        t.state = State::Sleeping;
        t.wake_at = crate::time::uptime_ms() + ms;
        s.sleepers.push(cur);
    }
    schedule();
    cpu::irq_restore(irq);
}

/// Called by the timer interrupt.
pub fn timer_tick() {
    crate::time::tick();
    let preempt = {
        let mut s = SCHED.lock();
        if !s.started {
            return;
        }
        let now = crate::time::uptime_ms();
        let cur = s.current;
        if let Some(t) = s.threads.get_mut(&cur) {
            t.cpu_ticks += 1;
        }
        let mut i = 0;
        while i < s.sleepers.len() {
            let tid = s.sleepers[i];
            let due = match s.threads.get(&tid) {
                Some(t) if t.state == State::Sleeping => t.wake_at <= now,
                _ => {
                    s.sleepers.swap_remove(i);
                    continue;
                }
            };
            if due {
                s.sleepers.swap_remove(i);
                s.threads.get_mut(&tid).unwrap().state = State::Ready;
                s.ready.push_back(tid);
            } else {
                i += 1;
            }
        }
        s.slice = s.slice.saturating_sub(1);
        (s.slice == 0 || cur == s.idle) && !s.ready.is_empty()
    };
    if preempt {
        schedule();
    }
}

pub fn current() -> Tid {
    SCHED.lock().current
}

pub fn current_pid() -> u32 {
    let s = SCHED.lock();
    s.threads.get(&s.current).map(|t| t.pid).unwrap_or(0)
}

/// Name of the running thread, without allocating (panic path).
pub fn current_name_lossy() -> &'static str {
    match SCHED.try_lock() {
        Some(s) => match s.threads.get(&s.current) {
            // SAFETY: thread names live until the thread is reaped; this is
            // only used to print a message right away.
            Some(t) => unsafe { &*(t.name.as_str() as *const str) },
            None => "?",
        },
        None => "?",
    }
}

/// Marks the running thread as waiting; returns its id. The caller must
/// have interrupts disabled and call [`schedule_blocked`] next.
fn block_current() -> Tid {
    let mut s = SCHED.lock();
    let cur = s.current;
    s.threads.get_mut(&cur).unwrap().state = State::Blocked;
    cur
}

fn schedule_blocked() {
    schedule();
}

/// Makes a waiting thread ready again.
pub fn wake(tid: Tid) {
    let mut s = SCHED.lock();
    if let Some(t) = s.threads.get_mut(&tid) {
        if t.state == State::Blocked || t.state == State::Sleeping {
            t.state = State::Ready;
            s.ready.push_back(tid);
        }
    }
}

/// A set of threads waiting for something to happen.
pub struct WaitQueue {
    waiters: IrqLock<VecDeque<Tid>>,
}

impl Default for WaitQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl WaitQueue {
    pub const fn new() -> WaitQueue {
        WaitQueue { waiters: IrqLock::new(VecDeque::new()) }
    }

    /// Blocks until `ready()` returns `Some`. `ready` runs with interrupts
    /// disabled, so an interrupt handler cannot change the condition between
    /// the check and going to sleep.
    pub fn wait_for<T>(&self, mut ready: impl FnMut() -> Option<T>) -> T {
        let irq = cpu::irq_save();
        loop {
            if let Some(v) = ready() {
                cpu::irq_restore(irq);
                return v;
            }
            let started = SCHED.lock().started;
            if !started {
                cpu::enable_and_halt();
                cpu::disable_irq();
                continue;
            }
            let me = block_current();
            self.waiters.lock().push_back(me);
            schedule_blocked();
        }
    }

    /// Like [`wait_for`](Self::wait_for), giving up after `ms` milliseconds.
    pub fn wait_for_timeout<T>(&self, ms: u64, mut ready: impl FnMut() -> Option<T>) -> Option<T> {
        let until = crate::time::uptime_ms() + ms;
        loop {
            let irq = cpu::irq_save();
            if let Some(v) = ready() {
                cpu::irq_restore(irq);
                return Some(v);
            }
            cpu::irq_restore(irq);
            if crate::time::uptime_ms() >= until {
                return None;
            }
            sleep_ms(1);
        }
    }

    pub fn notify_all(&self) {
        let waiters = core::mem::take(&mut *self.waiters.lock());
        for t in waiters {
            wake(t);
        }
    }

    pub fn notify_one(&self) {
        let t = self.waiters.lock().pop_front();
        if let Some(t) = t {
            wake(t);
        }
    }
}

#[derive(Clone, Debug)]
pub struct ThreadInfo {
    pub tid: Tid,
    pub name: String,
    pub state: State,
    pub cpu_ms: u64,
    pub pid: u32,
}

pub fn list() -> Vec<ThreadInfo> {
    let s = SCHED.lock();
    s.threads
        .values()
        .map(|t| ThreadInfo {
            tid: t.tid,
            name: t.name.clone(),
            state: t.state,
            cpu_ms: t.cpu_ticks * 1000 / crate::arch::pit::HZ,
            pid: t.pid,
        })
        .collect()
}

/// Ends another thread (it must not hold kernel locks: only used for user
/// program threads, which are always stopped at a safe point).
pub fn kill(tid: Tid) -> bool {
    let mut s = SCHED.lock();
    if tid == s.current || tid == s.idle {
        return false;
    }
    match s.threads.remove(&tid) {
        Some(mut t) => {
            t.state = State::Dead;
            s.zombies.push(t);
            true
        }
        None => false,
    }
}

// ----------------------------------------------------------------------------
// User mode hooks (see `crate::user`).

pub fn syscall(frame: &mut crate::arch::idt::TrapFrame) {
    crate::user::syscall(frame);
}

pub fn user_fault(frame: &mut crate::arch::idt::TrapFrame, what: &str) {
    crate::user::fault(frame, what);
}
