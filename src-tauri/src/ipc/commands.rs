use crate::app_state::AppState;
use crate::db::{SettingsRecord, WeeklyReport};
use crate::fsm::{FsmEvent, SystemState};
use crate::win32::process_guard::list_running_process_names;
use crate::win32::configure_overlay_window_style;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};

type S<'a> = State<'a, Arc<AppState>>;

#[tauri::command]
pub async fn get_fsm_state(state: S<'_>) -> Result<SystemState, String> {
    Ok(state.fsm.get_state())
}

#[tauri::command]
pub async fn start_focus_session(
    app: AppHandle,
    duration_mins: Option<u32>,
    state: S<'_>,
) -> Result<SystemState, String> {
    let mins = duration_mins.unwrap_or_else(|| state.default_focus_mins());
    state.dispatch_and_apply(&app, FsmEvent::StartSession {
        focus_duration_mins: mins,
    })?;
    Ok(state.fsm.get_state())
}

#[tauri::command]
pub async fn request_temporary_pause(
    app: AppHandle,
    reason: String,
    state: S<'_>,
) -> Result<SystemState, String> {
    if reason.trim().is_empty() {
        return Err("请输入临时原因".into());
    }
    state.dispatch_and_apply(
        &app,
        FsmEvent::UserRequestPause {
            reason: reason.trim().to_string(),
        },
    )?;
    Ok(state.fsm.get_state())
}

#[tauri::command]
pub async fn resume_focus_session(
    app: AppHandle,
    state: S<'_>,
) -> Result<SystemState, String> {
    state.dispatch_and_apply(&app, FsmEvent::ResumeFocus)?;
    Ok(state.fsm.get_state())
}

#[tauri::command]
pub async fn skip_debt_and_resume(
    app: AppHandle,
    state: S<'_>,
) -> Result<SystemState, String> {
    state.dispatch_and_apply(&app, FsmEvent::SkipDebtAndResume)?;
    Ok(state.fsm.get_state())
}

#[tauri::command]
pub async fn stop_session(app: AppHandle, state: S<'_>) -> Result<SystemState, String> {
    state.dispatch_and_apply(&app, FsmEvent::StopSession)?;
    Ok(state.fsm.get_state())
}

/// 测试模式：强制进入 L1/L2/L3（仅 test_mode=true 时生效）。
#[tauri::command]
pub async fn test_inject_level(
    app: AppHandle,
    level: u32,
    state: S<'_>,
) -> Result<SystemState, String> {
    let s = state.load_settings()?;
    if !s.test_mode {
        return Err("仅测试模式可注入干预等级".into());
    }
    if !(1..=3).contains(&level) {
        return Err("level 必须为 1/2/3".into());
    }
    // 若尚无会话，先开一段短专注再注入
    if matches!(state.fsm.get_state(), SystemState::Idle) {
        state.dispatch_and_apply(
            &app,
            FsmEvent::StartSession {
                focus_duration_mins: 5,
            },
        )?;
    }
    state.dispatch_and_apply(&app, FsmEvent::TestInjectLevel { level })?;
    Ok(state.fsm.get_state())
}

/// 测试/逃生退出：对话框输入「测试」或主界面按钮。
/// 不再硬拦 test_mode——卡死时必须能退；非测试也会关遮罩回 Idle。
#[tauri::command]
pub async fn test_exit_session(
    app: AppHandle,
    state: S<'_>,
) -> Result<SystemState, String> {
    Ok(state.force_exit_everything(&app))
}

/// 强制逃生：始终可用，不依赖 test_mode / 当前状态。
#[tauri::command]
pub async fn force_exit_session(
    app: AppHandle,
    state: S<'_>,
) -> Result<SystemState, String> {
    tracing::warn!(target: "deepflow", "force_exit_session invoked");
    Ok(state.force_exit_everything(&app))
}

