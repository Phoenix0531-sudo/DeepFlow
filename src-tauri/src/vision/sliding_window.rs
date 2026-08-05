/// 帧级防抖约 0.5s，秒级滑动窗 60s，离开延迟 2s。
const DEFAULT_FPS: usize = 10;
const DEFAULT_DEBOUNCE_FRAMES: usize = 5; // ~0.5s @10fps
const DEFAULT_WINDOW_SECS: usize = 60;
const LEAVE_LATCH_FRAMES: usize = 20; // ~2s @10fps

pub struct SlidingWindowFilter {
    fps: usize,
    debounce_threshold_count: usize,
    sliding_capacity_secs: usize,
    leave_latch_frames: usize,
    frame_buffer: Vec<bool>,
    consecutive_positive: usize,
    consecutive_negative: usize,
    /// 当前是否处于「有效持握」锁存（离开需连续负帧才解除）。
    latched: bool,
    second_buffer: Vec<bool>,
}

impl SlidingWindowFilter {
    pub fn new() -> Self {
        Self::with_params(DEFAULT_FPS, DEFAULT_DEBOUNCE_FRAMES, DEFAULT_WINDOW_SECS)
    }

    pub fn with_params(fps: usize, debounce_frames: usize, window_secs: usize) -> Self {
        Self {
            fps: fps.max(1),
            debounce_threshold_count: debounce_frames.max(1),
            sliding_capacity_secs: window_secs.max(1),
            leave_latch_frames: LEAVE_LATCH_FRAMES,
            frame_buffer: Vec::with_capacity(fps.max(1)),
            consecutive_positive: 0,
            consecutive_negative: 0,
            latched: false,
            second_buffer: Vec::with_capacity(window_secs.max(1)),
        }
    }

    pub fn reset(&mut self) {
        self.frame_buffer.clear();
        self.second_buffer.clear();
        self.consecutive_positive = 0;
        self.consecutive_negative = 0;
        self.latched = false;
    }

    /// 推入一帧检测结果。每满约 1 秒返回一次 hold_secs（60s 窗内违规秒数）。
    pub fn push_frame_result(&mut self, positive: bool) -> Option<u32> {
        if positive {
            self.consecutive_positive = self.consecutive_positive.saturating_add(1);
            self.consecutive_negative = 0;
            if self.consecutive_positive >= self.debounce_threshold_count {
                self.latched = true;
            }
        } else {
            self.consecutive_positive = 0;
            self.consecutive_negative = self.consecutive_negative.saturating_add(1);
            if self.consecutive_negative >= self.leave_latch_frames {
                self.latched = false;
            }
        }

        let frame_pos = self.latched;
        self.frame_buffer.push(frame_pos);

        if self.frame_buffer.len() >= self.fps {
            let threshold = self.fps / 2 + 1;
            let second_has = self.frame_buffer.iter().filter(|&&x| x).count() >= threshold;
            self.frame_buffer.clear();
            self.second_buffer.push(second_has);
            if self.second_buffer.len() > self.sliding_capacity_secs {
                self.second_buffer.remove(0);
            }
            let total = self.second_buffer.iter().filter(|&&x| x).count() as u32;
            return Some(total);
        }
        None
    }

    pub fn current_hold_secs(&self) -> u32 {
        self.second_buffer.iter().filter(|&&x| x).count() as u32
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
    fn accumulates_hold_after_debounce() {
        let mut f = SlidingWindowFilter::with_params(5, 2, 10);
        // 2 帧正 → latch
        assert!(f.push_frame_result(true).is_none());
        let mut last = None;
        for _ in 0..20 {
            last = f.push_frame_result(true);
        }
        assert!(last.unwrap_or(0) >= 1);
    }
}
