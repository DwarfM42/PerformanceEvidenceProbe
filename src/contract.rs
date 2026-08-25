//! Pure contract primitives used by the Windows runtime and deterministic tests.

use std::collections::HashSet;

/// Bounded bookkeeping for retained live-process handles. A refusal is
/// deliberate degradation; it never manufactures missing terminal counters.
#[derive(Debug)]
pub struct HandleRetention {
    limit: usize,
    retained: HashSet<u64>,
    maximum_retained_count: usize,
    degraded: bool,
}

impl HandleRetention {
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            retained: HashSet::new(),
            maximum_retained_count: 0,
            degraded: false,
        }
    }

    pub fn try_retain(&mut self, process_local_id: u64) -> bool {
        if self.retained.contains(&process_local_id) {
            return true;
        }
        if self.retained.len() >= self.limit {
            self.degraded = true;
            return false;
        }
        self.retained.insert(process_local_id);
        self.maximum_retained_count = self.maximum_retained_count.max(self.retained.len());
        true
    }

    pub fn release(&mut self, process_local_id: u64) -> bool {
        self.retained.remove(&process_local_id)
    }

    pub fn retained_count(&self) -> usize {
        self.retained.len()
    }
    pub fn maximum_retained_count(&self) -> usize {
        self.maximum_retained_count
    }
    pub fn degraded(&self) -> bool {
        self.degraded
    }
}

/// Job configuration must be explicitly non-destructive. Limit flags remain
/// zero in M1; the Job exists only for containment observation/accounting.
#[derive(Debug, Clone, Copy)]
pub struct JobSafetyPolicy {
    limit_flags: u32,
}

impl JobSafetyPolicy {
    pub const fn probe_default() -> Self {
        Self { limit_flags: 0 }
    }
    pub const fn limit_flags(self) -> u32 {
        self.limit_flags
    }
    pub const fn kill_on_job_close_enabled(self) -> bool {
        false
    }
}

/// Computes `start + n * interval`; callers must wait to this deadline rather
/// than sleeping relative to their previous sampling completion.
pub const fn absolute_deadline_ns(start_ns: u64, interval_ns: u64, sample_index: u64) -> u64 {
    start_ns.saturating_add(interval_ns.saturating_mul(sample_index))
}