#[tauri::command]
pub async fn acknowledge_level2(app: AppHandle, state: S<'_>) -> Result<(), String> {
    state.dispatch_and_apply(&app, FsmEvent::AcknowledgeLevel2)?;
    Ok(())
}

#[tauri::command]
pub async fn submit_l3_reason(
    app: AppHandle,
    reason: String,
    state: S<'_>,
) -> Result<SystemState, String> {
    state.dispatch_and_apply(
        &app,
        FsmEvent::SubmitL3Reason {
            reason: reason.trim().to_string(),
        },
    )?;
    Ok(state.fsm.get_state())
}

#[tauri::command]
pub async fn choose_session_end(
    app: AppHandle,
    choice: String,
    state: S<'_>,
) -> Result<SystemState, String> {
    let ev = match choice.as_str() {
        "continue" => FsmEvent::ChooseContinue,
        "rest" => FsmEvent::ChooseRest,
        "end" => FsmEvent::ChooseEnd,
        _ => return Err(format!("未知 choice: {choice}")),
    };
    state.dispatch_and_apply(&app, ev)?;
    Ok(state.fsm.get_state())
}

#[tauri::command]
pub async fn get_settings(state: S<'_>) -> Result<SettingsRecord, String> {
    state.load_settings()
}

#[tauri::command]
pub async fn save_settings(
    app: AppHandle,
    settings: SettingsRecord,
    state: S<'_>,
) -> Result<(), String> {
    state.save_settings(&app, settings)
}

#[tauri::command]
pub async fn get_today_focus_secs(state: S<'_>) -> Result<u32, String> {
    Ok(state.today_focus_secs())
}

#[tauri::command]
pub async fn get_weekly_report(state: S<'_>) -> Result<WeeklyReport, String> {
    state.weekly_report()
}

/// P2：导出周报分享图 PNG → data/exports/weekly-report-*.png
#[tauri::command]
pub async fn export_weekly_report_png(state: S<'_>) -> Result<String, String> {
    state.export_weekly_png()
}

