use image::{ImageBuffer, Rgb};
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType, Resolution};
use nokhwa::{Camera, NokhwaError};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use tracing::{debug, error, info, warn};

pub type RgbFrame = ImageBuffer<Rgb<u8>, Vec<u8>>;

/// 摄像头帧回调：RGBA/RGB8 宽高 + 像素。
pub trait CameraFramesink: Send {
    fn on_frame(&mut self, width: u32, height: u32, rgb: &[u8]);
}

/// 列出可用摄像头（名称优先，失败回退索引）。
pub fn list_cameras() -> Result<Vec<String>, String> {
    match nokhwa::query(nokhwa::utils::ApiBackend::MediaFoundation) {
        Ok(devices) => {
            if devices.is_empty() {
                return Ok(vec!["0".into()]);
            }
            Ok(devices
                .into_iter()
                .map(|d| {
                    let name = d.human_name();
                    if name.is_empty() {
                        format!("{}", d.index())
                    } else {
                        format!("{}|{}", d.index(), name)
                    }
                })
                .collect())
        }
        Err(e) => {
            warn!("camera query failed: {e}");
            Ok(vec!["0".into()])
        }
    }
}

fn parse_index(device: &str) -> CameraIndex {
    // "0" / "0|Integrated Camera" / 纯名称
    let head = device.split('|').next().unwrap_or(device).trim();
    if let Ok(i) = head.parse::<u32>() {
        return CameraIndex::Index(i);
    }
    // 按名称模糊匹配
    if let Ok(devices) = nokhwa::query(nokhwa::utils::ApiBackend::MediaFoundation) {
        for d in devices {
            if d.human_name().contains(device) || device.contains(&d.human_name()) {
                return d.index().clone();
            }
        }
    }
    CameraIndex::Index(0)
}

