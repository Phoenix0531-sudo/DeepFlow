use crate::app_state::AppState;
use crate::db::{RecentWeeklyReport, SettingsRecord, WeeklyReport};
use crate::fsm::{FsmEvent, SystemState};
use crate::win32::process_guard::list_running_process_names;
use crate::win32::configure_overlay_window_style;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

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

/// #14：手动重新触发 seed_models（从 bundle.resources 的 seed_models/
/// 目录复制缺失 ONNX），返回本次复制数。需要 tauri app handle 拿 resource_dir——
/// install 模式唯一可靠来源（cwd 兜底已移除）。
#[tauri::command]
pub async fn reseed_models(
    app: tauri::AppHandle,
    state: S<'_>,
) -> Result<u32, String> {
    let resource_dir = app
        .path()
        .resource_dir()
        .ok();
    Ok(crate::seed_models_if_needed(&state.data_dir, resource_dir.as_deref()))
}

/// #15：返回指定历史周的聚合周报（weeks_ago=0 为本周）。
#[tauri::command]
pub async fn get_weekly_report_at(state: S<'_>, weeks_ago: u32) -> Result<WeeklyReport, String> {
    state.logger.lock().weekly_report_weeks_ago(weeks_ago).map_err(|e| e.to_string())
}

/// #8：跨周趋势。返回最近 `count` 周（从本周往前）的带索引周报，
/// 供前端趋势图轴渲染。count 被 clamp 到 1..=12。
#[tauri::command]
pub async fn get_weekly_reports_recent(state: S<'_>, count: u32) -> Result<Vec<RecentWeeklyReport>, String> {
    state.logger.lock().weekly_reports_recent(count).map_err(|e| e.to_string())
}

/// #16：返回最近 limit 条 L3 原因记录，每条为 (created_at, reason)。
#[tauri::command]
pub async fn get_l3_reasons(state: S<'_>, limit: Option<u32>) -> Result<Vec<(String, String)>, String> {
    let n = limit.unwrap_or(20).clamp(1, 200);
    state.logger.lock().list_l3_reasons(n).map_err(|e| e.to_string())
}

