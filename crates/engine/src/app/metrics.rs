use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, Default)]
pub struct LoopMetricsSnapshot {
    pub fps: f32,
    pub tps: f32,
    pub frame_time_ms: f32,
}

#[derive(Clone, Debug)]
pub struct MetricsHandle {
    snapshot: Arc<RwLock<LoopMetricsSnapshot>>,
}

impl Default for MetricsHandle {
    fn default() -> Self {
        Self {
            snapshot: Arc::new(RwLock::new(LoopMetricsSnapshot::default())),
        }
    }
}

impl MetricsHandle {
    pub fn snapshot(&self) -> LoopMetricsSnapshot {
        *self
            .snapshot
            .read()
            .expect("metrics snapshot lock poisoned")
    }

    pub(crate) fn publish(&self, snapshot: LoopMetricsSnapshot) {
        *self
            .snapshot
            .write()
            .expect("metrics snapshot lock poisoned") = snapshot;
    }
}

#[derive(Debug)]
pub(crate) struct MetricsAccumulator {
    interval_start: Instant,
    interval: Duration,
    frames: u32,
    ticks: u32,
    frame_time_sum: Duration,
}

impl MetricsAccumulator {
    pub(crate) fn new(interval: Duration) -> Self {
        Self {
            interval_start: Instant::now(),
            interval,
            frames: 0,
            ticks: 0,
            frame_time_sum: Duration::ZERO,
        }
    }

    pub(crate) fn record_frame(&mut self, frame_dt: Duration) {
        self.frames = self.frames.saturating_add(1);
        self.frame_time_sum = self.frame_time_sum.saturating_add(frame_dt);
    }

    pub(crate) fn record_tick(&mut self) {
        self.ticks = self.ticks.saturating_add(1);
    }

    pub(crate) fn maybe_snapshot(&mut self, now: Instant) -> Option<LoopMetricsSnapshot> {
        let elapsed = now.saturating_duration_since(self.interval_start);
        if elapsed < self.interval {
            return None;
        }

        let elapsed_seconds = elapsed.as_secs_f32().max(f32::EPSILON);
        let frame_time_ms = if self.frames == 0 {
            0.0
        } else {
            (self.frame_time_sum.as_secs_f32() / self.frames as f32) * 1000.0
        };

        let snapshot = LoopMetricsSnapshot {
            fps: self.frames as f32 / elapsed_seconds,
            tps: self.ticks as f32 / elapsed_seconds,
            frame_time_ms,
        };

        self.interval_start = now;
        self.frames = 0;
        self.ticks = 0;
        self.frame_time_sum = Duration::ZERO;

        Some(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_computes_expected_values() {
        let mut accumulator = MetricsAccumulator::new(Duration::from_secs(1));
        let base = Instant::now();

        accumulator.record_frame(Duration::from_millis(16));
        accumulator.record_frame(Duration::from_millis(16));
        accumulator.record_tick();
        accumulator.record_tick();
        accumulator.record_tick();
        accumulator.record_tick();

        let snapshot = accumulator
            .maybe_snapshot(base + Duration::from_secs(1))
            .expect("snapshot should be emitted");

        assert!((snapshot.fps - 2.0).abs() < f32::EPSILON);
        assert!((snapshot.tps - 4.0).abs() < f32::EPSILON);
        assert!((snapshot.frame_time_ms - 16.0).abs() < 0.001);
    }

    #[test]
    fn snapshot_not_emitted_before_interval() {
        let mut accumulator = MetricsAccumulator::new(Duration::from_secs(1));
        let base = Instant::now();
        accumulator.record_frame(Duration::from_millis(16));

        assert!(accumulator
            .maybe_snapshot(base + Duration::from_millis(500))
            .is_none());
    }
}
