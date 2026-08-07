use image::{imageops, RgbImage};
use ndarray::Array4;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;
use parking_lot::Mutex;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize)]
pub struct Detection {
    pub has_phone: bool,
    pub has_hand: bool,
    pub phone_brightness: u8,
    pub hand_phone_overlap: bool,
    /// 最高置信度（手机类）。
    pub phone_score: f32,
    pub backend: String,
}

impl Default for Detection {
    fn default() -> Self {
        Self {
            has_phone: false,
            has_hand: false,
            phone_brightness: 0,
            hand_phone_overlap: false,
            phone_score: 0.0,
            backend: "none".into(),
        }
    }
}

/// 是否判定为「正在操作手机」：亮屏手机 或 手-机重叠。
pub fn is_operating_phone(d: &Detection) -> bool {
    if !d.has_phone {
        return false;
    }
    // 黑屏手机在桌上不算操作
    if d.phone_brightness < 40 && !d.hand_phone_overlap {
        return false;
    }
    d.hand_phone_overlap || d.phone_brightness >= 40 || d.has_hand
}

pub trait Detector: Send {
    fn detect_rgb(&self, width: u32, height: u32, rgb: &[u8]) -> Detection;
}

/// 无模型时的启发式：中心 ROI 亮度 + 边缘高对比（粗略手机矩形）。
pub struct HeuristicDetector;

impl Detector for HeuristicDetector {
    fn detect_rgb(&self, width: u32, height: u32, rgb: &[u8]) -> Detection {
        if width == 0 || height == 0 || rgb.len() < (width * height * 3) as usize {
            return Detection {
                backend: "heuristic".into(),
                ..Default::default()
            };
        }
        let x0 = width * 3 / 10;
        let x1 = width * 7 / 10;
        let y0 = height * 3 / 10;
        let y1 = height * 7 / 10;
        let mut sum: u64 = 0;
        let mut n: u64 = 0;
        let mut bright_pixels: u64 = 0;
        for y in y0..y1 {
            for x in x0..x1 {
                let i = ((y * width + x) * 3) as usize;
                let r = rgb[i] as u64;
                let g = rgb[i + 1] as u64;
                let b = rgb[i + 2] as u64;
                let yv = (r * 30 + g * 59 + b * 11) / 100;
                sum += yv;
                n += 1;
                if yv > 160 {
                    bright_pixels += 1;
                }
            }
        }
        let avg = if n > 0 { (sum / n) as u8 } else { 0 };
        let bright_ratio = if n > 0 {
            bright_pixels as f32 / n as f32
        } else {
            0.0
        };
        let has_phone = avg > 90 && bright_ratio > 0.12;
        let has_hand = bright_ratio > 0.08 && avg > 70;
        Detection {
            has_phone,
            has_hand,
            phone_brightness: avg,
            hand_phone_overlap: has_phone && has_hand,
            phone_score: if has_phone { bright_ratio } else { 0.0 },
            backend: "heuristic".into(),
        }
    }
}

/// YOLO ONNX 检测器（COCO class 67 = cell phone）。
pub struct OnnxYoloDetector {
    session: Mutex<Session>,
    input_size: u32,
    conf_thres: f32,
    phone_class: i32,
}

impl OnnxYoloDetector {
    pub fn load(model_path: &Path, prefer_cpu: bool) -> Result<Self, String> {
        if !model_path.exists() {
            return Err(format!("model not found: {}", model_path.display()));
        }
        let mut builder = Session::builder()
            .map_err(|e| e.to_string())?;
        builder = builder
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| e.to_string())?;
        builder = builder
            .with_intra_threads(2)
            .map_err(|e| e.to_string())?;

        let _ = prefer_cpu;
        let session = builder
            .commit_from_file(model_path)
            .map_err(|e| e.to_string())?;

        info!(
            target: "deepflow",
            "ONNX model loaded: {} inputs={} outputs={}",
            model_path.display(),
            session.inputs().len(),
            session.outputs().len()
        );