/// #28 B1：导出全部数据为 JSON 文件，写入 data/exports，返回文件路径。
#[tauri::command]
pub async fn export_all_data(state: S<'_>) -> Result<String, String> {
    let json = state
        .logger
        .lock()
        .export_all_json()
        .map_err(|e| e.to_string())?;
    let exports = state.data_dir.join("exports");
    std::fs::create_dir_all(&exports).map_err(|e| e.to_string())?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = exports.join(format!("deepflow-export-{stamp}.json"));
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// #28 B1：清空历史记录。clear_settings=true 时同时重置 settings。
#[tauri::command]
pub async fn clear_all_data(
    app: AppHandle,
    clear_settings: Option<bool>,
    state: S<'_>,
) -> Result<(), String> {
    let reset = clear_settings.unwrap_or(false);
    state
        .logger
        .lock()
        .clear_all_data(reset)
        .map_err(|e| e.to_string())?;
    if reset {
        // 从 DB 重新装入 settings 并刷新缓存
        if let Ok(s) = state.logger.lock().load_settings() {
            let _ = state.save_settings(&app, s);
        }
    }
    // 今日专注累计清零
    state.reset_today_focus_secs();
    let _ = app.emit(crate::ipc::events::EVT_TODAY_FOCUS, 0u32);
    Ok(())
}

/// #7 清空反悔 - 快照版 IPC。清空 + 快照写入 Rust 侧 state 缓存，不返回任何 payload。
/// 反悔请调 restore_last_snapshot (前端无需拋讯 base64,避免 settings 全字段在前端往返)。
#[tauri::command]
pub async fn clear_all_data_with_snapshot(
    app: AppHandle,
    clear_settings: Option<bool>,
    state: S<'_>,
) -> Result<(), String> {
    let reset = clear_settings.unwrap_or(false);
    // #review F7：快照字节仅存 Rust 侧 Mutex,前端拿不到。
    let snap = state
        .logger
        .lock()
        .clear_all_data_with_snapshot(reset)
        .map_err(|e| e.to_string())?;
    state.set_last_clear_snapshot(snap);
    if reset {
        if let Ok(s) = state.logger.lock().load_settings() {
            let _ = state.save_settings(&app, s);
        }
    }
    state.reset_today_focus_secs();
    let _ = app.emit(crate::ipc::events::EVT_TODAY_FOCUS, 0u32);
    Ok(())
}

/// #7 清空反悔 - 还原 IPC。无参数：从 Rust 侧 state 缓存取最后一次 clear_all_data_with_snapshot
/// 的快照，一次性 take 后置为 None。未缓存时返错提示“无可撤销操作”。
#[tauri::command]
pub async fn restore_last_snapshot(
    app: AppHandle,
    state: S<'_>,
) -> Result<(), String> {
    let bytes = state.take_last_clear_snapshot().ok_or_else(|| {
        "无可用快照（已过期或未调用 clear_all_data_with_snapshot 时快照不保留）".to_string()
    })?;
    state
        .logger
        .lock()
        .restore_from_snapshot(&bytes)
        .map_err(|e| e.to_string())?;
    // 快照可能还原了 settings，重新装入并刷新缓存
    if let Ok(s) = state.logger.lock().load_settings() {
        let _ = state.save_settings(&app, s);
    }
    // 同步今日专注累计到 restored daily_focus today 行
    let today = state.logger.lock().today_focus_secs().unwrap_or(0);
    state.set_today_focus_secs(today);
    let _ = app.emit(crate::ipc::events::EVT_TODAY_FOCUS, today);
    Ok(())
}

/// #34 B2：备份当前设置为 JSON 文件，返回路径。
#[tauri::command]
pub async fn backup_settings(state: S<'_>) -> Result<String, String> {
    let s = state.load_settings()?;
    let exports = state.data_dir.join("exports");
    std::fs::create_dir_all(&exports).map_err(|e| e.to_string())?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = exports.join(format!("deepflow-settings-{stamp}.json"));
    let body = serde_json::json!({
        "kind": "deepflow_settings_backup",
        "schema_version": 3,
        "exported_at": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        "settings": s,
    });
    std::fs::write(&path, serde_json::to_string_pretty(&body).unwrap_or_default())
        .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// #34 B2：从 JSON 备份恢复设置（仅 settings 字段；忽略历史日志）。
#[tauri::command]
pub async fn restore_settings(
    app: AppHandle,
    path: String,
    state: S<'_>,
) -> Result<SettingsRecord, String> {
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("读取失败: {e}"))?;
    let v: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| format!("JSON 无效: {e}"))?;
    let settings_val = v
        .get("settings")
        .cloned()
        .or_else(|| if v.get("default_focus_mins").is_some() { Some(v.clone()) } else { None })
        .ok_or_else(|| "备份文件中找不到 settings".to_string())?;
    let mut s: SettingsRecord =
        serde_json::from_value(settings_val).map_err(|e| format!("settings 解析失败: {e}"))?;
    // 恢复后强制 setup_completed，避免被踢回首次配置
    s.setup_completed = true;
    state.save_settings(&app, s.clone())?;
    Ok(s)
}

/// #23：登录时自启管理（读取当前状态）
#[tauri::command]
pub async fn get_autostart_enabled(app: AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    let am = app.autolaunch();
    am.is_enabled().map_err(|e| e.to_string())
}

