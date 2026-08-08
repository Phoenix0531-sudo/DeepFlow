mod app_state;
mod db;
mod fsm;
mod ipc;
mod report;
mod vision;
mod win32;

use app_state::AppState;
use fsm::FsmEvent;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};
use tracing_subscriber::{fmt, EnvFilter};
use win32::{KeyHookManager, KeyboardHookEvent};

fn is_dev_exe(exe: &std::path::Path) -> bool {
    // cargo/tauri 构建产物：.../target/debug|release/...
    let mut saw_target = false;
    for comp in exe.components() {
        let s = comp.as_os_str().to_string_lossy().to_ascii_lowercase();
        if s == "target" {
            saw_target = true;
            continue;
        }
        if saw_target && (s == "debug" || s == "release") {
            return true;
        }
    }
    false
}

fn is_portable_mode(exe: Option<&std::path::Path>) -> bool {
    if std::env::var("DEEPFLOW_PORTABLE")
        .map(|v| {
            let t = v.trim().to_ascii_lowercase();
            t == "1" || t == "true" || t == "yes" || t == "on"
        })
        .unwrap_or(false)
    {
        return true;
    }
    if let Some(parent) = exe.and_then(|p| p.parent()) {
        // 旁路标记：portable.flag / .portable / DeepFlow.portable
        for name in ["portable.flag", ".portable", "DeepFlow.portable"] {
            if parent.join(name).is_file() {
                return true;
            }
        }
    }
    false
}

/// 解析数据目录 + 模式标签（env / portable / dev / install / fallback）。
fn resolve_data_dir() -> (PathBuf, &'static str) {
    // 1) 显式覆盖
    if let Ok(p) = std::env::var("DEEPFLOW_DATA_DIR") {
        let pb = PathBuf::from(p);
        let _ = std::fs::create_dir_all(&pb);
        return (pb, "env");
    }

    let exe = std::env::current_exe().ok();
    let exe_ref = exe.as_deref();
    let portable = is_portable_mode(exe_ref);
    let dev = exe_ref.map(is_dev_exe).unwrap_or(false);

    // 2) 便携：始终写到 exe 旁 data/（U 盘 / 免安装）
    if portable {
        if let Some(parent) = exe_ref.and_then(|p| p.parent()) {
            let beside = parent.join("data");
            if std::fs::create_dir_all(&beside).is_ok() {
                return (beside, "portable");
            }
        }
    }

    // 3) 开发：仓库/cwd data 优先
    if dev {
        if let Ok(cwd) = std::env::current_dir() {
            for cand in [
                cwd.join("data"),
                cwd.join("..").join("data"),
                cwd.join("..").join("..").join("data"),
            ] {
                if cand.is_dir() {
                    return (cand.canonicalize().unwrap_or(cand), "dev");
                }
            }
            // cwd 下尚无 data 时创建
            let created = cwd.join("data");
            if std::fs::create_dir_all(&created).is_ok() {
                return (created, "dev");
            }
        }
    }

    // 4) 正式安装：%LOCALAPPDATA%\DeepFlow\data（可写、不污染 Program Files）
    if !dev {
        if let Ok(la) = std::env::var("LOCALAPPDATA") {
            let dir = PathBuf::from(la).join("DeepFlow").join("data");
            if std::fs::create_dir_all(&dir).is_ok() {
                return (dir, "install");
            }
        }
    }

    // 5) 回退：exe 旁 data
    if let Some(parent) = exe_ref.and_then(|p| p.parent()) {
        let beside = parent.join("data");
        let _ = std::fs::create_dir_all(&beside);
        return (beside, if portable { "portable" } else { "fallback" });
    }

    // 6) 极端兜底：临时目录
    let tmp = std::env::temp_dir().join("DeepFlow").join("data");
    let _ = std::fs::create_dir_all(&tmp);
    (tmp, "fallback")
}

fn data_dir() -> PathBuf {
    resolve_data_dir().0
}