        Ok(Self {
            session: Mutex::new(session),
            input_size: 640,
            conf_thres: 0.35,
            phone_class: 67,
        })
    }

    fn letterbox(&self, img: &RgbImage) -> (RgbImage, f32, f32, f32) {
        let size = self.input_size;
        let (w, h) = (img.width() as f32, img.height() as f32);
        let scale = (size as f32 / w).min(size as f32 / h);
        let nw = (w * scale).round() as u32;
        let nh = (h * scale).round() as u32;
        let resized = imageops::resize(img, nw, nh, imageops::FilterType::Triangle);
        let mut out = RgbImage::from_pixel(size, size, image::Rgb([114, 114, 114]));
        let dx = ((size - nw) / 2) as i64;
        let dy = ((size - nh) / 2) as i64;
        imageops::overlay(&mut out, &resized, dx, dy);
        (out, scale, dx as f32, dy as f32)
    }

    fn preprocess(
        &self,
        width: u32,
        height: u32,
        rgb: &[u8],
    ) -> Result<(Array4<f32>, f32, f32, f32), String> {
        let img = RgbImage::from_raw(width, height, rgb.to_vec())
            .ok_or_else(|| "invalid rgb buffer".to_string())?;
        let (boxed, scale, dx, dy) = self.letterbox(&img);
        let size = self.input_size as usize;
        let mut arr = Array4::<f32>::zeros((1, 3, size, size));
        for y in 0..size {
            for x in 0..size {
                let p = boxed.get_pixel(x as u32, y as u32).0;
                arr[[0, 0, y, x]] = p[0] as f32 / 255.0;
                arr[[0, 1, y, x]] = p[1] as f32 / 255.0;
                arr[[0, 2, y, x]] = p[2] as f32 / 255.0;
            }
        }
        Ok((arr, scale, dx, dy))
    }

    fn parse_yolo_output(
        &self,
        data: &[f32],
        shape: &[usize],
        scale: f32,
        dx: f32,
        dy: f32,
        orig_w: u32,
        orig_h: u32,
        rgb: &[u8],
    ) -> Detection {
        if shape.len() < 3 {
            return Detection {
                backend: "onnx".into(),
                ..Default::default()
            };
        }
        // 去掉 batch 维后： [84, N] 或 [N, 84]
        let (n_attr, n_pred, transposed) = if shape.len() == 3 {
            if shape[1] < shape[2] {
                (shape[1], shape[2], true)
            } else {
                (shape[2], shape[1], false)
            }
        } else if shape[0] < shape[1] {
            (shape[0], shape[1], true)
        } else {
            (shape[1], shape[0], false)
        };

        let mut best_phone = 0.0f32;
        let mut best_box: Option<(f32, f32, f32, f32)> = None;
        let mut best_person = 0.0f32;

        for i in 0..n_pred {
            let get = |a: usize| -> f32 {
                if transposed {
                    data.get(a * n_pred + i).copied().unwrap_or(0.0)
                } else {
                    data.get(i * n_attr + a).copied().unwrap_or(0.0)
                }
            };
            if n_attr < 6 {
                continue;
            }
            let cx = get(0);
            let cy = get(1);
            let w = get(2);
            let h = get(3);

            let mut phone_s = 0.0f32;
            let mut person_s = 0.0f32;
            let class_count = n_attr.saturating_sub(4);
            for c in 0..class_count {
                let s = get(4 + c);
                if c == self.phone_class as usize {
                    phone_s = s;
                }
                if c == 0 {
                    person_s = s;
                }
            }
            if phone_s > best_phone && phone_s >= self.conf_thres {
                best_phone = phone_s;
                let x1 = (cx - w / 2.0 - dx) / scale;
                let y1 = (cy - h / 2.0 - dy) / scale;
                let x2 = (cx + w / 2.0 - dx) / scale;
                let y2 = (cy + h / 2.0 - dy) / scale;
                best_box = Some((x1, y1, x2, y2));
            }
            if person_s > best_person {
                best_person = person_s;
            }
        }

        let has_phone = best_phone >= self.conf_thres;
        let has_hand = best_person >= self.conf_thres * 0.8;
        let mut brightness = 0u8;
        if let Some((x1, y1, x2, y2)) = best_box {
            brightness = roi_brightness(rgb, orig_w, orig_h, x1, y1, x2, y2);
        }

        Detection {
            has_phone,
            has_hand,
            phone_brightness: brightness,
            hand_phone_overlap: has_phone && has_hand,
            phone_score: best_phone,
            backend: "onnx".into(),
        }
    }
}

