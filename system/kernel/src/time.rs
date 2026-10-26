//! Time: the 1 kHz system tick and the battery-backed real-time clock.
//!
//! Wall-clock time is kept as seconds since 2000-01-01 00:00 (the "Arc
//! epoch"), read from the CMOS clock at boot and advanced by the tick.

use crate::arch::cpu::{inb, outb};
use crate::arch::pit::HZ;
use core::sync::atomic::{AtomicU64, Ordering};

static TICKS: AtomicU64 = AtomicU64::new(0);
static BOOT_TIME: AtomicU64 = AtomicU64::new(0);

pub fn tick() {
    TICKS.fetch_add(1, Ordering::Relaxed);
}

pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

pub fn uptime_ms() -> u64 {
    ticks() * 1000 / HZ
}

/// Seconds since 2000-01-01 00:00 (local time as kept by the CMOS clock).
pub fn now() -> u64 {
    BOOT_TIME.load(Ordering::Relaxed) + uptime_ms() / 1000
}

/// Seconds since 1970-01-01 (Unix time), taking the clock as UTC.
pub fn unix_now() -> u64 {
    now() + 946_684_800
}

fn cmos(reg: u8) -> u8 {
    outb(0x70, reg);
    inb(0x71)
}

fn read_rtc_raw() -> [u8; 7] {
    while cmos(0x0A) & 0x80 != 0 {
        core::hint::spin_loop();
    }
    [cmos(0x00), cmos(0x02), cmos(0x04), cmos(0x07), cmos(0x08), cmos(0x09), cmos(0x32)]
}

pub fn init() {
    // Read twice until stable: the clock may tick between register reads.
    let mut v = read_rtc_raw();
    loop {
        let w = read_rtc_raw();
        if w == v {
            break;
        }
        v = w;
    }
    let status_b = cmos(0x0B);
    let bcd = |x: u8| if status_b & 4 == 0 { (x & 0x0F) + (x >> 4) * 10 } else { x };
    let sec = bcd(v[0]) as u64;
    let hour_raw = v[2];
    let mut hour = bcd(hour_raw & 0x7F) as u64;
    if status_b & 2 == 0 && hour_raw & 0x80 != 0 {
        hour = (hour % 12) + 12; // 12-hour clock, PM
    }
    let min = bcd(v[1]) as u64;
    let day = bcd(v[3]) as u64;
    let month = bcd(v[4]) as u64;
    let mut year = bcd(v[5]) as u64;
    let century = bcd(v[6]) as u64;
    year += if (19..=21).contains(&century) { century * 100 } else { 2000 };
    BOOT_TIME.store(to_epoch(year, month, day, hour, min, sec), Ordering::Relaxed);
}

fn is_leap(y: u64) -> bool {
    (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400)
}

pub fn days_in_month(y: u64, m: u64) -> u64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
    }
}

pub fn to_epoch(year: u64, month: u64, day: u64, hour: u64, min: u64, sec: u64) -> u64 {
    let year = year.max(2000);
    let mut days = 0;
    for y in 2000..year {
        days += if is_leap(y) { 366 } else { 365 };
    }
    for m in 1..month.clamp(1, 12) {
        days += days_in_month(year, m);
    }
    days += day.max(1) - 1;
    ((days * 24 + hour) * 60 + min) * 60 + sec
}

#[derive(Clone, Copy, Debug)]
pub struct DateTime {
    pub year: u64,
    pub month: u64,
    pub day: u64,
    pub hour: u64,
    pub min: u64,
    pub sec: u64,
    /// 0 = Monday
    pub weekday: u64,
}

pub fn from_epoch(secs: u64) -> DateTime {
    let mut days = secs / 86400;
    let rem = secs % 86400;
    let weekday = (days + 5) % 7; // 2000-01-01 was a Saturday
    let mut year = 2000;
    loop {
        let len = if is_leap(year) { 366 } else { 365 };
        if days < len {
            break;
        }
        days -= len;
        year += 1;
    }
    let mut month = 1;
    while days >= days_in_month(year, month) {
        days -= days_in_month(year, month);
        month += 1;
    }
    DateTime { year, month, day: days + 1, hour: rem / 3600, min: rem / 60 % 60, sec: rem % 60, weekday }
}

pub const WEEKDAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

/// Busy-waits for about `us` microseconds (before the scheduler runs).
pub fn spin_us(us: u64) {
    for _ in 0..us {
        crate::arch::cpu::io_wait();
    }
}