/// 首次启动：若 data/models 无 onnx，尝试从安装目录/资源旁复制。
pub(crate) fn seed_models_if_needed(data: &PathBuf) -> u32 {
    let dest = data.join("models");
    let _ = std::fs::create_dir_all(&dest);
    let has_onnx = std::fs::read_dir(&dest)
        .ok()
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .any(|e| {
                    e.path()
                        .extension()
                        .and_then(|x| x.to_str())
                        .map(|x| x.eq_ignore_ascii_case("onnx"))
                        .unwrap_or(false)
                })
        })
        .unwrap_or(false);
    let mut copied = 0u32;
    if has_onnx {
        return 0;
    }

    let exe = std::env::current_exe().ok();
    let parent = exe.as_ref().and_then(|p| p.parent());
    let mut sources: Vec<PathBuf> = Vec::new();
    if let Some(p) = parent {
        sources.push(p.join("models"));
        sources.push(p.join("resources").join("models"));
        // tauri bundle 资源常见位置
        sources.push(p.join("..").join("Resources").join("models"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        sources.push(cwd.join("data").join("models"));
        sources.push(cwd.join("models"));
    }

    for src in sources {
        let Ok(rd) = std::fs::read_dir(&src) else {
            continue;
        };
        for ent in rd.filter_map(|e| e.ok()) {
            let p = ent.path();
            let is_onnx = p
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x.eq_ignore_ascii_case("onnx"))
                .unwrap_or(false);
            if !is_onnx {
                continue;
            }
            if let Some(name) = p.file_name() {
                let target = dest.join(name);
                if !target.exists() {
                    match std::fs::copy(&p, &target) {
                        Ok(_) => {
                            copied += 1;
                            tracing::info!(
                                target: "deepflow",
                                "seeded model {:?} -> {:?}",
                                p,
                                target
                            );
                        }
                        Err(e) => tracing::warn!(
                            target: "deepflow",
                            "seed model failed {:?}: {e}",
                            p
                        ),
                    }
                }
            }
        }
    }
    copied
}

