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

#[tauri::command]
pub async fn list_running_processes() -> Result<Vec<String>, String> {
    Ok(list_running_process_names())
}

#[tauri::command]
pub async fn get_available_cameras() -> Result<Vec<String>, String> {
    crate::vision::CameraController::list_cameras()
}

#[tauri::command]
pub async fn apply_overlay_native_style(
    app: AppHandle,
    label: String,
) -> Result<(), String> {
    let win = app
        .get_webview_window(&label)
        .ok_or_else(|| format!("window not found: {label}"))?;
    if let Ok(hwnd) = win.hwnd() {
        configure_overlay_window_style(hwnd.0 as isize);
    }
    Ok(())
}

#[tauri::command]
pub async fn open_overlay_window(app: AppHandle) -> Result<(), String> {
    if app.get_webview_window("overlay").is_some() {
        if let Some(w) = app.get_webview_window("overlay") {
            let _ = w.show();
            let _ = w.set_focus();
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
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.close();
    }
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
    .inner_size(220.0, 160.0)
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
    .inner_size(720.0, 560.0)
    .center()
    .build()
    .map_err(|e| e.to_string())?;
    Ok(())
}
