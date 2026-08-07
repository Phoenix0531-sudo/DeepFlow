/// 帧级防抖 + 真实时间累计持握秒数。
///
/// 旧实现按「假定 10fps 凑满 1 秒」计 hold，实际 process_every 降采样后
/// 有效 fps 更低，导致 5 秒真持握只累计到 1–2s，测试模式 L1=3 永远达不到。
/// 现改为：防抖仍用帧计数；一旦 latched，用墙钟累计 hold_secs。
use std::time::Instant;

const DEFAULT_DEBOUNCE_FRAMES: usize = 3; // ~0.4s @ ~7fps processed
const DEFAULT_LEAVE_LATCH_SECS: f32 = 1.5;
const DEFAULT_WINDOW_SECS: u32 = 60;

pub struct SlidingWindowFilter {
    debounce_threshold_count: usize,
    leave_latch_secs: f32,
    sliding_capacity_secs: u32,
    consecutive_positive: usize,
    /// 最近一次变为负帧的时刻（用于离开延迟）。
    negative_since: Option<Instant>,
    latched: bool,
    /// latched 后开始累计的时刻。
    hold_started: Option<Instant>,
    /// 本段已确认的 hold 秒（整数，单调不减直到 reset/离开）。
    /// 连续 hold 超过 sliding_capacity_secs 时封顶。
    last_emitted_hold: u32,
}

impl SlidingWindowFilter {
    pub fn new() -> Self {
        Self::with_params(DEFAULT_DEBOUNCE_FRAMES, DEFAULT_LEAVE_LATCH_SECS, DEFAULT_WINDOW_SECS)
    }

    pub fn with_params(
        debounce_frames: usize,
        leave_latch_secs: f32,
        window_secs: u32,
    ) -> Self {
        Self {
            debounce_threshold_count: debounce_frames.max(1),
            leave_latch_secs: leave_latch_secs.max(0.2),
            sliding_capacity_secs: window_secs.max(1),
            consecutive_positive: 0,
            negative_since: None,
            latched: false,
            hold_started: None,
            last_emitted_hold: 0,
        }
    }

    /// 测试模式：更快进入 / 更快放下恢复。
    pub fn for_test_mode() -> Self {
        Self::with_params(2, 0.6, 30)
    }

    pub fn reset(&mut self) {
        self.consecutive_positive = 0;
        self.negative_since = None;
        self.latched = false;
        self.hold_started = None;
        self.last_emitted_hold = 0;
    }

    /// 推入一帧检测结果。hold 秒变化时返回 Some(hold_secs)。
    pub fn push_frame_result(&mut self, positive: bool) -> Option<u32> {
        let now = Instant::now();

        if positive {
            self.consecutive_positive = self.consecutive_positive.saturating_add(1);
            self.negative_since = None;
            if !self.latched && self.consecutive_positive >= self.debounce_threshold_count {
                self.latched = true;
                self.hold_started = Some(now);
                self.last_emitted_hold = 0;
            }
        } else {
            self.consecutive_positive = 0;
            if self.latched {
                let since = self.negative_since.get_or_insert(now);
                if since.elapsed().as_secs_f32() >= self.leave_latch_secs {
                    self.latched = false;
                    self.hold_started = None;
                    self.negative_since = None;
                    self.last_emitted_hold = 0;
                    return Some(0);
                }
            } else {
                self.negative_since = None;
            }
        }

        if self.latched {
            if let Some(start) = self.hold_started {
                let hold = start
                    .elapsed()
                    .as_secs()
                    .min(self.sliding_capacity_secs as u64) as u32;
                // 至少在 latch 当帧报 1，避免 0→阈值 空窗过长
                let hold = hold.max(1);
                if hold != self.last_emitted_hold {
                    self.last_emitted_hold = hold;
                    return Some(hold);
                }
            }
        }

        None
    }

    pub fn current_hold_secs(&self) -> u32 {
        if !self.latched {
            return 0;
        }
        if let Some(start) = self.hold_started {
            return start
                .elapsed()
                .as_secs()
                .min(self.sliding_capacity_secs as u64)
                .max(1) as u32;
        }
        self.last_emitted_hold
    }
}

impl Default for SlidingWindowFilter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latches_and_reports_hold() {
        let mut f = SlidingWindowFilter::with_params(2, 0.5, 10);
        // 未防抖完成前不应 hold
        assert!(f.push_frame_result(true).is_none());
        // 第 2 正帧 latch → hold>=1
        let h = f.push_frame_result(true);
        assert_eq!(h, Some(1));
        assert!(f.current_hold_secs() >= 1);
    }

    #[test]
    fn release_resets_hold() {
        // with_params 会把 leave_latch 下限钳到 0.2s
        let mut f = SlidingWindowFilter::with_params(1, 0.2, 10);
        assert_eq!(f.push_frame_result(true), Some(1));
        // 刚转负：尚未满 leave_latch，不应释放
        assert_eq!(f.push_frame_result(false), None);
        assert!(f.current_hold_secs() >= 1);
        // 等待超过 leave_latch 后再推负帧 → 释放
        std::thread::sleep(std::time::Duration::from_millis(250));
        assert_eq!(f.push_frame_result(false), Some(0));
        assert_eq!(f.current_hold_secs(), 0);
    }

    #[test]
    fn wall_clock_accumulates_independent_of_fps() {
        // 旧帧计数实现：2s 内若只推 2 帧，hold 只有 0–1。
        // 墙钟实现：哪怕帧极少，as_secs 也应跟真实时间走。
        // 注意 Instant::as_secs 向下取整，需 >2s 才能从 1 跳到 2。
        let mut f = SlidingWindowFilter::with_params(1, 0.3, 60);
        assert_eq!(f.push_frame_result(true), Some(1));
        std::thread::sleep(std::time::Duration::from_millis(2100));
        // 仅再推 1 帧（模拟极低有效 fps）
        let h = f.push_frame_result(true);
        assert!(h.is_some(), "should emit hold change after >2s");
        let hold = h.unwrap();
        assert!(
            hold >= 2,
            "wall-clock hold should be >=2 after 2.1s, got {hold}"
        );
        assert!(f.current_hold_secs() >= 2);
    }
}
