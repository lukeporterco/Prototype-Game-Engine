use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use tracing::warn;

static METRICS_LOCK_POISON_WARNED: AtomicBool = AtomicBool::new(false);

fn warn_metrics_lock_poison_once(operation: &'static str) {
    if METRICS_LOCK_POISON_WARNED
        .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
    {
        warn!(operation, "metrics lock poisoned; recovered inner value");
    }
}

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
        match self.snapshot.read() {
            Ok(guard) => *guard,
            Err(poisoned) => {
                warn_metrics_lock_poison_once("read");
                *poisoned.into_inner()
            }
        }
    }

    pub(crate) fn publish(&self, snapshot: LoopMetricsSnapshot) {
        match self.snapshot.write() {
            Ok(mut guard) => *guard = snapshot,
            Err(poisoned) => {
                warn_metrics_lock_poison_once("write");
                let mut guard = poisoned.into_inner();
                *guard = snapshot;
            }
        }
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
    use std::sync::RwLock;
    use std::thread;

    use super::*;

    fn poison_lock(lock: &RwLock<LoopMetricsSnapshot>) {
        thread::scope(|scope| {
            let _ = scope
                .spawn(|| {
                    let _guard = lock.write().expect("write guard");
                    panic!("poison metrics lock");
                })
                .join();
        });
    }

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

        assert!((snapshot.fps - 2.0).abs() < 0.05);
        assert!((snapshot.tps - 4.0).abs() < 0.05);
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

    #[test]
    fn snapshot_recovers_after_poison_without_panic() {
        let handle = MetricsHandle::default();
        poison_lock(handle.snapshot.as_ref());

        let snapshot = handle.snapshot();
        assert_eq!(snapshot.fps, 0.0);
        assert_eq!(snapshot.tps, 0.0);
        assert_eq!(snapshot.frame_time_ms, 0.0);
    }

    #[test]
    fn publish_recovers_after_poison_without_panic() {
        let handle = MetricsHandle::default();
        poison_lock(handle.snapshot.as_ref());

        let expected = LoopMetricsSnapshot {
            fps: 15.0,
            tps: 60.0,
            frame_time_ms: 11.0,
        };
        handle.publish(expected);

        let actual = handle.snapshot();
        assert_eq!(actual.fps, expected.fps);
        assert_eq!(actual.tps, expected.tps);
        assert_eq!(actual.frame_time_ms, expected.frame_time_ms);
    }
}