pub struct CameraController {
    running: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl CameraController {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            thread: Mutex::new(None),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn list_cameras() -> Result<Vec<String>, String> {
        list_cameras()
    }

    /// 在后台线程以 ~target_fps 拉流，每帧回调 sink。
    pub fn start<F>(&self, device: &str, target_fps: u32, mut on_frame: F) -> Result<(), String>
    where
        F: FnMut(u32, u32, Vec<u8>) + Send + 'static,
    {
        self.stop();
        let index = parse_index(device);
        let running = self.running.clone();
        running.store(true, Ordering::SeqCst);
        let fps = target_fps.max(1).min(30);

        let handle = std::thread::Builder::new()
            .name("deepflow-camera".into())
            .spawn(move || {
                // #31：断流自动重试（指数退避，上限 30s），running=false 时退出
                let mut attempt: u32 = 0;
                while running.load(Ordering::SeqCst) {
                    match camera_loop(index.clone(), fps, running.clone(), &mut on_frame) {
                        Ok(()) => {
                            // 正常 stop() 退出
                            break;
                        }
                        Err(e) => {
                            attempt = attempt.saturating_add(1);
                            let backoff_ms = (500u64 * 2u64.saturating_pow(attempt.min(6)))
                                .min(30_000);
                            error!(
                                "camera loop ended (attempt {attempt}): {e}; retry in {backoff_ms}ms"
                            );
                            // 分段 sleep，便于 stop() 及时打断
                            let mut waited = 0u64;
                            while waited < backoff_ms && running.load(Ordering::SeqCst) {
                                let step = 200u64.min(backoff_ms - waited);
                                std::thread::sleep(std::time::Duration::from_millis(step));
                                waited += step;
                            }
                        }
                    }
                }
                running.store(false, Ordering::SeqCst);
            })
            .map_err(|e| e.to_string())?;

        *self.thread.lock() = Some(handle);
        info!(target: "deepflow", "camera started device={device:?} fps={fps}");
        Ok(())
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(h) = self.thread.lock().take() {
            let _ = h.join();
            debug!(target: "deepflow", "camera thread joined");
        }
    }
}

impl Default for CameraController {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for CameraController {
    fn drop(&mut self) {
        self.stop();
    }
}

fn camera_loop<F>(
    index: CameraIndex,
    fps: u32,
    running: Arc<AtomicBool>,
    on_frame: &mut F,
) -> Result<(), String>
where
    F: FnMut(u32, u32, Vec<u8>),
{
    // 优先 MJPEG（带宽低）；失败再试 AbsoluteHighestResolution
    let mut cam = open_camera(index.clone(), fps).map_err(map_err)?;
    cam.open_stream().map_err(map_err)?;
    info!(
        target: "deepflow",
        "camera open {}x{} @ {:?}",
        cam.resolution().width(),
        cam.resolution().height(),
        cam.frame_format()
    );

    let frame_interval = std::time::Duration::from_millis((1000 / fps.max(1)) as u64);
    let mut decode_fail_logged = 0u32;
    // #31：连续抓帧失败超过阈值则退出 loop，由外层自动重连
    let mut consecutive_grab_fail = 0u32;
    const GRAB_FAIL_LIMIT: u32 = 30; // ~1.5s @ 50ms sleep，或更长取决于 fps
    while running.load(Ordering::SeqCst) {
        let t0 = std::time::Instant::now();
        match cam.frame() {
            Ok(buf) => {
                consecutive_grab_fail = 0;
                match decode_buffer_rgb(&buf) {
                    Ok((w, h, rgb)) => on_frame(w, h, rgb),
                    Err(e) => {
                        if decode_fail_logged < 5 {
                            warn!("decode frame: {e}");
                            decode_fail_logged += 1;
                        }
                    }
                }
            }
            Err(e) => {
                consecutive_grab_fail = consecutive_grab_fail.saturating_add(1);
                if consecutive_grab_fail <= 5 || consecutive_grab_fail % 10 == 0 {
                    warn!("grab frame ({consecutive_grab_fail}): {e}");
                }
                if consecutive_grab_fail >= GRAB_FAIL_LIMIT {
                    let _ = cam.stop_stream();
                    return Err(format!(
                        "camera stream stalled after {consecutive_grab_fail} grab failures: {e}"
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
        let elapsed = t0.elapsed();
        if elapsed < frame_interval {
            std::thread::sleep(frame_interval - elapsed);
        }
    }

    let _ = cam.stop_stream();
    Ok(())
}

fn open_camera(index: CameraIndex, fps: u32) -> Result<Camera, NokhwaError> {
    let candidates = [
        RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(
            nokhwa::utils::CameraFormat::new(
                Resolution::new(640, 480),
                nokhwa::utils::FrameFormat::MJPEG,
                fps,
            ),
        )),
        RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(
            nokhwa::utils::CameraFormat::new(
                Resolution::new(640, 480),
                nokhwa::utils::FrameFormat::YUYV,
                fps,
            ),
        )),
        RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate),
    ];
    let mut last = None;
    for req in candidates {
        match Camera::new(index.clone(), req) {
            Ok(cam) => return Ok(cam),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| {
        NokhwaError::GeneralError("no camera format available".into())
    }))
}

/// nokhwa decode，失败时用 image 直接解 MJPEG 原始缓冲。
fn decode_buffer_rgb(buf: &nokhwa::Buffer) -> Result<(u32, u32, Vec<u8>), String> {
    match buf.decode_image::<RgbFormat>() {
        Ok(img) => {
            let w = img.width();
            let h = img.height();
            Ok((w, h, img.into_raw()))
        }
        Err(e) => {
            // 兜底：把 buffer 当 JPEG 文件解（MJPEG 帧本身是完整 JPEG）
            if let Ok(dyn_img) = image::load_from_memory(buf.buffer()) {
                let rgb = dyn_img.to_rgb8();
                return Ok((rgb.width(), rgb.height(), rgb.into_raw()));
            }
            Err(e.to_string())
        }
    }
}

fn map_err(e: NokhwaError) -> String {
    e.to_string()
}
