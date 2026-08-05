mod app_state;
mod db;
mod fsm;
mod ipc;
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

fn data_dir() -> PathBuf {
    PathBuf::from(r"D:\3_Code_Projects\DeepFlow\data")
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
    let data = data_dir();
    let _ = std::fs::create_dir_all(&data);
    let _ = std::fs::create_dir_all(data.join("logs"));
    let _ = std::fs::create_dir_all(data.join("models"));
    let _ = std::fs::create_dir_all(data.join("exports"));

    // bootstrap settings for log level
    let boot_debug = LocalLoggerOpen::peek_debug(&data);
    init_logging(&data, boot_debug);

    let state = AppState::new(data).expect("failed to init AppState");
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
                                tracing::warn!("emergency double-esc");
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
            ipc::acknowledge_level2,
            ipc::submit_l3_reason,
            ipc::choose_session_end,
            ipc::get_settings,
            ipc::save_settings,
            ipc::get_today_focus_secs,
            ipc::get_weekly_report,
            ipc::list_running_processes,
            ipc::get_available_cameras,
            ipc::get_vision_status,
            ipc::restart_vision,
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
