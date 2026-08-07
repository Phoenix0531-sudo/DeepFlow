//! Rust → React 事件名常量。

pub const EVT_FSM_STATE_CHANGE: &str = "fsm_state_change";
pub const EVT_WHITELIST_HIT: &str = "whitelist_hit";
pub const EVT_TODAY_FOCUS: &str = "today_focus_secs";
pub const EVT_DEBUG_LOG: &str = "debug_log";
pub const EVT_SESSION_END_CHOICE: &str = "session_end_choice";
/// Frontend plays local WebAudio beep: "chime" | "severe" | "inject"
pub const EVT_PLAY_SOUND: &str = "play_sound";
