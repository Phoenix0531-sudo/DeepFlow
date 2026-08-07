use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::thread::JoinHandle;
use tokio::sync::mpsc::UnboundedSender;

#[cfg(windows)]
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
#[cfg(windows)]
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_ESCAPE, VK_F9, VK_MENU, VK_SHIFT,
};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

static LAST_ESC_TIMESTAMP_MS: AtomicU64 = AtomicU64::new(0);
const DOUBLE_CLICK_THRESHOLD_MS: u64 = 400;

/// 紧急键模式编码（进程内可热更新，无需重装 hook）。
/// 0 = double_esc
/// 1 = f9
/// 2 = ctrl_shift_e
/// 3 = ctrl_alt_q
static EMERGENCY_MODE: AtomicU32 = AtomicU32::new(0);

#[derive(Debug, Clone)]
pub enum KeyboardHookEvent {
    EmergencyEscapeTriggered,
}

pub struct KeyHookManager {
    _thread: Option<JoinHandle<()>>,
}

#[cfg(windows)]
static mut GLOBAL_SENDER: Option<UnboundedSender<KeyboardHookEvent>> = None;

#[cfg(windows)]
static mut HOOK_HANDLE: HHOOK = HHOOK(std::ptr::null_mut());

/// 解析设置字段 → 内部 mode id。未知值回落 double_esc。
pub fn parse_emergency_hotkey(s: &str) -> u32 {
    match s.trim().to_ascii_lowercase().as_str() {
        "double_esc" | "esc" | "double-esc" | "" => 0,
        "f9" => 1,
        "ctrl_shift_e" | "ctrl+shift+e" => 2,
        "ctrl_alt_q" | "ctrl+alt+q" => 3,
        _ => 0,
    }
}

pub fn set_emergency_hotkey(s: &str) {
    let mode = parse_emergency_hotkey(s);
    EMERGENCY_MODE.store(mode, Ordering::SeqCst);
    tracing::info!(target: "deepflow", "emergency hotkey mode={mode} raw={s}");
}

pub fn emergency_hotkey_label(mode: u32) -> &'static str {
    match mode {
        1 => "F9",
        2 => "Ctrl+Shift+E",
        3 => "Ctrl+Alt+Q",
        _ => "双击 ESC",
    }
}

impl KeyHookManager {
    pub fn spawn_hook(event_sender: UnboundedSender<KeyboardHookEvent>) -> Self {
        #[cfg(windows)]
        {
            let handle = std::thread::Builder::new()
                .name("deepflow-kb-hook".into())
                .spawn(move || unsafe {
                    GLOBAL_SENDER = Some(event_sender);
                    match SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard_proc), None, 0)
                    {
                        Ok(hook) => {
                            HOOK_HANDLE = hook;
                            tracing::info!("WH_KEYBOARD_LL installed");
                            let mut msg = MSG::default();
                            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                                let _ = TranslateMessage(&msg);
                                DispatchMessageW(&msg);
                            }
                            let _ = UnhookWindowsHookEx(hook);
                        }
                        Err(e) => {
                            tracing::error!("Failed to install WH_KEYBOARD_LL: {e:?}");
                        }
                    }
                })
                .ok();
            Self { _thread: handle }
        }
        #[cfg(not(windows))]
        {
            let _ = event_sender;
            Self { _thread: None }
        }
    }
}

#[cfg(windows)]
fn key_down(vk: u16) -> bool {
    unsafe { GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 }
}

#[cfg(windows)]
fn fire_emergency() {
    unsafe {
        if let Some(ref sender) = GLOBAL_SENDER {
            let _ = sender.send(KeyboardHookEvent::EmergencyEscapeTriggered);
        }
    }
}

#[cfg(windows)]
unsafe extern "system" fn low_level_keyboard_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code >= 0 {
        let w = w_param.0 as u32;
        if w == WM_KEYDOWN || w == WM_SYSKEYDOWN {
            let kb = *(l_param.0 as *const KBDLLHOOKSTRUCT);
            let vk = kb.vkCode;
            let mode = EMERGENCY_MODE.load(Ordering::SeqCst);

            match mode {
                1 => {
                    // F9 单击
                    if vk == VK_F9.0 as u32 {
                        fire_emergency();
                    }
                }
                2 => {
                    // Ctrl+Shift+E
                    if vk == 0x45
                        && key_down(VK_CONTROL.0)
                        && key_down(VK_SHIFT.0)
                        && !key_down(VK_MENU.0)
                    {
                        fire_emergency();
                    }
                }
                3 => {
                    // Ctrl+Alt+Q
                    if vk == 0x51
                        && key_down(VK_CONTROL.0)
                        && key_down(VK_MENU.0)
                    {
                        fire_emergency();
                    }
                }
                _ => {
                    // double ESC
                    if vk == VK_ESCAPE.0 as u32 {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0);
                        let previous = LAST_ESC_TIMESTAMP_MS.swap(now, Ordering::SeqCst);
                        if previous != 0 && now.saturating_sub(previous) <= DOUBLE_CLICK_THRESHOLD_MS
                        {
                            fire_emergency();
                            LAST_ESC_TIMESTAMP_MS.store(0, Ordering::SeqCst);
                        }
                    }
                }
            }
        }
    }
    CallNextHookEx(None, n_code, w_param, l_param)
}
