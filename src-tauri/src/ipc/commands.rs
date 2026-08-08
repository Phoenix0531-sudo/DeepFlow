use crate::app_state::AppState;
use crate::db::{SettingsRecord, WeeklyReport};
use crate::fsm::{FsmEvent, SystemState};
use crate::win32::process_guard::list_running_process_names;
use crate::win32::configure_overlay_window_style;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};

type S<'a> = State<'a, Arc<AppState>>;

/// #14：模型文件元信息。
#[derive(serde::Serialize)]
pub struct ModelEntry {
    pub name: String,
    pub size: u64,
    /// 修改时间（本地 RFC3339，缺失则为空串）。
    pub modified: String,
}

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

/// 数据路径策略诊断：mode / data_dir / exports / models / db。
#[tauri::command]
pub async fn get_path_info(state: S<'_>) -> Result<serde_json::Value, String> {
    let data = state.data_dir.clone();
    let mode = resolve_path_mode_label(&data);
    Ok(serde_json::json!({
        "mode": mode,
        "data_dir": data.to_string_lossy(),
        "exports_dir": data.join("exports").to_string_lossy(),
        "models_dir": data.join("models").to_string_lossy(),
        "db_path": data.join("deepflow.db").to_string_lossy(),
        "logs_dir": data.join("logs").to_string_lossy(),
        "portable_hint": "设置 DEEPFLOW_PORTABLE=1 或在 exe 旁放 portable.flag",
        "env_override": "DEEPFLOW_DATA_DIR",
    }))
}

fn resolve_path_mode_label(data: &std::path::Path) -> &'static str {
    if std::env::var("DEEPFLOW_DATA_DIR").is_ok() {
        return "env";
    }
    if std::env::var("DEEPFLOW_PORTABLE")
        .map(|v| {
            let t = v.trim().to_ascii_lowercase();
            t == "1" || t == "true" || t == "yes" || t == "on"
        })
        .unwrap_or(false)
    {
        return "portable";
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            for name in ["portable.flag", ".portable", "DeepFlow.portable"] {
                if parent.join(name).is_file() {
                    return "portable";
                }
            }
            // 开发产物路径特征
            let mut saw_target = false;
            for comp in exe.components() {
                let s = comp.as_os_str().to_string_lossy().to_ascii_lowercase();
                if s == "target" {
                    saw_target = true;
                } else if saw_target && (s == "debug" || s == "release") {
                    return "dev";
                }
            }
            // 与 exe 同级 → 便携/回退；LOCALAPPDATA 下 → install
            if let Ok(la) = std::env::var("LOCALAPPDATA") {
                let install = std::path::PathBuf::from(la).join("DeepFlow").join("data");
                if paths_equal(data, &install) {
                    return "install";
                }
            }
            if paths_equal(data, &parent.join("data")) {
                return "portable";
            }
        }
    }
    let _ = data;
    "fallback"
}

fn paths_equal(a: &std::path::Path, b: &std::path::Path) -> bool {
    let ca = a.canonicalize().unwrap_or_else(|_| a.to_path_buf());
    let cb = b.canonicalize().unwrap_or_else(|_| b.to_path_buf());
    ca == cb
}

/// 资源管理器打开文件或目录（导出 PNG / 数据目录）。
pub(crate) fn reveal_path_sync(p: &std::path::Path) -> Result<(), String> {
    if p.as_os_str().is_empty() {
        return Err("路径为空".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // 文件 → /select, 目录 → 直接打开
        let mut cmd = std::process::Command::new("explorer");
        if p.is_file() {
            cmd.arg(format!("/select,{}", p.to_string_lossy()));
        } else {
            let dir = if p.is_dir() {
                p.to_path_buf()
            } else {
                p.parent().unwrap_or(p).to_path_buf()
            };
            cmd.arg(dir);
        }
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.spawn().map_err(|e| format!("打开资源管理器失败: {e}"))?;
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        Err("仅 Windows 支持 reveal_path".into())
    }
}

/// 资源管理器打开文件或目录（导出 PNG / 数据目录）。
#[tauri::command]
pub async fn reveal_path(path: String) -> Result<(), String> {
    let p = std::path::PathBuf::from(path.trim());
    reveal_path_sync(&p)
}

/// #14：列出 data/models 目录下的 ONNX 模型文件名 + 字节数 + 修改时间戳（RFC3339）。
#[tauri::command]
pub async fn list_models(state: S<'_>) -> Result<Vec<ModelEntry>, String> {
    let dir = state.data_dir.join("models");
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for ent in rd.flatten() {
            let p = ent.path();
            if p.extension().and_then(|x| x.to_str()).map(|x| x.eq_ignore_ascii_case("onnx")).unwrap_or(false) {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                let size = ent.metadata().map(|m| m.len()).unwrap_or(0);
                let modified = ent.metadata().ok().and_then(|m| m.modified().ok())
                    .map(|t| {
                        let dt = chrono::DateTime::<chrono::Local>::from(t);
                        dt.to_rfc3339()
                    })
                    .unwrap_or_default();
                out.push(ModelEntry { name, size, modified });
            }
        }
    }
    Ok(out)
}

/// #14：手动重新触发 seed_models（从安装目录/资源旁复制缺失 ONNX），返回本次复制数。
#[tauri::command]
pub async fn reseed_models(state: S<'_>) -> Result<u32, String> {
    Ok(crate::seed_models_if_needed(&state.data_dir))
}

/// #15：返回指定历史周的聚合周报（weeks_ago=0 为本周）。
#[tauri::command]
pub async fn get_weekly_report_at(state: S<'_>, weeks_ago: u32) -> Result<WeeklyReport, String> {
    state.logger.lock().weekly_report_weeks_ago(weeks_ago).map_err(|e| e.to_string())
}

/// #16：返回最近 limit 条 L3 原因记录，每条为 (created_at, reason)。
#[tauri::command]
pub async fn get_l3_reasons(state: S<'_>, limit: Option<u32>) -> Result<Vec<(String, String)>, String> {
    let n = limit.unwrap_or(20).clamp(1, 200);
    state.logger.lock().list_l3_reasons(n).map_err(|e| e.to_string())
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
