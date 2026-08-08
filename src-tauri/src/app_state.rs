use crate::db::{LocalLogger, SettingsRecord, WeeklyReport};
use crate::fsm::{FsmEvent, FsmSideEffect, SystemFSM, SystemState};
use crate::ipc::events;
use crate::vision::{VisionEvent, VisionPipeline};
use crate::win32::ProcessGuard;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tracing::{debug, info, warn};

/// 应用全局状态：FSM + DB + 白名单 + 专注剩余快照（用于 pause/干预恢复）。
pub struct AppState {
    pub fsm: Arc<SystemFSM>,
    pub logger: Arc<Mutex<LocalLogger>>,
    pub data_dir: PathBuf,
    pub process_guard: Arc<Mutex<ProcessGuard>>,
    pub vision: Arc<VisionPipeline>,
    /// Focus 进入 pause/干预前的 remaining，恢复时加回。
    focus_remaining_snapshot: Arc<Mutex<u32>>,
    today_focus_secs: Arc<Mutex<u32>>,
    settings_cache: Arc<Mutex<SettingsRecord>>,
    whitelist_monitor_on: Arc<Mutex<bool>>,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Result<Self, String> {
        let logger = LocalLogger::open(&data_dir).map_err(|e| e.to_string())?;
        let settings = logger.load_settings().unwrap_or_default();
        let today = logger.today_focus_secs().unwrap_or(0);

        let fsm = Arc::new(SystemFSM::new());
        fsm.set_debt_floor_secs(settings.debt_floor_secs);
        fsm.set_pending_debt_secs(settings.pending_debt_secs);
        fsm.set_test_mode(settings.test_mode);

        let names: Vec<String> =
            serde_json::from_str(&settings.whitelist_json).unwrap_or_default();

        let models_dir = data_dir.join("models");
        let vision = VisionPipeline::new(models_dir, settings.prefer_cpu_inference);
        vision.set_enabled(settings.vision_enabled);
        vision.set_debug(settings.debug_mode);
        vision.set_test_mode(settings.test_mode);
        vision.set_roi_json(&settings.roi_json);

        Ok(Self {
            fsm,
            logger: Arc::new(Mutex::new(logger)),
            data_dir,
            process_guard: Arc::new(Mutex::new(ProcessGuard::new(names))),
            vision: Arc::new(vision),
            focus_remaining_snapshot: Arc::new(Mutex::new(0)),
            today_focus_secs: Arc::new(Mutex::new(today)),
            settings_cache: Arc::new(Mutex::new(settings)),
            whitelist_monitor_on: Arc::new(Mutex::new(false)),
        })
    }

    pub fn default_focus_mins(&self) -> u32 {
        self.settings_cache.lock().default_focus_mins
    }

    pub fn today_focus_secs(&self) -> u32 {
        *self.today_focus_secs.lock()
    }

    /// #28：清空后重置内存中的今日累计。
    pub fn reset_today_focus_secs(&self) {
        *self.today_focus_secs.lock() = 0;
    }

    pub fn load_settings(&self) -> Result<SettingsRecord, String> {
        Ok(self.settings_cache.lock().clone())
    }

    pub fn save_settings(&self, app: &AppHandle, s: SettingsRecord) -> Result<(), String> {
        self.fsm.set_debt_floor_secs(s.debt_floor_secs);
        self.fsm.set_test_mode(s.test_mode);
        let names: Vec<String> = serde_json::from_str(&s.whitelist_json).unwrap_or_default();
        self.process_guard.lock().set_whitelist(names);
        self.vision.set_enabled(s.vision_enabled);
        self.vision.set_debug(s.debug_mode);
        self.vision.set_test_mode(s.test_mode);
        self.vision.set_roi_json(&s.roi_json);
        self.vision.reload_detector(s.prefer_cpu_inference);
        // P2：紧急键热更新（不重装 WH_KEYBOARD_LL）
        crate::win32::set_emergency_hotkey(&s.emergency_hotkey);
        self.logger
            .lock()
            .save_settings(&s)
            .map_err(|e| e.to_string())?;
        *self.settings_cache.lock() = s.clone();
        if s.debug_mode {
            info!(target: "deepflow", "debug mode enabled");
        }
        let _ = app.emit(events::EVT_DEBUG_LOG, "settings_saved");
        Ok(())
    }

