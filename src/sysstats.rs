use std::fs;

#[derive(Clone, Copy, Debug)]
struct CpuTimes {
    total: u64,
    idle: u64,
}

#[derive(Clone, Copy, Debug)]
pub struct SystemStatsSample {
    pub cpu_usage: f64, // 0.0..1.0
    pub ram_usage: f64, // 0.0..1.0
}

pub struct SystemStatsReader {
    prev_cpu: Option<CpuTimes>,
}

impl SystemStatsReader {
    pub fn new() -> Self {
        Self { prev_cpu: None }
    }

    pub fn sample(&mut self) -> Option<SystemStatsSample> {
        let curr_cpu = read_cpu_times()?;
        let cpu_usage = if let Some(prev) = self.prev_cpu {
            cpu_usage(prev, curr_cpu).unwrap_or(0.0)
        } else {
            0.0
        };
        self.prev_cpu = Some(curr_cpu);

        let ram_usage = read_ram_usage().unwrap_or(0.0);
        Some(SystemStatsSample {
            cpu_usage: cpu_usage.clamp(0.0, 1.0),
            ram_usage: ram_usage.clamp(0.0, 1.0),
        })
    }
}

fn cpu_usage(prev: CpuTimes, curr: CpuTimes) -> Option<f64> {
    let total_delta = curr.total.saturating_sub(prev.total);
    let idle_delta = curr.idle.saturating_sub(prev.idle);
    if total_delta == 0 {
        return None;
    }
    Some(1.0 - (idle_delta as f64 / total_delta as f64))
}

fn read_cpu_times() -> Option<CpuTimes> {
    let content = fs::read_to_string("/proc/stat").ok()?;
    let first = content.lines().next()?;
    let mut it = first.split_whitespace();
    let label = it.next()?;
    if label != "cpu" {
        return None;
    }
    let mut values = Vec::new();
    for s in it {
        if let Ok(v) = s.parse::<u64>() {
            values.push(v);
        } else {
            return None;
        }
    }
    if values.len() < 4 {
        return None;
    }
    let user = values[0];
    let nice = values[1];
    let system = values[2];
    let idle = values[3];
    let iowait = *values.get(4).unwrap_or(&0);
    let irq = *values.get(5).unwrap_or(&0);
    let softirq = *values.get(6).unwrap_or(&0);
    let steal = *values.get(7).unwrap_or(&0);
    let guest = *values.get(8).unwrap_or(&0);
    let guest_nice = *values.get(9).unwrap_or(&0);

    let total = user
        .saturating_add(nice)
        .saturating_add(system)
        .saturating_add(idle)
        .saturating_add(iowait)
        .saturating_add(irq)
        .saturating_add(softirq)
        .saturating_add(steal)
        .saturating_add(guest)
        .saturating_add(guest_nice);

    Some(CpuTimes {
        total,
        idle: idle.saturating_add(iowait),
    })
}

fn read_ram_usage() -> Option<f64> {
    let content = fs::read_to_string("/proc/meminfo").ok()?;
    let mut total_kb: Option<u64> = None;
    let mut avail_kb: Option<u64> = None;
    let mut free_kb: Option<u64> = None;
    let mut buffers_kb: Option<u64> = None;
    let mut cached_kb: Option<u64> = None;

    for line in content.lines() {
        if let Some(v) = parse_meminfo_kb(line, "MemTotal:") {
            total_kb = Some(v);
        } else if let Some(v) = parse_meminfo_kb(line, "MemAvailable:") {
            avail_kb = Some(v);
        } else if let Some(v) = parse_meminfo_kb(line, "MemFree:") {
            free_kb = Some(v);
        } else if let Some(v) = parse_meminfo_kb(line, "Buffers:") {
            buffers_kb = Some(v);
        } else if let Some(v) = parse_meminfo_kb(line, "Cached:") {
            cached_kb = Some(v);
        }
    }

    let total = total_kb?;
    if total == 0 {
        return None;
    }
    let available = if let Some(avail) = avail_kb {
        avail
    } else {
        free_kb
            .unwrap_or(0)
            .saturating_add(buffers_kb.unwrap_or(0))
            .saturating_add(cached_kb.unwrap_or(0))
    };
    let used = total.saturating_sub(available);
    Some(used as f64 / total as f64)
}

fn parse_meminfo_kb(line: &str, key: &str) -> Option<u64> {
    let rest = line.strip_prefix(key)?.trim();
    let value = rest.split_whitespace().next()?;
    value.parse::<u64>().ok()
}