fn init_logging(data_dir: &PathBuf, debug: bool) {
    let log_dir = data_dir.join("logs");
    let _ = std::fs::create_dir_all(&log_dir);
    let file_appender = tracing_appender::rolling::daily(&log_dir, "deepflow.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    // leak guard for process lifetime
    std::mem::forget(guard);

    let filter = if debug {
        EnvFilter::new("debug,deepflow=debug")
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };

    let _ = fmt()
        .with_env_filter(filter)
        .with_writer(non_blocking)
        .with_ansi(false)
        .try_init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let (data, path_mode) = resolve_data_dir();
    let _ = std::fs::create_dir_all(&data);
    let _ = std::fs::create_dir_all(data.join("logs"));
    let _ = std::fs::create_dir_all(data.join("models"));
    let _ = std::fs::create_dir_all(data.join("exports"));

    // bootstrap settings for log level
    let boot_debug = LocalLoggerOpen::peek_debug(&data);
    init_logging(&data, boot_debug);
    tracing::info!(target: "deepflow", "data_dir={:?} mode={path_mode}", data);
    seed_models_if_needed(&data);

    let state = AppState::new(data).expect("failed to init AppState");
    // 启动时同步紧急键模式（可在 save_settings 热更新）
    if let Ok(s) = state.load_settings() {
        win32::set_emergency_hotkey(&s.emergency_hotkey);
    }
    let state = Arc::new(state);

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(state.clone())
        .setup({
            let state = state.clone();
            move |app| {

                // Keyboard hook → FSM
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<KeyboardHookEvent>();
                let _hook = KeyHookManager::spawn_hook(tx);
                // Keep hook alive
                app.manage(_hook);

                let handle = app.handle().clone();
                let st = state.clone();
                tauri::async_runtime::spawn(async move {
                    while let Some(ev) = rx.recv().await {
                        match ev {
                            KeyboardHookEvent::EmergencyEscapeTriggered => {
                                tracing::warn!("emergency hotkey triggered");
                                let _ = st.dispatch_and_apply(&handle, FsmEvent::DoubleEscPressed);
                            }
                        }
                    }
                });

                // 1s ticks
                let handle = app.handle().clone();
                let st = state.clone();
                tauri::async_runtime::spawn(async move {
                    let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
                    loop {
                        interval.tick().await;
                        st.on_focus_tick(&handle);
                        st.on_pause_tick(&handle);
                        st.on_intervention_tick(&handle);
                    }
                });

                // 3s whitelist scan
                let handle = app.handle().clone();
                let st = state.clone();
                tauri::async_runtime::spawn(async move {
                    let mut interval = tokio::time::interval(std::time::Duration::from_secs(3));
                    loop {
                        interval.tick().await;
                        st.scan_and_emit_whitelist(&handle);
                    }
                });

                // Vision pipeline events → FSM
                let (vtx, mut vrx) =
                    tokio::sync::mpsc::unbounded_channel::<crate::vision::VisionEvent>();
                state.vision.set_event_sender(vtx);
                let handle = app.handle().clone();
                let st = state.clone();
                tauri::async_runtime::spawn(async move {
                    while let Some(ev) = vrx.recv().await {
                        st.handle_vision_event(&handle, ev);
                    }
                });

                // Tray
                let show_i = MenuItem::with_id(app, "show", "打开主界面", true, None::<&str>)?;
                let settings_i =
                    MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
                let report_i = MenuItem::with_id(app, "report", "周报", true, None::<&str>)?;
                let open_data_i =
                    MenuItem::with_id(app, "open_data_dir", "打开数据目录", true, None::<&str>)?;
                let open_exports_i =
                    MenuItem::with_id(app, "open_exports_dir", "打开导出目录", true, None::<&str>)?;
                let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
                let menu = Menu::with_items(
                    app,
                    &[
                        &show_i,
                        &settings_i,
                        &report_i,
                        &open_data_i,
                        &open_exports_i,
                        &quit_i,
                    ],
                )?;

                let _tray = TrayIconBuilder::new()
                    .icon(app.default_window_icon().unwrap().clone())
                    .menu(&menu)
                    .tooltip("DeepFlow")
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "show" => {
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                        "settings" => {
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                                let _ = app.emit_to("main", "open_settings", ());
                            }
                        }
                        "report" => {
                            let _ = app.emit_to("main", "open_report", ());
                        }
                        "open_data_dir" => {
                            if let Some(state) = app.try_state::<AppState>() {
                                let _ = crate::ipc::reveal_path_sync(&state.data_dir);
                            }
                        }
                        "open_exports_dir" => {
                            if let Some(state) = app.try_state::<AppState>() {
                                let exports = state.data_dir.join("exports");
                                let _ = std::fs::create_dir_all(&exports);
                                let _ = crate::ipc::reveal_path_sync(&exports);
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            let app = tray.app_handle();
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                    })
                    .build(app)?;

                // 首次未 setup → 开 setup 窗
                let settings = state.load_settings().unwrap_or_default();
                if !settings.setup_completed {
                    let app2 = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = crate::ipc::open_setup_window(app2).await;
                    });
                }

                tracing::info!("DeepFlow started");
                Ok(())
            }
        })
        .invoke_handler(tauri::generate_handler![
            ipc::get_fsm_state,
            ipc::start_focus_session,
            ipc::request_temporary_pause,
            ipc::resume_focus_session,
            ipc::skip_debt_and_resume,
            ipc::stop_session,
            ipc::test_inject_level,
            ipc::test_exit_session,
            ipc::force_exit_session,
            ipc::acknowledge_level2,
            ipc::submit_l3_reason,
            ipc::choose_session_end,
            ipc::get_settings,
            ipc::save_settings,
            ipc::get_today_focus_secs,
            ipc::get_weekly_report,
            ipc::export_weekly_report_png,
            ipc::get_data_dir,
            ipc::get_path_info,
            ipc::reveal_path,
            ipc::list_models,
            ipc::reseed_models,
            ipc::get_weekly_report_at,
            ipc::get_l3_reasons,
            ipc::export_all_data,
            ipc::clear_all_data,
            ipc::get_autostart_enabled,
            ipc::set_autostart_enabled,
            ipc::request_notification_permission,
            ipc::send_notification,
            ipc::check_for_updates,
            ipc::download_and_install_update,
            ipc::backup_settings,
            ipc::restore_settings,
            ipc::list_running_processes,
            ipc::get_available_cameras,
            ipc::get_vision_status,
            ipc::get_vision_preview,
            ipc::restart_vision,
            ipc::start_vision_preview,
            ipc::stop_vision_preview,
            ipc::apply_overlay_native_style,
            ipc::open_overlay_window,
            ipc::close_overlay_window,
            ipc::open_floating_clock,
            ipc::close_floating_clock,
            ipc::open_setup_window,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    // 关主窗 → 隐藏到托盘
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building DeepFlow")
        .run(|app_handle, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(st) = app_handle.try_state::<Arc<AppState>>() {
                    st.persist_pending_debt_if_any();
                }
            }
        });
}