impl Detector for OnnxYoloDetector {
    fn detect_rgb(&self, width: u32, height: u32, rgb: &[u8]) -> Detection {
        let (arr, scale, dx, dy) = match self.preprocess(width, height, rgb) {
            Ok(v) => v,
            Err(e) => {
                warn!("preprocess: {e}");
                return Detection {
                    backend: "onnx-err".into(),
                    ..Default::default()
                };
            }
        };

        let input = match Tensor::from_array(arr) {
            Ok(t) => t,
            Err(e) => {
                warn!("tensor: {e}");
                return Detection {
                    backend: "onnx-err".into(),
                    ..Default::default()
                };
            }
        };

        let mut session = self.session.lock();
        let outputs = match session.run(ort::inputs![input]) {
            Ok(o) => o,
            Err(e) => {
                warn!("ort run: {e}");
                return Detection {
                    backend: "onnx-err".into(),
                    ..Default::default()
                };
            }
        };

        let (_name, value) = match outputs.iter().next() {
            Some(v) => v,
            None => {
                return Detection {
                    backend: "onnx".into(),
                    ..Default::default()
                }
            }
        };

        match value.try_extract_array::<f32>() {
            Ok(view) => {
                let shape: Vec<usize> = view.shape().to_vec();
                // 保证 C 连续切片
                let owned = view.to_owned();
                let slice = owned.as_slice().unwrap_or(&[]);
                self.parse_yolo_output(slice, &shape, scale, dx, dy, width, height, rgb)
            }
            Err(e) => {
                warn!("extract: {e}");
                Detection {
                    backend: "onnx-err".into(),
                    ..Default::default()
                }
            }
        }
    }
}

fn roi_brightness(rgb: &[u8], w: u32, h: u32, x1: f32, y1: f32, x2: f32, y2: f32) -> u8 {
    let x0 = x1.max(0.0).min(w as f32 - 1.0) as u32;
    let y0 = y1.max(0.0).min(h as f32 - 1.0) as u32;
    let x1 = x2.max(0.0).min(w as f32 - 1.0) as u32;
    let y1 = y2.max(0.0).min(h as f32 - 1.0) as u32;
    if x1 <= x0 || y1 <= y0 {
        return 0;
    }
    let mut sum: u64 = 0;
    let mut n: u64 = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let i = ((y * w + x) * 3) as usize;
            if i + 2 >= rgb.len() {
                continue;
            }
            let r = rgb[i] as u64;
            let g = rgb[i + 1] as u64;
            let b = rgb[i + 2] as u64;
            sum += (r * 30 + g * 59 + b * 11) / 100;
            n += 1;
        }
    }
    if n == 0 {
        0
    } else {
        (sum / n) as u8
    }
}

/// 统一入口：优先 ONNX，失败回落启发式。
pub struct HybridDetector {
    inner: Box<dyn Detector>,
    kind: String,
}

