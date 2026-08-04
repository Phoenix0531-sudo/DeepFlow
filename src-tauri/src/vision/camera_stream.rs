/// P0：摄像头流骨架（留空实现，P1 接 nokhwa + ONNX）。
pub struct CameraController;

impl CameraController {
    pub fn open(_device: &str) -> Result<Self, String> {
        Ok(Self)
    }
    pub fn list_cameras() -> Result<Vec<String>, String> {
        Ok(vec!["(P1) 默认摄像头".to_string()])
    }
}

pub trait CameraFramesink {
    fn on_frame(&mut self, width: u32, height: u32, pixels: &[u8]);
}
