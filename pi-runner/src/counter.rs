//! Cycle counting on Linux/aarch64, best source first.
//!
//! Unlike the DWT counter on a Cortex-M, an application core gives no single
//! obviously-correct clock, and which one you get changes what the experiment
//! can resolve. The source actually used is recorded with every capture so a
//! result is never read without knowing its measurement floor.
//!
//!   1. **PMU cycles via `perf_event_open`** — true core cycles. Needs
//!      `perf_event_paranoid <= 2` (or CAP_PERFMON). Each read is a syscall,
//!      which adds overhead, but the overhead is common to both classes and so
//!      cancels in the t-test.
//!   2. **`CNTVCT_EL0`** — the architectural virtual counter, readable from
//!      userspace with a single instruction. Very low overhead but coarse: it
//!      ticks at a fixed 19.2 MHz (Pi 3/4) or 54 MHz (Pi 5) regardless of how
//!      fast the core is running, so it cannot resolve a leak smaller than
//!      roughly a hundred core cycles.
//!   3. **`CLOCK_MONOTONIC`** — always available, nanosecond units.

use std::fs;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Pmu,
    Cntvct,
    Monotonic,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Pmu => "pmu-cycles",
            Source::Cntvct => "cntvct",
            Source::Monotonic => "clock-monotonic",
        }
    }
    /// What one unit of this counter means, for reporting the measurement floor.
    pub fn unit(self) -> &'static str {
        match self {
            Source::Pmu => "core cycles",
            Source::Cntvct => "cntvct ticks",
            Source::Monotonic => "nanoseconds",
        }
    }
}

pub struct Counter {
    source: Source,
    fd: i32,
}

// perf_event_attr, as the kernel expects it. The struct is versioned by its
// `size` field, so a zeroed struct of this length with `size` set is accepted.
#[repr(C)]
#[derive(Default)]
struct PerfEventAttr {
    type_: u32,
    size: u32,
    config: u64,
    sample_period: u64,
    sample_type: u64,
    read_format: u64,
    flags: u64,
    wakeup_events: u32,
    bp_type: u32,
    config1: u64,
    config2: u64,
    branch_sample_type: u64,
    sample_regs_user: u64,
    sample_stack_user: u32,
    clockid: i32,
    sample_regs_intr: u64,
    aux_watermark: u32,
    sample_max_stack: u16,
    reserved_2: u16,
    aux_sample_size: u32,
    reserved_3: u32,
}

const PERF_TYPE_HARDWARE: u32 = 0;
const PERF_COUNT_HW_CPU_CYCLES: u64 = 0;
const EXCLUDE_KERNEL: u64 = 1 << 5;
const EXCLUDE_HV: u64 = 1 << 6;

impl Counter {
    pub fn open() -> Self {
        if let Some(fd) = Self::open_pmu() {
            return Counter { source: Source::Pmu, fd };
        }
        eprintln!(
            "note: PMU cycle counter unavailable (perf_event_paranoid too high, or no \
             permission). Falling back to a coarser counter — see the recorded \
             `counter` field before interpreting small effects.\n      \
             To enable: sudo sysctl kernel.perf_event_paranoid=1"
        );
        if Self::cntvct_works() {
            Counter { source: Source::Cntvct, fd: -1 }
        } else {
            Counter { source: Source::Monotonic, fd: -1 }
        }
    }

    fn open_pmu() -> Option<i32> {
        let mut attr = PerfEventAttr::default();
        attr.type_ = PERF_TYPE_HARDWARE;
        attr.size = std::mem::size_of::<PerfEventAttr>() as u32;
        attr.config = PERF_COUNT_HW_CPU_CYCLES;
        attr.flags = EXCLUDE_KERNEL | EXCLUDE_HV;
        // pid = 0 (this thread), cpu = -1 (whichever it runs on), no group.
        let fd = unsafe {
            libc::syscall(libc::SYS_perf_event_open, &attr as *const _, 0, -1, -1, 0) as i32
        };
        if fd < 0 {
            None
        } else {
            Some(fd)
        }
    }

    fn cntvct_works() -> bool {
        cfg!(target_arch = "aarch64")
    }

    pub fn source(&self) -> Source {
        self.source
    }

    /// Frequency of the counter, where it is known — used to report the floor.
    pub fn hz(&self) -> Option<u64> {
        match self.source {
            Source::Cntvct => Some(read_cntfrq()),
            Source::Monotonic => Some(1_000_000_000),
            Source::Pmu => None, // core cycles; the unit *is* the cycle
        }
    }

    #[inline(always)]
    pub fn read(&self) -> u64 {
        match self.source {
            Source::Pmu => {
                let mut v: u64 = 0;
                let n = unsafe {
                    libc::read(self.fd, &mut v as *mut u64 as *mut libc::c_void, 8)
                };
                if n == 8 {
                    v
                } else {
                    0
                }
            }
            Source::Cntvct => read_cntvct(),
            Source::Monotonic => {
                let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
                unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
                ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
            }
        }
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        if self.fd >= 0 {
            unsafe { libc::close(self.fd) };
        }
    }
}

#[cfg(target_arch = "aarch64")]
#[inline(always)]
fn read_cntvct() -> u64 {
    let v: u64;
    unsafe { std::arch::asm!("mrs {}, cntvct_el0", out(reg) v, options(nomem, nostack)) };
    v
}
#[cfg(not(target_arch = "aarch64"))]
#[inline(always)]
fn read_cntvct() -> u64 {
    0
}

#[cfg(target_arch = "aarch64")]
fn read_cntfrq() -> u64 {
    let v: u64;
    unsafe { std::arch::asm!("mrs {}, cntfrq_el0", out(reg) v, options(nomem, nostack)) };
    v
}
#[cfg(not(target_arch = "aarch64"))]
fn read_cntfrq() -> u64 {
    0
}

/// Pin this thread to one core. An unpinned measurement on a big.LITTLE-free
/// but still multi-core SoC migrates between cores mid-run, which shows up as
/// spurious variance.
pub fn pin_to_cpu(cpu: usize) -> bool {
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_ZERO(&mut set);
        libc::CPU_SET(cpu, &mut set);
        libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set) == 0
    }
}

/// Model name and current governor, recorded alongside the results so a capture
/// is self-describing.
pub fn platform() -> (String, String) {
    let model = fs::read_to_string("/proc/device-tree/model")
        .or_else(|_| fs::read_to_string("/proc/cpuinfo"))
        .unwrap_or_default()
        .lines()
        .find(|l| l.contains("Raspberry") || l.starts_with("model name"))
        .unwrap_or("unknown")
        .trim()
        .trim_end_matches('\0')
        .to_string();
    let gov = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
        .unwrap_or_else(|_| "unknown".into())
        .trim()
        .to_string();
    (model, gov)
}