/// #23：开启/关闭登录自启
#[tauri::command]
pub async fn set_autostart_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let am = app.autolaunch();
    if enabled {
        am.enable().map_err(|e| e.to_string())?;
    } else {
        am.disable().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// #29：请求系统通知权限（首次申请）
#[tauri::command]
pub async fn request_notification_permission(app: AppHandle) -> Result<bool, String> {
    use tauri_plugin_notification::NotificationExt;
    let state = app.notification().request_permission().map_err(|e| e.to_string())?;
    Ok(matches!(state, tauri_plugin_notification::PermissionState::Granted))
}

/// #29：发送系统通知
#[tauri::command]
pub async fn send_notification(app: AppHandle, title: String, body: String) -> Result<(), String> {
    use tauri_plugin_notification::NotificationExt;
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|e| e.to_string())
}

/// #4 updater 配置可测状态。用于 check_for_updates 在调取 updater_builder 之前、
/// 根据 `plugins.updater` 中的 `endpoints` 与 `pubkey` 判定是否真正配置过。
/// 提出为纯函数便于单测(不需要 AppHandle)。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum UpdaterConfigStatus {
    /// endpoints 或 pubkey 任一为空：走 configured:false 分支。
    NotConfigured(&'static str),
    /// 两者均非空：可尝试调 updater_builder().check()。
    Configured,
}

/// 判定 `plugins.updater` 配置是否完备。
/// - `endpoints`: 必须是数组且非空
/// - `pubkey`: 必须是字符串且 trim 后非空
pub(crate) fn updater_config_status(updater_cfg: Option<&serde_json::Value>) -> UpdaterConfigStatus {
    let endpoints_empty = updater_cfg
        .and_then(|v| v.get("endpoints"))
        .map(|v| v.as_array().map(|a| a.is_empty()).unwrap_or(true))
        .unwrap_or(true);
    let pubkey_empty = updater_cfg
        .and_then(|v| v.get("pubkey"))
        .map(|v| v.as_str().map(|s| s.trim().is_empty()).unwrap_or(true))
        .unwrap_or(true);
    if endpoints_empty || pubkey_empty {
        return UpdaterConfigStatus::NotConfigured("updater endpoints 或 pubkey 未配置");
    }
    UpdaterConfigStatus::Configured
}

/// #33：检查更新。未配置 updater（endpoints/pubkey 任一为空）时返回
/// `{ available: false, configured: false }`，前端据此给出"未配置"提示，
/// 而不是误导用户"已是最新版本"。
#[tauri::command]
pub async fn check_for_updates(app: AppHandle) -> Result<serde_json::Value, String> {
    use tauri_plugin_updater::UpdaterExt;
    // 读取 tauri.conf.json 的 plugins.updater 配置，检测是否真正配置过。
    let cfg = app.config();
    let updater_cfg = cfg.plugins.0.get("updater");
    if let UpdaterConfigStatus::NotConfigured(reason) = updater_config_status(updater_cfg) {
        return Ok(serde_json::json!({
            "available": false,
            "configured": false,
            "reason": reason,
        }));
    }
    let resp = app
        .updater_builder()
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    if let Some(update) = resp {
        Ok(serde_json::json!({
            "available": true,
            "configured": true,
            "version": update.version,
            "date": update.date.map(|d| d.to_string()).unwrap_or_default(),
            "body": update.body.clone(),
        }))
    } else {
        Ok(serde_json::json!({
            "available": false,
            "configured": true,
        }))
    }
}

/// #33：下载并安装更新（仅当已配置 updater 且存在更新时才调用）。安装完成后重启。
#[tauri::command]
pub async fn download_and_install_update(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    let resp = app
        .updater_builder()
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    if let Some(update) = resp {
        update
            .download_and_install(|_, _| {}, || {})
            .await
            .map_err(|e| e.to_string())?;
        app.restart();
    }
    Ok(())
}

