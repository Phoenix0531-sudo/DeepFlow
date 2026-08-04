use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::JoinHandle;
use tokio::sync::mpsc::UnboundedSender;

#[cfg(windows)]
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
#[cfg(windows)]
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

static LAST_ESC_TIMESTAMP_MS: AtomicU64 = AtomicU64::new(0);
const DOUBLE_CLICK_THRESHOLD_MS: u64 = 400;

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
unsafe extern "system" fn low_level_keyboard_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code >= 0 {
        let w = w_param.0 as u32;
        if w == WM_KEYDOWN || w == WM_SYSKEYDOWN {
            let kb = *(l_param.0 as *const KBDLLHOOKSTRUCT);
            if kb.vkCode == VK_ESCAPE.0 as u32 {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                let previous = LAST_ESC_TIMESTAMP_MS.swap(now, Ordering::SeqCst);
                if previous != 0 && now.saturating_sub(previous) <= DOUBLE_CLICK_THRESHOLD_MS {
                    if let Some(ref sender) = GLOBAL_SENDER {
                        let _ = sender.send(KeyboardHookEvent::EmergencyEscapeTriggered);
                    }
                    LAST_ESC_TIMESTAMP_MS.store(0, Ordering::SeqCst);
                }
            }
        }
    }
    CallNextHookEx(None, n_code, w_param, l_param)
}
