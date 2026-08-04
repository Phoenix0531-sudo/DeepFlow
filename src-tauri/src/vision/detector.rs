use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Detection {
    pub has_phone: bool,
    pub has_hand: bool,
    pub phone_brightness: u8,
    pub hand_phone_overlap: bool,
}

pub trait Detector {
    fn detect(&self, pixels: &[u8]) -> Detection;
}

pub struct MockDetector;

impl Detector for MockDetector {
    fn detect(&self, _pixels: &[u8]) -> Detection {
        Detection {
            has_phone: false,
            has_hand: false,
            phone_brightness: 0,
            hand_phone_overlap: false,
        }
    }
}
