use std::time::Duration;

pub(crate) const PERF_WINDOW_LEN: usize = 120;
const PERF_STATS_ENABLED_BY_DEFAULT: bool = true;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct RollingMsStats {
    pub last_ms: f32,
    pub avg_ms: f32,
    pub max_ms: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct PerfStatsSnapshot {
    pub sim: RollingMsStats,
    pub ren: RollingMsStats,
}

#[derive(Debug, Default)]
pub(crate) struct PerfStats {
    sim: RollingWindowMs,
    ren: RollingWindowMs,
}

impl PerfStats {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn enabled_by_default() -> bool {
        PERF_STATS_ENABLED_BY_DEFAULT
    }

    pub(crate) fn window_len() -> usize {
        PERF_WINDOW_LEN
    }

    pub(crate) fn record_frame(&mut self, sim_duration: Duration, render_duration: Duration) {
        self.sim.push_ms(duration_to_ms(sim_duration));
        self.ren.push_ms(duration_to_ms(render_duration));
    }

    pub(crate) fn snapshot(&self) -> PerfStatsSnapshot {
        PerfStatsSnapshot {
            sim: self.sim.snapshot(),
            ren: self.ren.snapshot(),
        }
    }
}

#[derive(Debug)]
struct RollingWindowMs {
    samples_ms: [f32; PERF_WINDOW_LEN],
    head: usize,
    count: usize,
    sum_ms: f32,
    last_ms: f32,
}

impl Default for RollingWindowMs {
    fn default() -> Self {
        Self {
            samples_ms: [0.0; PERF_WINDOW_LEN],
            head: 0,
            count: 0,
            sum_ms: 0.0,
            last_ms: 0.0,
        }
    }
}

impl RollingWindowMs {
    fn push_ms(&mut self, value_ms: f32) {
        self.last_ms = value_ms;

        if self.count < PERF_WINDOW_LEN {
            self.samples_ms[self.head] = value_ms;
            self.head = (self.head + 1) % PERF_WINDOW_LEN;
            self.count += 1;
            self.sum_ms += value_ms;
            return;
        }

        let evicted = self.samples_ms[self.head];
        self.samples_ms[self.head] = value_ms;
        self.head = (self.head + 1) % PERF_WINDOW_LEN;
        self.sum_ms += value_ms - evicted;
    }

    fn snapshot(&self) -> RollingMsStats {
        if self.count == 0 {
            return RollingMsStats::default();
        }

        let mut max_ms = self.samples_ms[0];
        for index in 1..self.count {
            let candidate = self.samples_ms[index];
            if candidate > max_ms {
                max_ms = candidate;
            }
        }

        RollingMsStats {
            last_ms: self.last_ms,
            avg_ms: self.sum_ms / self.count as f32,
            max_ms,
        }
    }
}

fn duration_to_ms(duration: Duration) -> f32 {
    duration.as_secs_f32() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_stats_snapshot_is_zeroed() {
        let stats = PerfStats::new();
        let snapshot = stats.snapshot();

        assert_eq!(snapshot.sim, RollingMsStats::default());
        assert_eq!(snapshot.ren, RollingMsStats::default());
    }

    #[test]
    fn partial_window_average_uses_current_sample_count() {
        let mut window = RollingWindowMs::default();
        window.push_ms(1.0);
        window.push_ms(2.0);
        window.push_ms(3.0);
        let snapshot = window.snapshot();

        assert_eq!(snapshot.last_ms, 3.0);
        assert!((snapshot.avg_ms - 2.0).abs() < 0.0001);
        assert_eq!(snapshot.max_ms, 3.0);
    }

    #[test]
    fn full_window_average_and_max_are_correct() {
        let mut window = RollingWindowMs::default();
        for value in 1..=PERF_WINDOW_LEN {
            window.push_ms(value as f32);
        }
        let snapshot = window.snapshot();

        assert_eq!(snapshot.last_ms, PERF_WINDOW_LEN as f32);
        assert!((snapshot.avg_ms - ((PERF_WINDOW_LEN as f32 + 1.0) / 2.0)).abs() < 0.001);
        assert_eq!(snapshot.max_ms, PERF_WINDOW_LEN as f32);
    }

    #[test]
    fn wraparound_eviction_updates_average() {
        let mut window = RollingWindowMs::default();
        for _ in 0..PERF_WINDOW_LEN {
            window.push_ms(10.0);
        }
        window.push_ms(20.0);
        let snapshot = window.snapshot();

        let expected_avg = ((PERF_WINDOW_LEN as f32 - 1.0) * 10.0 + 20.0) / PERF_WINDOW_LEN as f32;
        assert_eq!(snapshot.last_ms, 20.0);
        assert!((snapshot.avg_ms - expected_avg).abs() < 0.001);
        assert_eq!(snapshot.max_ms, 20.0);
    }

    #[test]
    fn max_recomputes_when_prior_max_is_evicted() {
        let mut window = RollingWindowMs::default();
        window.push_ms(100.0);
        for _ in 1..PERF_WINDOW_LEN {
            window.push_ms(10.0);
        }
        window.push_ms(20.0);
        let snapshot = window.snapshot();

        assert_eq!(snapshot.last_ms, 20.0);
        assert_eq!(snapshot.max_ms, 20.0);
    }

    #[test]
    fn duration_to_ms_conversion_is_expected() {
        let value = duration_to_ms(Duration::from_micros(1_500));
        assert!((value - 1.5).abs() < 0.0001);
    }
}
