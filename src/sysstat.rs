//! CPU and memory for the built-in `cpu` and `mem` bar modules, read from
//! `/proc` on the module's own tick. Parsing is pure so it is tested without a
//! machine to read.

use std::path::Path;

/// Jiffies from the aggregate `cpu` line of `/proc/stat`. Usage is the change
/// between two samples, so one sample alone says nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuSample {
    idle: u64,
    total: u64,
}

/// `/proc/meminfo`'s total and available memory, in KiB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mem {
    pub total_kib: u64,
    pub available_kib: u64,
}

/// At or above this percentage the module turns urgent.
pub const URGENT_PERCENT: u8 = 90;

pub fn read_cpu() -> Option<CpuSample> {
    parse_cpu(&std::fs::read_to_string(Path::new("/proc/stat")).ok()?)
}

pub fn read_mem() -> Option<Mem> {
    parse_mem(&std::fs::read_to_string(Path::new("/proc/meminfo")).ok()?)
}

/// The first line: `cpu  user nice system idle iowait irq softirq steal guest
/// guest_nice`. Guest time is already counted in user and nice, so only the
/// first eight fields make the total; waiting on I/O counts as idle.
pub fn parse_cpu(stat: &str) -> Option<CpuSample> {
    let line = stat.lines().next()?;
    let mut fields = line.split_whitespace();
    if fields.next()? != "cpu" {
        return None;
    }
    let v: Vec<u64> = fields
        .take(8)
        .map(|f| f.parse().ok())
        .collect::<Option<_>>()?;
    if v.len() < 5 {
        return None;
    }
    Some(CpuSample {
        idle: v[3] + v[4],
        total: v.iter().sum(),
    })
}

/// Busy percentage between two samples, or `None` if no time passed.
pub fn cpu_percent(prev: CpuSample, now: CpuSample) -> Option<u8> {
    let total = now.total.checked_sub(prev.total)?;
    let idle = now.idle.checked_sub(prev.idle)?;
    if total == 0 {
        return None;
    }
    let busy = total.saturating_sub(idle);
    Some(((busy * 100 + total / 2) / total).min(100) as u8)
}

pub fn parse_mem(meminfo: &str) -> Option<Mem> {
    let field = |name: &str| {
        meminfo
            .lines()
            .find_map(|l| l.strip_prefix(name)?.strip_prefix(':'))
            .and_then(|rest| rest.split_whitespace().next()?.parse::<u64>().ok())
    };
    Some(Mem {
        total_kib: field("MemTotal")?,
        available_kib: field("MemAvailable")?,
    })
}

impl Mem {
    pub fn used_kib(&self) -> u64 {
        self.total_kib.saturating_sub(self.available_kib)
    }
    pub fn used_percent(&self) -> u8 {
        if self.total_kib == 0 {
            return 0;
        }
        (self.used_kib() * 100 / self.total_kib).min(100) as u8
    }
}

/// KiB as GiB with one decimal: `24.1G`.
pub fn gib(kib: u64) -> String {
    format!("{:.1}G", kib as f64 / (1024.0 * 1024.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_usage_is_the_change_between_samples() {
        let a = parse_cpu("cpu  100 0 100 800 0 0 0 0 0 0\ncpu0 1 2 3 4\n").unwrap();
        // 200 more jiffies, 50 of them idle (40 idle + 10 iowait): 75% busy.
        let b = parse_cpu("cpu  200 0 150 840 10 0 0 0 7 0\n").unwrap();
        assert_eq!(cpu_percent(a, b), Some(75));
        assert_eq!(cpu_percent(b, b), None, "no time passed");
        assert_eq!(cpu_percent(b, a), None, "went backwards");
        assert!(parse_cpu("intr 1 2 3").is_none());
        assert!(parse_cpu("cpu 1 2 x 4 5").is_none());
    }

    #[test]
    fn memory_used_is_total_minus_available() {
        let m = parse_mem(
            "MemTotal:       65536000 kB\nMemFree:  100 kB\nMemAvailable:   49152000 kB\n",
        )
        .unwrap();
        assert_eq!(m.used_kib(), 16_384_000);
        assert_eq!(m.used_percent(), 25);
        assert_eq!(gib(m.used_kib()), "15.6G");
        // MemAvailableX must not pass for MemAvailable.
        assert!(parse_mem("MemTotal: 10 kB\nMemAvailableX: 5 kB\n").is_none());
    }
}
