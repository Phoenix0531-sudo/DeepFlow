pub mod keyboard_hook;
pub mod process_guard;
pub mod whitelist_action;
pub mod window_effects;

pub use keyboard_hook::{
    emergency_hotkey_label, parse_emergency_hotkey, set_emergency_hotkey, KeyHookManager,
    KeyboardHookEvent,
};
pub use process_guard::ProcessGuard;
pub use whitelist_action::{classify_whitelist_action, select_targets, WhitelistActionKind};
pub use window_effects::configure_overlay_window_style;