#[tauri::command]
pub async fn get_data_dir(state: S<'_>) -> Result<String, String> {
    Ok(state.data_dir.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn list_running_processes() -> Result<Vec<String>, String> {
    Ok(list_running_process_names())
}

#[tauri::command]
pub async fn get_available_cameras() -> Result<Vec<String>, String> {
    crate::vision::list_cameras()
}

#[tauri::command]
pub async fn get_vision_status(state: S<'_>) -> Result<serde_json::Value, String> {
    let settings = state.load_settings().ok();
    let det = state.vision.last_detection();
    Ok(serde_json::json!({
        "running": state.vision.is_running(),
        "enabled": settings
            .as_ref()
            .map(|s| s.vision_enabled)
            .unwrap_or_else(|| state.vision.is_enabled()),
        "detector": state.vision.detector_kind(),
        "hold_secs": state.vision.last_hold_secs(),
        "camera_name": settings
            .as_ref()
            .map(|s| s.camera_name.clone())
            .unwrap_or_default(),
        "has_preview": state.vision.preview_jpeg().is_some(),
        "last_detection": det,
    }))
}

#[tauri::command]
pub async fn get_vision_preview(state: S<'_>) -> Result<Option<String>, String> {
    use base64::Engine;
    match state.vision.preview_jpeg() {
        Some(bytes) => Ok(Some(
            base64::engine::general_purpose::STANDARD.encode(bytes),
        )),
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn restart_vision(state: S<'_>) -> Result<(), String> {
    state.vision.stop();
    let s = state.load_settings()?;
    if !s.vision_enabled {
        return Ok(());
    }
    let device = if s.camera_name.is_empty() {
        "0".into()
    } else {
        s.camera_name
    };
    state.vision.set_enabled(true);
    state.vision.start(&device)
}

/// Setup / 调试：临时开摄像头预览（不依赖当前会话 FSM）。
#[tauri::command]
pub async fn start_vision_preview(
    device: Option<String>,
    state: S<'_>,
) -> Result<(), String> {
    let s = state.load_settings().unwrap_or_default();
    let dev = device
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| {
            if s.camera_name.is_empty() {
                "0".into()
            } else {
                s.camera_name.clone()
            }
        });
    state.vision.set_enabled(true);
    // start() 自身幂等：同设备不重启，换设备才重启
    state.vision.start(&dev)
}

#[tauri::command]
pub async fn stop_vision_preview(state: S<'_>) -> Result<(), String> {
    state.vision.stop();
    Ok(())
}

#[tauri::command]
pub async fn apply_overlay_native_style(
    app: AppHandle,
    label: String,
) -> Result<(), String> {
    // 软失败：窗体销毁中不要让前端看到硬错误
    let Some(win) = app.get_webview_window(&label) else {
        return Ok(());
    };
    if let Ok(hwnd) = win.hwnd() {
        configure_overlay_window_style(hwnd.0 as isize);
    }
    Ok(())
}

/// 关遮罩：先 hide + 穿透点击，再 destroy。
/// 仅 hide 在部分 Win/WebView2 组合下会留下全屏不可点幽灵窗。
pub fn force_hide_overlay(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.set_ignore_cursor_events(true);
        let _ = w.hide();
        // destroy 比 close 更干净，避免半死 webview → invoke 报
        // "failed to acquire webview reference"
        let _ = w.destroy();
    }
}

pub fn force_hide_floating(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("floating-clock") {
        let _ = w.hide();
        let _ = w.destroy();
    }
}

#[tauri::command]
pub async fn open_overlay_window(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("overlay") {
        // 复用已有 webview，避免反复 create/destroy 导致句柄失效
        let _ = w.set_ignore_cursor_events(false);
        let _ = w.show();
        let _ = w.set_focus();
        if let Ok(hwnd) = w.hwnd() {
            configure_overlay_window_style(hwnd.0 as isize);
        }
        return Ok(());
    }
    let win = WebviewWindowBuilder::new(
        &app,
        "overlay",
        WebviewUrl::App("index.html?window=overlay".into()),
    )
    .title("DeepFlow Overlay")
    .fullscreen(true)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(true)
    .build()
    .map_err(|e| e.to_string())?;

    if let Ok(hwnd) = win.hwnd() {
        configure_overlay_window_style(hwnd.0 as isize);
    }
    Ok(())
}

#[tauri::command]
pub async fn close_overlay_window(app: AppHandle) -> Result<(), String> {
    force_hide_overlay(&app);
    Ok(())
}

#[tauri::command]
pub async fn open_floating_clock(app: AppHandle) -> Result<(), String> {
    if app.get_webview_window("floating-clock").is_some() {
        if let Some(w) = app.get_webview_window("floating-clock") {
            let _ = w.show();
        }
        return Ok(());
    }
    WebviewWindowBuilder::new(
        &app,
        "floating-clock",
        WebviewUrl::App("index.html?window=floating".into()),
    )
    .title("DeepFlow Clock")
    // 紧凑布局：单行头 + 大计时 + 并排按钮，避免 220 高溢出
    .inner_size(260.0, 168.0)
    .min_inner_size(220.0, 140.0)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .visible(true)
    .build()
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn close_floating_clock(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("floating-clock") {
        let _ = w.close();
    }
    Ok(())
}

#[tauri::command]
pub async fn open_setup_window(app: AppHandle) -> Result<(), String> {
    if app.get_webview_window("setup").is_some() {
        return Ok(());
    }
    WebviewWindowBuilder::new(
        &app,
        "setup",
        WebviewUrl::App("index.html?window=setup".into()),
    )
    .title("DeepFlow 首次配置")
    .inner_size(960.0, 640.0)
    .center()
    .build()
    .map_err(|e| e.to_string())?;
    Ok(())
}