    pub fn export_weekly_png(&self) -> Result<String, String> {
        let report = self.weekly_report()?;
        let exports = self.data_dir.join("exports");
        let path = crate::report::export_weekly_png(&report, &exports)?;
        Ok(path.to_string_lossy().into_owned())
    }

    pub fn handle_vision_event(&self, app: &AppHandle, ev: VisionEvent) {
        match ev {
            VisionEvent::HoldSecs(hold) => {
                let _ = self.dispatch_and_apply(
                    app,
                    FsmEvent::VisionPhoneDetectedUpdate { hold_secs: hold },
                );
            }
            VisionEvent::CameraBlocked => {
                warn!(target: "deepflow", "camera blocked/covered");
                let _ = self.dispatch_and_apply(app, FsmEvent::CameraBlockedOrCovered);
            }
            VisionEvent::DetectionDebug(d) => {
                let _ = app.emit(events::EVT_DEBUG_LOG, &d);
            }
        }
    }

    fn start_vision_from_settings(&self) {
        let s = self.settings_cache.lock().clone();
        if !s.vision_enabled {
            info!(target: "deepflow", "vision_enabled=false");
            return;
        }
        let device = if s.camera_name.is_empty() {
            "0".to_string()
        } else {
            s.camera_name.clone()
        };
        if let Err(e) = self.vision.start(&device) {
            warn!(target: "deepflow", "vision start failed: {e}");
        }
    }

    pub fn weekly_report(&self) -> Result<WeeklyReport, String> {
        self.logger
            .lock()
            .generate_weekly_report()
            .map_err(|e| e.to_string())
    }

    pub fn dispatch_and_apply(&self, app: &AppHandle, event: FsmEvent) -> Result<(), String> {
        let prev = self.fsm.get_state();
        let was_focus = matches!(prev, SystemState::FocusActive { .. });
        let was_pause = matches!(prev, SystemState::TemporaryPause { .. });
        let was_intervention = prev.is_intervening();

        if let SystemState::FocusActive { remaining_secs, .. } = &prev {
            if matches!(
                event,
                FsmEvent::UserRequestPause { .. }
                    | FsmEvent::CameraBlockedOrCovered
                    | FsmEvent::VisionPhoneDetectedUpdate { .. }
                    | FsmEvent::TestInjectLevel { .. }
            ) {
                *self.focus_remaining_snapshot.lock() = *remaining_secs;
            }
        }

        let pause_elapsed = if let SystemState::TemporaryPause { elapsed_secs, .. } = &prev {
            Some(*elapsed_secs)
        } else {
            None
        };

        let (changed, effects) = self.fsm.dispatch(event);
        if !changed && effects.is_empty() {
            return Ok(());
        }

        // Resume / Skip：remaining = snapshot + max(elapsed, floor)
        if was_pause {
            if let SystemState::FocusActive {
                session_id,
                debt_secs_owed,
                ..
            } = self.fsm.get_state()
            {
                let floor = self.fsm.debt_floor_secs();
                let elapsed = pause_elapsed.unwrap_or(0);
                let debt = elapsed.max(floor);
                let fixed = self
                    .focus_remaining_snapshot
                    .lock()
                    .saturating_add(debt);
                self.fsm
                    .replace_focus_active(fixed, session_id, debt_secs_owed.max(debt));
                *self.focus_remaining_snapshot.lock() = fixed;

                // 清 pending debt
                let mut s = self.settings_cache.lock().clone();
                s.pending_debt_secs = 0;
                let _ = self.logger.lock().save_settings(&s);
                *self.settings_cache.lock() = s;
            }
        }

        // 干预拉回：恢复 snapshot
        if was_intervention {
            if let SystemState::FocusActive {
                remaining_secs: 0,
                session_id,
                debt_secs_owed,
            } = self.fsm.get_state()
            {
                let snap = *self.focus_remaining_snapshot.lock();
                if snap > 0 {
                    self.fsm
                        .replace_focus_active(snap, session_id, debt_secs_owed);
                }
            }
        }

        let _ = was_focus;
        self.apply_effects(app, effects);
        let st = self.fsm.get_state();
        let _ = app.emit(events::EVT_FSM_STATE_CHANGE, &st);
        if matches!(st, SystemState::AwaitSessionEndChoice { .. }) {
            let _ = app.emit(events::EVT_SESSION_END_CHOICE, &st);
        }
        if self.settings_cache.lock().debug_mode {
            debug!(target: "deepflow", "state={st:?}");
        }
        Ok(())
    }