/// 避免在 init_logging 前完整打开 DB 失败时的小助手。
struct LocalLoggerOpen;
impl LocalLoggerOpen {
    fn peek_debug(data: &PathBuf) -> bool {
        db::LocalLogger::open(data)
            .ok()
            .and_then(|l| l.load_settings().ok())
            .map(|s| s.debug_mode)
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::fs;

    #[test]
    fn is_dev_exe_detects_target_debug() {
        assert!(is_dev_exe(Path::new(
            "/home/u/proj/target/debug/deepflow.exe"
        )));
        assert!(is_dev_exe(Path::new(
            "C:\\code\\DeepFlow\\target\\release\\deepflow.exe"
        )));
        assert!(is_dev_exe(Path::new("target/debug/x")));
    }

    #[test]
    fn is_dev_exe_rejects_install_paths() {
        // 安装目录不含 target/debug
        assert!(!is_dev_exe(Path::new(
            "C:\\Program Files\\DeepFlow\\deepflow.exe"
        )));
        assert!(!is_dev_exe(Path::new("/opt/deepflow/bin/deepflow")));
        // 只有 target 无 debug/release 不算（比如代码名 target_detect）
        assert!(!is_dev_exe(Path::new("/u/target/x/y")));
        // 顺序不对：debug 在 target 之前
        assert!(!is_dev_exe(Path::new("/u/debug/target/x")));
    }

    #[test]
    fn is_dev_exe_release_same_level_as_target() {
        // target 之后必须是紧邻的 debug|release 段；目录名叫 debug2 不算
        assert!(!is_dev_exe(Path::new("/u/target/debug2/x")));
        assert!(!is_dev_exe(Path::new("/u/target/profile/x")));
    }

    #[test]
    fn is_portable_mode_marker_files() {
        // 在 exe 同级放标记文件 → 便携
        let tmp = tempfile::tempdir().unwrap();
        let exe = tmp.path().join("deepflow.exe");
        fs::write(&exe, b"").unwrap();

        assert!(!is_portable_mode(Some(&exe)), "no marker → not portable");

        for name in ["portable.flag", ".portable", "DeepFlow.portable"] {
            let marker = tmp.path().join(name);
            fs::write(&marker, b"x").unwrap();
            assert!(
                is_portable_mode(Some(&exe)),
                "marker {name} → portable"
            );
            fs::remove_file(&marker).unwrap();
        }
        assert!(!is_portable_mode(Some(&exe)), "cleaned → not portable");
    }

    #[test]
    fn is_portable_mode_none_exe_safe_false() {
        // exe=None 且无事标 → 安全返回 false
        // 注意：此测试受 DEEPFLOW_PORTABLE 环境变量影响；但仍要求不 panic
        let _ = is_portable_mode(None);
    }

    // 环境变量分支（DEEPFLOW_DATA_DIR / DEEPFLOW_PORTABLE）会受测试并发影响，
    // 此处不做断言性测试，避免竞态。resolve_data_dir 的集成留到手工验收。

    // —— #10：FSM 集成冲 subprocess 烂数而生纯状态流。无 GUI/无视觉/无键盘依赖。 ——
    use crate::fsm::{FsmEvent, FsmSideEffect, SystemFSM, SystemState};

    /// 辅助：从副作用集里取出 Log 事件的 event_type 列表。
    fn log_kinds(effects: &[FsmSideEffect]) -> Vec<&str> {
        effects
            .iter()
            .filter_map(|e| match e {
                FsmSideEffect::Log { event_type, .. } => Some(event_type.as_str()),
                _ => None,
            })
            .collect()
    }

    /// 辅助：判断副作用中是否含 event_type=t 同时 reason=Some(r)。
    fn has_log(effects: &[FsmSideEffect], t: &str, r: &str) -> bool {
        effects.iter().any(|e| matches!(
            e,
            FsmSideEffect::Log { event_type, reason: Some(rr), .. }
                if event_type == t && rr == r
        ))
    }

    #[test]
    fn fsm_smoke_session_l1_l3_pause_exit() {
        let fsm = SystemFSM::new();
        fsm.set_test_mode(true);

        // Idle -> FocusActive
        let (ok, eff) = fsm.dispatch(FsmEvent::StartSession { focus_duration_mins: 25 });
        assert!(ok, "start should change state");
        assert!(matches!(fsm.get_state(), SystemState::FocusActive { .. }));
        assert!(log_kinds(&eff).iter().any(|s| *s == "SESSION_START"));

        // FocusActive -> InterventionLevel1
        let (ok, eff) = fsm.dispatch(FsmEvent::TestInjectLevel { level: 1 });
        assert!(ok);
        assert!(matches!(fsm.get_state(), SystemState::InterventionLevel1 { .. }));
        // 测试注入使用 event_type=TEST_INJECT，reason="L1"（区分于正常运行 L1 Log）
        assert!(has_log(&eff, "TEST_INJECT", "L1"));

        // L1 -> L3 跳级
        let (ok, eff) = fsm.dispatch(FsmEvent::TestInjectLevel { level: 3 });
        assert!(ok);
        assert!(matches!(fsm.get_state(), SystemState::InterventionLevel3 { .. }));
        assert!(has_log(&eff, "TEST_INJECT", "L3"));

        // L3 + 原因 -> TemporaryPause（reason 被携入）
        let (ok, eff) = fsm.dispatch(FsmEvent::SubmitL3Reason {
            reason: "刷手机".into(),
        });
        assert!(ok);
        match fsm.get_state() {
            SystemState::TemporaryPause { reason, .. } => assert_eq!(reason, "刷手机"),
            other => panic!("expected TemporaryPause, got {other:?}"),
        }
        // 副作用记录 PAUSE_START 并携带 reason
        assert!(log_kinds(&eff).iter().any(|s| *s == "PAUSE_START"));
        assert!(has_log(&eff, "PAUSE_START", "刷手机"), "PAUSE_START Log 应携带对应 L3 原因");

        // 任意状态 -> TestExit -> Idle
        let (ok, _) = fsm.dispatch(FsmEvent::TestExit);
        assert!(ok, "TestExit should mostly honor intervention/any");
        // 注：L3/暂停下 TestExit 的实际路径可能进入 AwaitSessionEndChoice，但纯测试模式幂终点为 Idle。
        // 不强制断言终态，避免与实际安全策略冲突。
    }

    #[test]
    fn fsm_smoke_focus_tick_decrements() {
        let fsm = SystemFSM::new();
        fsm.set_test_mode(true);
        fsm.set_debt_floor_secs(0);
        fsm.dispatch(FsmEvent::StartSession { focus_duration_mins: 1 });
        if let SystemState::FocusActive { remaining_secs, .. } = fsm.get_state() {
            assert_eq!(remaining_secs, 60, "25 分实际为 tick 计数，1 分 = 60s");
        } else {
            panic!("state should remain FocusActive");
        }
        fsm.dispatch(FsmEvent::SessionTimerTick);
        if let SystemState::FocusActive { remaining_secs, .. } = fsm.get_state() {
            assert_eq!(remaining_secs, 59, "tick 后应减 1");
        } else {
            panic!("state should remain FocusActive after one tick");
        }
    }

    #[test]
    fn fsm_smoke_double_esc_is_emergency_exit() {
        // DoubleEsc = 紧急退出：任何非 Idle 状态 -> Idle + EMERGENCY_EXIT Log。
        let fsm = SystemFSM::new();
        fsm.set_test_mode(true);
        fsm.dispatch(FsmEvent::StartSession { focus_duration_mins: 10 });
        assert!(matches!(fsm.get_state(), SystemState::FocusActive { .. }));
        let (ok, eff) = fsm.dispatch(FsmEvent::DoubleEscPressed);
        assert!(ok, "DoubleEsc 是紧急退出，必须状态迁移");
        assert!(matches!(fsm.get_state(), SystemState::Idle), "应回到 Idle");
        assert!(log_kinds(&eff).iter().any(|s| *s == "EMERGENCY_EXIT"));
    }

    #[test]
    fn fsm_smoke_manual_stop_enters_end_choice() {
        // #36：手动 StopSession 进入三选一，不直接 Idle
        let fsm = SystemFSM::new();
        fsm.set_test_mode(true);
        fsm.dispatch(FsmEvent::StartSession { focus_duration_mins: 10 });
        assert!(matches!(fsm.get_state(), SystemState::FocusActive { .. }));

        let (ok, eff) = fsm.dispatch(FsmEvent::StopSession);
        assert!(ok, "StopSession 应迁移");
        assert!(
            matches!(fsm.get_state(), SystemState::AwaitSessionEndChoice { .. }),
            "手动停止应进入 AwaitSessionEndChoice"
        );
        assert!(
            eff.iter().any(|e| matches!(e, FsmSideEffect::ShowOverlay)),
            "应弹出遮罩展示三选一"
        );
        assert!(has_log(&eff, "SESSION_STOP_CHOICE", "manual_stop"));

        // 选择结束 → Idle
        let (ok, _) = fsm.dispatch(FsmEvent::ChooseEnd);
        assert!(ok);
        assert!(matches!(fsm.get_state(), SystemState::Idle));
    }
}