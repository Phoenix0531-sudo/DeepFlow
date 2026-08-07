use super::camera_stream::CameraController;
use super::detector::{is_operating_phone, Detector, Detection, HybridDetector};
use super::sliding_window::SlidingWindowFilter;
use image::{ImageBuffer, ImageEncoder, Rgb};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoiRect {
    /// 归一化 0..1
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Default for RoiRect {
    fn default() -> Self {
        // 全帧
        Self {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        }
    }
}

impl RoiRect {
    pub fn from_json(s: &str) -> Self {
        serde_json::from_str(s).unwrap_or_default()
    }

    pub fn crop_rgb(&self, width: u32, height: u32, rgb: &[u8]) -> (u32, u32, Vec<u8>) {
        if self.w <= 0.0 || self.h <= 0.0 || width == 0 || height == 0 {
            return (width, height, rgb.to_vec());
        }
        let x0 = ((self.x.clamp(0.0, 1.0)) * width as f32) as u32;
        let y0 = ((self.y.clamp(0.0, 1.0)) * height as f32) as u32;
        let mut cw = ((self.w.clamp(0.0, 1.0)) * width as f32) as u32;
        let mut ch = ((self.h.clamp(0.0, 1.0)) * height as f32) as u32;
        if x0 >= width || y0 >= height {
            return (width, height, rgb.to_vec());
        }
        cw = cw.min(width - x0).max(1);
        ch = ch.min(height - y0).max(1);
        let mut out = Vec::with_capacity((cw * ch * 3) as usize);
        for y in y0..y0 + ch {
            let row = (y * width + x0) as usize * 3;
            let end = row + cw as usize * 3;
            if end <= rgb.len() {
                out.extend_from_slice(&rgb[row..end]);
            }
        }
        (cw, ch, out)
    }
}

#[derive(Debug, Clone)]
pub enum VisionEvent {
    HoldSecs(u32),
    CameraBlocked,
    DetectionDebug(Detection),
}

/// 视觉管线：摄像头 → ROI → 检测 → 滑动窗 → 事件。
pub struct VisionPipeline {
    camera: Arc<CameraController>,
    detector: Arc<Mutex<HybridDetector>>,
    filter: Arc<Mutex<SlidingWindowFilter>>,
    roi: Arc<Mutex<RoiRect>>,
    enabled: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    models_dir: PathBuf,
    event_tx: Mutex<Option<UnboundedSender<VisionEvent>>>,
    last_ok_frame: Arc<Mutex<std::time::Instant>>,
    consecutive_frame_fail: Arc<Mutex<u32>>,
    debug: Arc<AtomicBool>,
    /// 最近一次 hold 秒数（滑动窗）。
    last_hold_secs: Arc<Mutex<u32>>,
    /// 缩略 JPEG 预览（setup / UI 轮询）。
    last_preview_jpeg: Arc<Mutex<Option<Vec<u8>>>>,
    last_detection: Arc<Mutex<Option<Detection>>>,
    /// 当前已启动的摄像头设备标识。
    active_device: Arc<Mutex<Option<String>>>,
}

impl VisionPipeline {
    pub fn new(models_dir: PathBuf, prefer_cpu: bool) -> Self {
        let detector = HybridDetector::create(&models_dir, prefer_cpu);
        info!(target: "deepflow", "vision detector kind={}", detector.kind());
        Self {
            camera: Arc::new(CameraController::new()),
            detector: Arc::new(Mutex::new(detector)),
            filter: Arc::new(Mutex::new(SlidingWindowFilter::new())),
            roi: Arc::new(Mutex::new(RoiRect::default())),
            enabled: Arc::new(AtomicBool::new(true)),
            running: Arc::new(AtomicBool::new(false)),
            models_dir,
            event_tx: Mutex::new(None),
            last_ok_frame: Arc::new(Mutex::new(std::time::Instant::now())),
            consecutive_frame_fail: Arc::new(Mutex::new(0)),
            debug: Arc::new(AtomicBool::new(false)),
            last_hold_secs: Arc::new(Mutex::new(0)),
            last_preview_jpeg: Arc::new(Mutex::new(None)),
            last_detection: Arc::new(Mutex::new(None)),
            active_device: Arc::new(Mutex::new(None)),
        }
    }

    pub fn active_device(&self) -> Option<String> {
        self.active_device.lock().clone()
    }

    pub fn detector_kind(&self) -> String {
        self.detector.lock().kind().to_string()
    }

    pub fn last_hold_secs(&self) -> u32 {
        *self.last_hold_secs.lock()
    }

    pub fn last_detection(&self) -> Option<Detection> {
        self.last_detection.lock().clone()
    }

    pub fn preview_jpeg(&self) -> Option<Vec<u8>> {
        self.last_preview_jpeg.lock().clone()
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    pub fn set_event_sender(&self, tx: UnboundedSender<VisionEvent>) {
        *self.event_tx.lock() = Some(tx);
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::SeqCst);
    }

    pub fn set_debug(&self, on: bool) {
        self.debug.store(on, Ordering::SeqCst);
    }

    /// 测试模式：更快防抖 / 更快放下恢复。
    pub fn set_test_mode(&self, on: bool) {
        let mut f = self.filter.lock();
        *f = if on {
            SlidingWindowFilter::for_test_mode()
        } else {
            SlidingWindowFilter::new()
        };
        *self.last_hold_secs.lock() = 0;
    }

    pub fn set_roi_json(&self, json: &str) {
        *self.roi.lock() = RoiRect::from_json(json);
    }

    pub fn reload_detector(&self, prefer_cpu: bool) {
        let d = HybridDetector::create(&self.models_dir, prefer_cpu);
        info!(target: "deepflow", "reloaded detector kind={}", d.kind());
        *self.detector.lock() = d;
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn start(&self, device: &str) -> Result<(), String> {
        if !self.enabled.load(Ordering::SeqCst) {
            info!(target: "deepflow", "vision disabled — skip start");
            return Ok(());
        }
        if self.running.load(Ordering::SeqCst) {
            let same = self
                .active_device
                .lock()
                .as_ref()
                .map(|d| d == device)
                .unwrap_or(false);
            if same {
                return Ok(());
            }
            // 设备切换：先停再启
            self.stop();
        }

        self.filter.lock().reset();
        *self.active_device.lock() = Some(device.to_string());
        *self.last_ok_frame.lock() = std::time::Instant::now();
        *self.consecutive_frame_fail.lock() = 0;

        let detector = self.detector.clone();
        let filter = self.filter.clone();
        let roi = self.roi.clone();
        let event_tx = self.event_tx.lock().clone();
        let last_ok = self.last_ok_frame.clone();
        let fail_count = self.consecutive_frame_fail.clone();
        let debug = self.debug.clone();
        let running = self.running.clone();
        let last_hold = self.last_hold_secs.clone();
        let last_preview = self.last_preview_jpeg.clone();
        let last_det = self.last_detection.clone();

        // 降采样：每 N 帧推理一次；预览更低频
        // 持握秒数已改墙钟累计，不再依赖假定 fps
        let process_every = 2u32;
        let preview_every = 3u32;
        let frame_i = Arc::new(Mutex::new(0u32));

        let device = device.to_string();
        running.store(true, Ordering::SeqCst);

        self.camera.start(&device, 15, move |w, h, rgb| {
            *last_ok.lock() = std::time::Instant::now();
            *fail_count.lock() = 0;

            let mut fi = frame_i.lock();
            *fi = fi.wrapping_add(1);
            let n = *fi;
            drop(fi);

            // UI 预览：更低频 JPEG，避免拖垮 CPU
            if n % preview_every == 0 {
                if let Some(jpeg) = encode_preview_jpeg(w, h, &rgb, 320) {
                    *last_preview.lock() = Some(jpeg);
                }
            }

            if n % process_every != 0 {
                return;
            }

            let (cw, ch, cropped) = roi.lock().crop_rgb(w, h, &rgb);
            let det = detector.lock().detect_rgb(cw, ch, &cropped);
            let positive = is_operating_phone(&det);
            *last_det.lock() = Some(det.clone());

            if debug.load(Ordering::SeqCst) {
                if let Some(ref tx) = event_tx {
                    let _ = tx.send(VisionEvent::DetectionDebug(det.clone()));
                }
                debug!(
                    target: "deepflow",
                    "det phone={} hand={} bright={} score={:.2} backend={} pos={}",
                    det.has_phone,
                    det.has_hand,
                    det.phone_brightness,
                    det.phone_score,
                    det.backend,
                    positive
                );
            }

            // 墙钟累计：每帧刷新 last_hold；秒数变化才发 FSM 事件
            {
                let mut filt = filter.lock();
                let changed = filt.push_frame_result(positive);
                let cur = filt.current_hold_secs();
                *last_hold.lock() = cur;
                if let Some(hold) = changed {
                    if let Some(ref tx) = event_tx {
                        let _ = tx.send(VisionEvent::HoldSecs(hold));
                    }
                } else if cur > 0 {
                    // 同秒内也周期性上报，避免只在秒边界才触发阈值判断
                    // （process 降采样后一秒内可能只有 1 帧，changed 已覆盖）
                }
            }
        })?;

        // 看门狗：2s 无帧 → CameraBlocked
        let last_ok = self.last_ok_frame.clone();
        let event_tx2 = self.event_tx.lock().clone();
        let running2 = self.running.clone();
        let enabled = self.enabled.clone();
        std::thread::Builder::new()
            .name("deepflow-vision-wd".into())
            .spawn(move || {
                while running2.load(Ordering::SeqCst) {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    if !enabled.load(Ordering::SeqCst) {
                        continue;
                    }
                    if last_ok.lock().elapsed() > std::time::Duration::from_secs(2) {
                        if let Some(ref tx) = event_tx2 {
                            let _ = tx.send(VisionEvent::CameraBlocked);
                        }
                        // 避免刷屏
                        *last_ok.lock() = std::time::Instant::now();
                    }
                }
            })
            .ok();

        info!(target: "deepflow", "vision pipeline started");
        Ok(())
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        self.camera.stop();
        self.filter.lock().reset();
        *self.last_hold_secs.lock() = 0;
        *self.last_preview_jpeg.lock() = None;
        *self.last_detection.lock() = None;
        *self.active_device.lock() = None;
        info!(target: "deepflow", "vision pipeline stopped");
    }
}

fn encode_preview_jpeg(width: u32, height: u32, rgb: &[u8], max_w: u32) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || rgb.len() < (width * height * 3) as usize {
        return None;
    }
    let img = ImageBuffer::<Rgb<u8>, _>::from_raw(width, height, rgb.to_vec())?;
    let scale = (max_w as f32 / width as f32).min(1.0);
    let nw = ((width as f32) * scale).round().max(1.0) as u32;
    let nh = ((height as f32) * scale).round().max(1.0) as u32;
    let resized = if nw == width && nh == height {
        img
    } else {
        image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle)
    };
    let mut buf = Cursor::new(Vec::new());
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 70);
    enc.write_image(resized.as_raw(), nw, nh, image::ExtendedColorType::Rgb8)
        .ok()?;
    Some(buf.into_inner())
}