/// #48：关于/版本信息（读 tauri.conf.json 的 productName 与 version）
#[tauri::command]
pub async fn get_app_version(app: AppHandle) -> Result<serde_json::Value, String> {
    let cfg = app.config();
    Ok(serde_json::json!({
        "name": cfg.product_name.clone().unwrap_or_else(|| "DeepFlow".into()),
        "version": cfg.version.clone(),
        "identifier": cfg.identifier.clone(),
    }))
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn updater_block_ref(value: &serde_json::Value) -> Option<&serde_json::Value> {
        Some(value)
    }

    #[test]
    fn updater_status_none_when_missing() {
        // 完全没有 plugins.updater 块
        assert_eq!(
            updater_config_status(None),
            UpdaterConfigStatus::NotConfigured("updater endpoints 或 pubkey 未配置")
        );
    }

    #[test]
    fn updater_status_not_configured_when_endpoints_empty() {
        let cfg = json!({
            "endpoints": [],
            "pubkey": "dW50cnVzdGVkIGNvbW1lbnQ6IHNvbWVwdWJrZXkK"
        });
        assert_eq!(
            updater_config_status(updater_block_ref(&cfg)),
            UpdaterConfigStatus::NotConfigured("updater endpoints 或 pubkey 未配置")
        );
    }

    #[test]
    fn updater_status_not_configured_when_pubkey_empty() {
        let cfg = json!({
            "endpoints": ["https://example.com/updates.json"],
            "pubkey": ""
        });
        assert_eq!(
            updater_config_status(updater_block_ref(&cfg)),
            UpdaterConfigStatus::NotConfigured("updater endpoints 或 pubkey 未配置")
        );
    }

    #[test]
    fn updater_status_not_configured_when_pubkey_only_whitespace() {
        // trim 后为空也要判为未配置(避免 "   " 这种配置越过校验)
        let cfg = json!({
            "endpoints": ["https://example.com/updates.json"],
            "pubkey": "   \n  "
        });
        assert_eq!(
            updater_config_status(updater_block_ref(&cfg)),
            UpdaterConfigStatus::NotConfigured("updater endpoints 或 pubkey 未配置")
        );
    }

    #[test]
    fn updater_status_not_configured_when_endpoints_not_array() {
        // endpoints 不是数组(类型错误) → 保护性判为空
        let cfg = json!({
            "endpoints": "https://example.com/updates.json",
            "pubkey": "somekey"
        });
        assert_eq!(
            updater_config_status(updater_block_ref(&cfg)),
            UpdaterConfigStatus::NotConfigured("updater endpoints 或 pubkey 未配置")
        );
    }

    #[test]
    fn updater_status_configured_when_both_nonempty() {
        let cfg = json!({
            "endpoints": ["https://example.com/updates.json"],
            "pubkey": "dW50cnVzdGVkIGNvbW1lbnQ6IHNvbWVwdWJrZXkK"
        });
        assert_eq!(
            updater_config_status(updater_block_ref(&cfg)),
            UpdaterConfigStatus::Configured
        );
    }

    #[test]
    fn updater_status_configured_with_multiple_endpoints() {
        // 多端点 fallback 也算配置了
        let cfg = json!({
            "endpoints": [
                "https://primary.com/updates.json",
                "https://backup.com/updates.json"
            ],
            "pubkey": "dW50cnVzdGVkIGNvbW1lbnQ6IHNvbWVwdWJrZXkK"
        });
        assert_eq!(
            updater_config_status(updater_block_ref(&cfg)),
            UpdaterConfigStatus::Configured
        );
    }

    #[test]
    fn updater_status_handles_extra_fields_gracefully() {
        // _comment / windows / dangerousInsecureTransportProtocol 等额外字段不应干扰判定
        let cfg = json!({
            "_comment": "just a comment",
            "endpoints": ["https://example.com/updates.json"],
            "pubkey": "somekey",
            "windows": {"installMode": "passive"},
            "dangerousInsecureTransportProtocol": true
        });
        assert_eq!(
            updater_config_status(updater_block_ref(&cfg)),
            UpdaterConfigStatus::Configured
        );
    }
}