impl HybridDetector {
    pub fn create(models_dir: &Path, prefer_cpu: bool) -> Self {
        let candidates = [
            models_dir.join("yolo11n.onnx"),
            models_dir.join("yolov8n.onnx"),
            models_dir.join("phone_detect.onnx"),
        ];
        for p in candidates {
            match OnnxYoloDetector::load(&p, prefer_cpu) {
                Ok(d) => {
                    info!(target: "deepflow", "using ONNX detector: {}", p.display());
                    return Self {
                        inner: Box::new(d),
                        kind: format!("onnx:{}", p.file_name().unwrap().to_string_lossy()),
                    };
                }
                Err(e) => {
                    warn!(target: "deepflow", "skip model {}: {e}", p.display());
                }
            }
        }
        info!(target: "deepflow", "no ONNX model — heuristic detector");
        Self {
            inner: Box::new(HeuristicDetector),
            kind: "heuristic".into(),
        }
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn model_search_paths(models_dir: &Path) -> Vec<PathBuf> {
        vec![
            models_dir.join("yolo11n.onnx"),
            models_dir.join("yolov8n.onnx"),
            models_dir.join("phone_detect.onnx"),
        ]
    }
}

impl Detector for HybridDetector {
    fn detect_rgb(&self, width: u32, height: u32, rgb: &[u8]) -> Detection {
        self.inner.detect_rgb(width, height, rgb)
    }
}

pub struct MockDetector;
impl Detector for MockDetector {
    fn detect_rgb(&self, _w: u32, _h: u32, _rgb: &[u8]) -> Detection {
        Detection {
            backend: "mock".into(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn det(has_phone: bool, has_hand: bool, bright: u8, overlap: bool) -> Detection {
        Detection {
            has_phone,
            has_hand,
            phone_brightness: bright,
            hand_phone_overlap: overlap,
            phone_score: if has_phone { 0.8 } else { 0.0 },
            backend: "test".into(),
        }
    }

    #[test]
    fn is_operating_no_phone_false() {
        // 根本没检测到手机 → 一定不算操作
        assert!(!is_operating_phone(&det(false, true, 200, false)));
        assert!(!is_operating_phone(&det(false, false, 0, false)));
    }

    #[test]
    fn is_operating_black_phone_no_hand_false() {
        // 黑屏手机在桌上：亮度<40 且 无手-机重叠 → 不算操作
        assert!(!is_operating_phone(&det(true, false, 30, false)));
        assert!(!is_operating_phone(&det(true, true, 10, false)));
    }

    #[test]
    fn is_operating_black_phone_with_overlap_true() {
        // 黑屏但手拿手机（重叠） → 仍算操作（在看手机）
        assert!(is_operating_phone(&det(true, false, 30, true)));
    }

    #[test]
    fn is_operating_bright_phone_true() {
        // 亮屏手机（≥40）即便无手也算操作
        assert!(is_operating_phone(&det(true, false, 40, false)));
        assert!(is_operating_phone(&det(true, false, 200, false)));
    }

    #[test]
    fn is_operating_phone_with_hand_true() {
        // 有手机且有手 → 操作
        assert!(is_operating_phone(&det(true, true, 100, false)));
        assert!(is_operating_phone(&det(true, true, 100, true)));
    }

    #[test]
    fn mock_detector_returns_defaults() {
        let d = MockDetector.detect_rgb(2, 2, &[0; 12]);
        assert!(!d.has_phone);
        assert_eq!(d.backend, "mock");
    }

    // --- HeuristicDetector ---

    fn frame(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
        let mut v = Vec::with_capacity((w * h * 3) as usize);
        for _ in 0..(w * h) {
            v.extend_from_slice(&rgb);
        }
        v
    }

    #[test]
    fn heuristic_empty_or_tiny_input_safe() {
        let h = HeuristicDetector;
        let d = h.detect_rgb(0, 0, &[]);
        assert!(!d.has_phone);
        assert_eq!(d.backend, "heuristic");
        // 缓冲区不足也不 panic
        let d2 = h.detect_rgb(4, 4, &[0; 8]);
        assert!(!d2.has_phone);
    }

    #[test]
    fn heuristic_dark_frame_no_phone() {
        // 全黑帧：avg 低、bright_ratio=0 → 不算手机
        let h = HeuristicDetector;
        let d = h.detect_rgb(100, 100, &frame(100, 100, [5, 5, 5]));
        assert!(!d.has_phone);
        assert!(!d.has_hand);
        assert!(d.phone_brightness < 40);
        assert!(!is_operating_phone(&d));
    }

    #[test]
    fn heuristic_bright_center_triggers_phone() {
        // 中心 ROI（30%–70%）全亮：avg>90 且 bright_ratio 高 → 算手机
        let h = HeuristicDetector;
        let d = h.detect_rgb(100, 100, &frame(100, 100, [255, 255, 255]));
        assert!(d.has_phone, "bright frame should detect phone");
        // 全亮 → 同时判手
        assert!(d.has_hand);
        assert!(is_operating_phone(&d));
    }

    #[test]
    fn heuristic_dim_border_no_phone() {
        // 仅亮在边缘、中心暗：center ROI 采样不到亮 → 不算手机
        let h = HeuristicDetector;
        let mut img = frame(100, 100, [200, 200, 200]); // 边缘亮
        // 中心 30%–70% 抹黑
        for y in 30..70 {
            for x in 30..70 {
                let i = ((y * 100 + x) * 3) as usize;
                img[i] = 5;
                img[i + 1] = 5;
                img[i + 2] = 5;
            }
        }
        let d = h.detect_rgb(100, 100, &img);
        assert!(!d.has_phone, "dark center should not trigger phone");
        assert!(!d.has_hand);
    }
}