    fn apply_effects(&self, app: &AppHandle, effects: Vec<FsmSideEffect>) {
        let session = self
            .fsm
            .get_state()
            .session_id()
            .unwrap_or("none")
            .to_string();

        for eff in effects {
            match eff {
                FsmSideEffect::Log {
                    event_type,
                    reason,
                    duration_secs,
                } => {
                    if let Err(e) = self.logger.lock().log_event(
                        &session,
                        &event_type,
                        reason.as_deref(),
                        duration_secs,
                    ) {
                        warn!("log_event failed: {e}");
                    }
                    if event_type == "PAUSE_END" || event_type == "SKIP_DEBT" {
                        // 持久化 pending debt = 0 after applied; if emergency later set pending
                    }
                    if self.settings_cache.lock().debug_mode {
                        let _ = app.emit(
                            events::EVT_DEBUG_LOG,
                            format!("{event_type} {reason:?} {duration_secs}"),
                        );
                    }
                }
                FsmSideEffect::ShowOverlay => {
                    let app2 = app.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = crate::ipc::open_overlay_window(app2.clone()).await {
                            warn!(target: "deepflow", "ShowOverlay failed: {e}");
                            // 二次尝试：先藏再开，避免坏掉的 webview 句柄
                            crate::ipc::force_hide_overlay(&app2);
                            let _ = crate::ipc::open_overlay_window(app2).await;
                        }
                    });
                }
                FsmSideEffect::HideOverlay => {
                    // 优先 hide 保活 webview，避免 close 后 invoke 报
                    // "failed to acquire webview reference" 且窗体残留卡死
                    crate::ipc::force_hide_overlay(app);
                }
                FsmSideEffect::ShowFloatingClock => {
                    let app2 = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = crate::ipc::open_floating_clock(app2).await;
                    });
                }
                FsmSideEffect::HideFloatingClock => {
                    crate::ipc::force_hide_floating(app);
                }
                FsmSideEffect::StartWhitelistMonitor => {
                    *self.whitelist_monitor_on.lock() = true;
                }
                FsmSideEffect::StopWhitelistMonitor => {
                    *self.whitelist_monitor_on.lock() = false;
                }
                FsmSideEffect::StartVision => {
                    self.start_vision_from_settings();
                }
                FsmSideEffect::StopVision => {
                    self.vision.stop();
                }
                FsmSideEffect::PlayChime => {
                    let _ = app.emit(events::EVT_PLAY_SOUND, "chime");
                    let _ = app.emit(events::EVT_DEBUG_LOG, "play_chime");
                }
                FsmSideEffect::SevereEscalate => {
                    let _ = app.emit(events::EVT_PLAY_SOUND, "severe");
                    let _ = app.emit(events::EVT_DEBUG_LOG, "severe_escalate");
                }
                FsmSideEffect::PlaySound { kind } => {
                    let _ = app.emit(events::EVT_PLAY_SOUND, &kind);
                    if self.settings_cache.lock().debug_mode {
                        let _ = app.emit(events::EVT_DEBUG_LOG, format!("play_sound:{kind}"));
                    }
                }
            }
        }
    }

    pub fn on_focus_tick(&self, app: &AppHandle) {
        if !self.fsm.get_state().is_focus_ticking() {
            return;
        }
        // snapshot remaining each tick for crash safety lightly
        if let SystemState::FocusActive { remaining_secs, .. } = self.fsm.get_state() {
            *self.focus_remaining_snapshot.lock() = remaining_secs;
        }
        let _ = self.dispatch_and_apply(app, FsmEvent::SessionTimerTick);
        {
            let mut t = self.today_focus_secs.lock();
            *t = t.saturating_add(1);
            let _ = self.logger.lock().add_focus_secs_today(1);
            let _ = app.emit(events::EVT_TODAY_FOCUS, *t);
        }
    }

    pub fn on_pause_tick(&self, app: &AppHandle) {
        if self.fsm.get_state().is_pausing() {
            let _ = self.dispatch_and_apply(app, FsmEvent::PauseTimerTick);
        }
    }

    pub fn on_intervention_tick(&self, app: &AppHandle) {
        if self.fsm.get_state().is_intervening() {
            let _ = self.dispatch_and_apply(app, FsmEvent::InterventionTick);
        }
    }

    pub fn whitelist_enabled(&self) -> bool {
        *self.whitelist_monitor_on.lock()
    }

    pub fn scan_and_emit_whitelist(&self, app: &AppHandle) {
        if !self.whitelist_enabled() {
            return;
        }
        let hits = self.process_guard.lock().scan_violations();
        if hits.is_empty() {
            return;
        }
        // 按进程名去重，避免同一浏览器几十个 pid 刷爆日志/UI
        let mut seen = std::collections::BTreeSet::new();
        let mut compact: Vec<_> = Vec::new();
        for h in hits {
            if seen.insert(h.process_name.clone()) {
                compact.push(h);
            }
        }

        // #22：根据设置对违规进程执行最小化/礼貌关闭（不杀进程）
        let action = self
            .settings_cache
            .lock()
            .whitelist_action
            .to_lowercase();
        if action == "minimize" || action == "close_report" {
            let guard = self.process_guard.lock();
            for h in &compact {
                let n = if action == "close_report" {
                    guard.close_windows_of(h.pid)
                } else {
                    guard.minimize_windows_of(h.pid)
                };
                if n > 0 && self.settings_cache.lock().debug_mode {
                    debug!(
                        target: "deepflow",
                        "whitelist_action={} {} pid={} windows={}",
                        action, h.process_name, h.pid, n
                    );
                }
            }
        }

        let _ = app.emit(events::EVT_WHITELIST_HIT, &compact);
        if self.settings_cache.lock().debug_mode {
            for h in &compact {
                debug!(target: "deepflow", "whitelist hit {} pid={}", h.process_name, h.pid);
            }
        }
    }

    /// 无论 FSM 是否已 Idle：停视觉/白名单并强制藏遮罩（防卡死逃生）。
    pub fn force_exit_everything(&self, app: &AppHandle) -> SystemState {
        let st = self.fsm.get_state();
        if !matches!(st, SystemState::Idle) {
            let _ = self.dispatch_and_apply(app, FsmEvent::TestExit);
        } else {
            // 已 Idle 仍可能残留全屏 overlay
            crate::ipc::force_hide_overlay(app);
            crate::ipc::force_hide_floating(app);
            self.vision.stop();
            *self.whitelist_monitor_on.lock() = false;
        }
        // 再保险关一次窗
        crate::ipc::force_hide_overlay(app);
        crate::ipc::force_hide_floating(app);
        self.fsm.get_state()
    }

    pub fn persist_pending_debt_if_any(&self) {
        // 若处于 pause，把已产生债务写入 settings.pending_debt_secs
        if let SystemState::TemporaryPause { elapsed_secs, .. } = self.fsm.get_state() {
            let floor = self.fsm.debt_floor_secs();
            let debt = elapsed_secs.max(floor);
            let mut s = self.settings_cache.lock().clone();
            s.pending_debt_secs = debt;
            let _ = self.logger.lock().save_settings(&s);
            *self.settings_cache.lock() = s;
        }
    }
}
