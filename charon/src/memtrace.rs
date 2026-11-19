//! Opt-in memory tracing (`CHARON_MEMTRACE=<MiB threshold>`): reports, on stderr, every
//! translated item or transformed item/pass during which the process resident set grew by at
//! least the threshold, with the elapsed time. Used to find the dominant allocation when a run
//! exceeds its memory cap (no heap profiler needed).
use std::sync::LazyLock;
use std::time::Instant;

static THRESHOLD_MIB: LazyLock<Option<u64>> = LazyLock::new(|| {
    std::env::var("CHARON_MEMTRACE")
        .ok()
        .and_then(|s| s.parse().ok())
});

/// Resident set size in MiB (Linux `/proc/self/statm`; 0 elsewhere).
pub fn rss_mib() -> u64 {
    let Ok(statm) = std::fs::read_to_string("/proc/self/statm") else {
        return 0;
    };
    let pages: u64 = statm
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    pages * 4096 / (1 << 20)
}

pub struct Probe {
    start_rss: u64,
    start: Instant,
}

pub fn enabled() -> bool {
    THRESHOLD_MIB.is_some()
}

pub fn start() -> Option<Probe> {
    enabled().then(|| Probe {
        start_rss: rss_mib(),
        start: Instant::now(),
    })
}

/// Compact `a::b::{impl}::c` rendering of an item name for trace lines.
pub fn short_name(name: &crate::ast::Name) -> String {
    name.name
        .iter()
        .map(|e| match e {
            crate::ast::PathElem::Ident(s, _) => s.to_string(),
            crate::ast::PathElem::Impl(_) => "{impl}".to_string(),
            _ => "_".to_string(),
        })
        .collect::<Vec<_>>()
        .join("::")
}

impl Probe {
    /// Report if the RSS grew by at least the threshold or the scope took over 10 s.
    pub fn finish(self, label: impl FnOnce() -> String) {
        let threshold = THRESHOLD_MIB.unwrap_or(u64::MAX);
        let rss = rss_mib();
        let grew = rss.saturating_sub(self.start_rss);
        let secs = self.start.elapsed().as_secs_f64();
        if grew >= threshold || secs >= 10.0 {
            eprintln!(
                "memtrace rss={rss}MiB grew={grew}MiB secs={secs:.1} {}",
                label()
            );
        }
    }
}
