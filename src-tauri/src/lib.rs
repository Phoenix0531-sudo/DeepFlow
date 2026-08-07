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
fn seed_models_if_needed(data: &PathBuf) {
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
    if has_onnx {
        return;
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
                        Ok(_) => tracing::info!(
                            target: "deepflow",
                            "seeded model {:?} -> {:?}",
                            p,
                            target
                        ),
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
                let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&show_i, &settings_i, &report_i, &quit_i])?;

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
