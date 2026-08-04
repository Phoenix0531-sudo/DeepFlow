const DEFAULT_FPS: usize = 15;
const DEFAULT_DEBOUNCE_SECS: usize = 10;
const DEFAULT_WINDOW_SECS: usize = 60;

pub struct SlidingWindowFilter {
    fps: usize,
    debounce_threshold_count: usize,
    sliding_capacity_secs: usize,
    frame_buffer: Vec<bool>,
    consecutive_positive: usize,
    second_buffer: Vec<bool>,
    leave_latch: u32,
}

impl SlidingWindowFilter {
    pub fn new() -> Self {
        Self::with_params(DEFAULT_FPS, DEFAULT_DEBOUNCE_SECS, DEFAULT_WINDOW_SECS)
    }

    pub fn with_params(fps: usize, debounce_secs: usize, window_secs: usize) -> Self {
        Self {
            fps,
            debounce_threshold_count: fps.saturating_mul(debounce_secs),
            sliding_capacity_secs: window_secs,
            frame_buffer: Vec::with_capacity(fps),
            consecutive_positive: 0,
            second_buffer: Vec::with_capacity(window_secs),
            leave_latch: 0,
        }
    }

    /// 返回 Some(hold_secs)：每满一秒返回一次 60s 滑动累计违规秒。
    pub fn push_frame_result(&mut self, positive: bool) -> Option<u32> {
        if positive {
            self.consecutive_positive = self.consecutive_positive.saturating_add(1);
            self.leave_latch = 0;
        } else {
            self.consecutive_positive = 0;
            self.leave_latch = self.leave_latch.saturating_add(1);
        }

        // 离开后延迟 2s 再判为 false：leave_latch 计到 2*fps 才清 latch
        let debounced_positive = self.consecutive_positive >= self.debounce_threshold_count;
        self.frame_buffer
            .push(debounced_positive);

        if self.frame_buffer.len() >= self.fps {
            let threshold = self.fps / 2 + 1;
            let second_has = self.frame_buffer.iter().filter(|&&x| x).count() >= threshold;
            self.frame_buffer.clear();
            self.second_buffer.push(second_has);
            if self.second_buffer.len() > self.sliding_capacity_secs {
                self.second_buffer.remove(0);
            }
            let total = self
                .second_buffer
                .iter()
                .filter(|&&x| x)
                .count() as u32;
            return Some(total);
        }
        None
    }
}

impl Default for SlidingWindowFilter {
    fn default() -> Self {
        Self::new()
    }
}
