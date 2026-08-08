use super::state::{FsmEvent, FsmSideEffect, SystemState};
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;

/// 默认债务下限（秒）——定稿 U5：max(实际, floor)，默认 3 分钟。
pub const DEFAULT_DEBT_FLOOR_SECS: u32 = 180;

pub struct SystemFSM {
    current_state: Arc<RwLock<SystemState>>,
    event_bus: broadcast::Sender<SystemState>,
    /// 本 session 进入 L3 的次数（用于「第二次 L3 结束会话」）。
    l3_count_in_session: Arc<RwLock<u32>>,
    /// 未还债务（进程重启后由上层灌入）。
    pending_debt_secs: Arc<RwLock<u32>>,
    debt_floor_secs: Arc<RwLock<u32>>,
    test_mode: Arc<RwLock<bool>>,
    /// L1 观察期剩余秒（进入 L1 时置 30）。
    l1_observe_remaining: Arc<RwLock<u32>>,
}

impl Default for SystemFSM {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemFSM {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(64);
        Self {
            current_state: Arc::new(RwLock::new(SystemState::Idle)),
            event_bus: tx,
            l3_count_in_session: Arc::new(RwLock::new(0)),
            pending_debt_secs: Arc::new(RwLock::new(0)),
            debt_floor_secs: Arc::new(RwLock::new(DEFAULT_DEBT_FLOOR_SECS)),
            test_mode: Arc::new(RwLock::new(false)),
            l1_observe_remaining: Arc::new(RwLock::new(0)),
        }
    }

    pub fn get_state(&self) -> SystemState {
        self.current_state.read().clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SystemState> {
        self.event_bus.subscribe()
    }

    pub fn set_debt_floor_secs(&self, secs: u32) {
        *self.debt_floor_secs.write() = secs.max(0);
    }

    pub fn debt_floor_secs(&self) -> u32 {
        *self.debt_floor_secs.read()
    }

    pub fn set_test_mode(&self, enabled: bool) {
        *self.test_mode.write() = enabled;
    }

    pub fn test_mode(&self) -> bool {
        *self.test_mode.read()
    }

    /// 返回 (L1, L2, L3, 放下恢复) 阈值秒数。
    fn vision_thresholds(&self) -> (u32, u32, u32, u32) {
        if self.test_mode() {
            // 测试模式：可在约 10 秒内走完 L1→L2→L3
            (3, 6, 9, 1)
        } else {
            (60, 120, 180, 10)
        }
    }

    pub fn set_pending_debt_secs(&self, secs: u32) {
        *self.pending_debt_secs.write() = secs;
    }

    pub fn pending_debt_secs(&self) -> u32 {
        *self.pending_debt_secs.read()
    }

    /// 干预/暂停恢复时写回正确的 remaining。
    pub fn replace_focus_active(&self, remaining_secs: u32, session_id: String, debt_secs_owed: u32) {
        let next = SystemState::FocusActive {
            remaining_secs,
            session_id,
            debt_secs_owed,
        };
        *self.current_state.write() = next.clone();
        let _ = self.event_bus.send(next);
    }

    /// 派发事件，返回是否发生迁移以及副作用列表。
    pub fn dispatch(&self, event: FsmEvent) -> (bool, Vec<FsmSideEffect>) {
        let mut state = self.current_state.write();
        let (next, effects) = self.transition(&state, event);
        match next {
            Some(ns) => {
                *state = ns.clone();
                drop(state);
                let _ = self.event_bus.send(ns);
                (true, effects)
            }
            None => (false, effects),
        }
    }

    fn transition(
        &self,
        state: &SystemState,
        event: FsmEvent,
    ) -> (Option<SystemState>, Vec<FsmSideEffect>) {
        let floor = *self.debt_floor_secs.read();
        let (l1_threshold, l2_threshold, l3_threshold, release_threshold) =
            self.vision_thresholds();

        match (state, event) {
            // —— 启动 ——
            (SystemState::Idle, FsmEvent::StartSession { focus_duration_mins }) => {
                let debt = *self.pending_debt_secs.read();
                *self.pending_debt_secs.write() = 0;
                *self.l3_count_in_session.write() = 0;
                let session_id = Uuid::new_v4().to_string();
                let remaining = focus_duration_mins.saturating_mul(60).saturating_add(debt);
                (
                    Some(SystemState::FocusActive {
                        remaining_secs: remaining,
                        session_id: session_id.clone(),
                        debt_secs_owed: debt,
                    }),
                    vec![
                        FsmSideEffect::Log {
                            event_type: "SESSION_START".into(),
                            reason: None,
                            duration_secs: remaining,
                        },
                        FsmSideEffect::ShowOverlay,
                        FsmSideEffect::StartWhitelistMonitor,
                        FsmSideEffect::StartVision,
                    ],
                )
            }

            // —— 主动休息 ——
            (
                SystemState::FocusActive {
                    session_id,
                    remaining_secs,
                    ..
                },
                FsmEvent::UserRequestPause { reason },
            ) => (
                Some(SystemState::TemporaryPause {
                    elapsed_secs: 0,
                    reason: reason.clone(),
                    session_id: session_id.clone(),
                }),
                vec![
                    FsmSideEffect::Log {
                        event_type: "PAUSE_START".into(),
                        reason: Some(reason),
                        duration_secs: *remaining_secs,
                    },
                    FsmSideEffect::HideOverlay,
                    FsmSideEffect::ShowFloatingClock,
                    FsmSideEffect::StopWhitelistMonitor,
                    FsmSideEffect::StopVision,
                ],
            ),

            // —— 紧急退出：任意非 Idle → Idle；已 Idle 也强制关遮罩（防卡死）——
            (SystemState::Idle, FsmEvent::DoubleEscPressed) => (
                None,
                vec![
                    FsmSideEffect::HideOverlay,
                    FsmSideEffect::HideFloatingClock,
                    FsmSideEffect::StopWhitelistMonitor,
                    FsmSideEffect::StopVision,
                ],
            ),
            (s, FsmEvent::DoubleEscPressed) => {
                let sid = s.session_id().unwrap_or("").to_string();
                (
                    Some(SystemState::Idle),
                    vec![
                        FsmSideEffect::Log {
                            event_type: "EMERGENCY_EXIT".into(),
                            reason: Some(sid),
                            duration_secs: 0,
                        },
                        FsmSideEffect::HideOverlay,
                        FsmSideEffect::HideFloatingClock,
                        FsmSideEffect::StopWhitelistMonitor,
                        FsmSideEffect::StopVision,
                    ],
                )
            }

            // —— 恢复（含债务 max(实际, floor)）——
            (
                SystemState::TemporaryPause {
                    elapsed_secs,
                    reason,
                    session_id,
                },
                FsmEvent::ResumeFocus,
            ) => {
                let debt = (*elapsed_secs).max(floor);
                (
                    Some(SystemState::FocusActive {
                        remaining_secs: debt, // 上层应把 pause 前 remaining 一并加回；见 AppState 包装
                        session_id: session_id.clone(),
                        debt_secs_owed: debt,
                    }),
                    vec![
                        FsmSideEffect::Log {
                            event_type: "PAUSE_END".into(),
                            reason: Some(reason.clone()),
                            duration_secs: *elapsed_secs,
                        },
                        FsmSideEffect::Log {
                            event_type: "DEBT_APPLIED".into(),
                            reason: Some(format!("debt={debt}")),
                            duration_secs: debt,
                        },
                        FsmSideEffect::HideFloatingClock,
                        FsmSideEffect::ShowOverlay,
                        FsmSideEffect::StartWhitelistMonitor,
                        FsmSideEffect::StartVision,
                    ],
                )
            }

            // —— 跳过仍记账 + 仍加债（定稿）——
            (
                SystemState::TemporaryPause {
                    elapsed_secs,
                    session_id,
                    ..
                },
                FsmEvent::SkipDebtAndResume,
            ) => {
                let debt = (*elapsed_secs).max(floor);
                (
                    Some(SystemState::FocusActive {
                        remaining_secs: debt,
                        session_id: session_id.clone(),
                        debt_secs_owed: debt,
                    }),
                    vec![
                        FsmSideEffect::Log {
                            event_type: "SKIP_DEBT".into(),
                            reason: Some(format!("elapsed={elapsed_secs}")),
                            duration_secs: *elapsed_secs,
                        },
                        FsmSideEffect::Log {
                            event_type: "DEBT_APPLIED".into(),
                            reason: Some(format!("debt={debt}")),
                            duration_secs: debt,
                        },
                        FsmSideEffect::HideFloatingClock,
                        FsmSideEffect::ShowOverlay,
                        FsmSideEffect::StartWhitelistMonitor,
                        FsmSideEffect::StartVision,
                    ],
                )
            }

            // —— 专注倒计时 ——
            (
                SystemState::FocusActive {
                    remaining_secs,
                    session_id,
                    debt_secs_owed,
                },
                FsmEvent::SessionTimerTick,
            ) => {
                if *remaining_secs == 0 {
                    return (None, vec![]);
                }
                let next = remaining_secs.saturating_sub(1);
                if next == 0 {
                    (
                        Some(SystemState::AwaitSessionEndChoice {
                            session_id: session_id.clone(),
                        }),
                        vec![
                            FsmSideEffect::Log {
                                event_type: "SESSION_TIMER_DONE".into(),
                                reason: None,
                                duration_secs: 0,
                            },
                            FsmSideEffect::ShowOverlay,
                            FsmSideEffect::StopVision,
                            FsmSideEffect::StopWhitelistMonitor,
                        ],
                    )
                } else {
                    (
                        Some(SystemState::FocusActive {
                            remaining_secs: next,
                            session_id: session_id.clone(),
                            debt_secs_owed: *debt_secs_owed,
                        }),
                        vec![],
                    )
                }
            }

            // —— 休息计时 ——
            (
                SystemState::TemporaryPause {
                    elapsed_secs,
                    reason,
                    session_id,
                },
                FsmEvent::PauseTimerTick,
            ) => (
                Some(SystemState::TemporaryPause {
                    elapsed_secs: elapsed_secs.saturating_add(1),
                    reason: reason.clone(),
                    session_id: session_id.clone(),
                }),
                vec![],
            ),

            // —— 视觉持握 ——
            (
                SystemState::FocusActive { session_id, .. },
                FsmEvent::VisionPhoneDetectedUpdate { hold_secs },
            ) => {
                if hold_secs >= l1_threshold {
                    let obs = if self.test_mode() { 3 } else { 30 };
                    *self.l1_observe_remaining.write() = obs;
                    (
                        Some(SystemState::InterventionLevel1 {
                            phone_hold_duration_secs: hold_secs,
                            session_id: session_id.clone(),
                            observe_remaining_secs: obs,
                        }),
                        vec![
                            FsmSideEffect::Log {
                                event_type: "L1".into(),
                                reason: None,
                                duration_secs: hold_secs,
                            },
                            FsmSideEffect::PlaySound {
                                kind: "chime".into(),
                            },
                        ],
                    )
                } else {
                    (None, vec![])
                }
            }

            (
                SystemState::InterventionLevel1 {
                    session_id,
                    phone_hold_duration_secs,
                    observe_remaining_secs,
                },
                FsmEvent::VisionPhoneDetectedUpdate { hold_secs },
            ) => {
                if hold_secs < release_threshold {
                    // 放下 → 拉回
                    (
                        Some(SystemState::FocusActive {
                            remaining_secs: 0, // 由 AppState 恢复快照
                            session_id: session_id.clone(),
                            debt_secs_owed: 0,
                        }),
                        vec![FsmSideEffect::Log {
                            event_type: "PULLBACK".into(),
                            reason: Some("L1".into()),
                            duration_secs: *phone_hold_duration_secs,
                        }],
                    )
                } else if hold_secs >= l2_threshold {
                    (
                        Some(SystemState::InterventionLevel2 {
                            phone_hold_duration_secs: hold_secs,
                            session_id: session_id.clone(),
                        }),
                        vec![
                            FsmSideEffect::Log {
                                event_type: "L2".into(),
                                reason: None,
                                duration_secs: hold_secs,
                            },
                            FsmSideEffect::PlayChime,
                            FsmSideEffect::PlaySound {
                                kind: "chime".into(),
                            },
                        ],
                    )
                } else {
                    (
                        Some(SystemState::InterventionLevel1 {
                            phone_hold_duration_secs: hold_secs,
                            session_id: session_id.clone(),
                            observe_remaining_secs: *observe_remaining_secs,
                        }),
                        vec![],
                    )
                }
            }

            (
                SystemState::InterventionLevel2 {
                    session_id,
                    phone_hold_duration_secs,
                },
                FsmEvent::VisionPhoneDetectedUpdate { hold_secs },
            ) => {
                if hold_secs < release_threshold {
                    (
                        Some(SystemState::FocusActive {
                            remaining_secs: 0,
                            session_id: session_id.clone(),
                            debt_secs_owed: 0,
                        }),
                        vec![FsmSideEffect::Log {
                            event_type: "PULLBACK".into(),
                            reason: Some("L2".into()),
                            duration_secs: *phone_hold_duration_secs,
                        }],
                    )
                } else if hold_secs >= l3_threshold {
                    self.enter_l3(session_id, hold_secs)
                } else {
                    (
                        Some(SystemState::InterventionLevel2 {
                            phone_hold_duration_secs: hold_secs,
                            session_id: session_id.clone(),
                        }),
                        vec![],
                    )
                }
            }

            (
                SystemState::InterventionLevel3 {
                    session_id,
                    phone_hold_duration_secs,
                    escalate_elapsed_secs,
                },
                FsmEvent::VisionPhoneDetectedUpdate { hold_secs },
            ) => {
                if hold_secs < release_threshold {
                    (
                        Some(SystemState::FocusActive {
                            remaining_secs: 0,
                            session_id: session_id.clone(),
                            debt_secs_owed: 0,
                        }),
                        vec![FsmSideEffect::Log {
                            event_type: "L3_RECOVERED_BY_PUTDOWN".into(),
                            reason: None,
                            duration_secs: *phone_hold_duration_secs,
                        }],
                    )
                } else {
                    (
                        Some(SystemState::InterventionLevel3 {
                            phone_hold_duration_secs: hold_secs,
                            session_id: session_id.clone(),
                            escalate_elapsed_secs: *escalate_elapsed_secs,
                        }),
                        vec![],
                    )
                }
            }

            // 挡摄像头 → L3
            (
                SystemState::FocusActive { session_id, .. }
                | SystemState::InterventionLevel1 { session_id, .. }
                | SystemState::InterventionLevel2 { session_id, .. },
                FsmEvent::CameraBlockedOrCovered,
            ) => self.enter_l3(session_id, l3_threshold),

            // L2 知道了：不回 Focus
            (SystemState::InterventionLevel2 { .. }, FsmEvent::AcknowledgeLevel2) => (
                None,
                vec![FsmSideEffect::Log {
                    event_type: "IGNORED_L2".into(),
                    reason: None,
                    duration_secs: 0,
                }],
            ),

            // L3 输入原因 → 休息
            (
                SystemState::InterventionLevel3 { session_id, .. },
                FsmEvent::SubmitL3Reason { reason },
            ) => (
                Some(SystemState::TemporaryPause {
                    elapsed_secs: 0,
                    reason: reason.clone(),
                    session_id: session_id.clone(),
                }),
                vec![
                    FsmSideEffect::Log {
                        event_type: "PAUSE_START".into(),
                        reason: Some(reason),
                        duration_secs: 0,
                    },
                    FsmSideEffect::HideOverlay,
                    FsmSideEffect::ShowFloatingClock,
                    FsmSideEffect::StopWhitelistMonitor,
                    FsmSideEffect::StopVision,
                ],
            ),

            // L3 加深 tick
            (
                SystemState::InterventionLevel3 {
                    session_id,
                    phone_hold_duration_secs,
                    escalate_elapsed_secs,
                },
                FsmEvent::InterventionTick,
            ) => {
                let next_esc = escalate_elapsed_secs.saturating_add(1);
                let mut effects = vec![];
                if next_esc == 60 || (self.test_mode() && next_esc == 5) {
                    effects.push(FsmSideEffect::SevereEscalate);
                    effects.push(FsmSideEffect::PlaySound {
                        kind: "severe".into(),
                    });
                    effects.push(FsmSideEffect::Log {
                        event_type: "SEVERE".into(),
                        reason: Some(if self.test_mode() {
                            "l3_uncooperative_test".into()
                        } else {
                            "l3_uncooperative_60s".into()
                        }),
                        duration_secs: next_esc,
                    });
                }
                (
                    Some(SystemState::InterventionLevel3 {
                        phone_hold_duration_secs: *phone_hold_duration_secs,
                        session_id: session_id.clone(),
                        escalate_elapsed_secs: next_esc,
                    }),
                    effects,
                )
            }

            // L1 观察 tick：递减观察秒并回写状态，便于 UI 显示倒计时
            (
                SystemState::InterventionLevel1 {
                    session_id,
                    phone_hold_duration_secs,
                    observe_remaining_secs,
                },
                FsmEvent::InterventionTick,
            ) => {
                let next_obs = observe_remaining_secs.saturating_sub(1);
                *self.l1_observe_remaining.write() = next_obs;
                (
                    Some(SystemState::InterventionLevel1 {
                        phone_hold_duration_secs: *phone_hold_duration_secs,
                        session_id: session_id.clone(),
                        observe_remaining_secs: next_obs,
                    }),
                    vec![],
                )
            }

            // —— 测试/强制退出：始终允许关遮罩；非 Idle 则回 Idle ——
            (SystemState::Idle, FsmEvent::TestExit) => (
                None,
                vec![
                    FsmSideEffect::HideOverlay,
                    FsmSideEffect::HideFloatingClock,
                    FsmSideEffect::StopWhitelistMonitor,
                    FsmSideEffect::StopVision,
                ],
            ),
            (_, FsmEvent::TestExit) => (
                Some(SystemState::Idle),
                vec![
                    FsmSideEffect::Log {
                        event_type: "TEST_EXIT".into(),
                        reason: Some("dialog_or_button".into()),
                        duration_secs: 0,
                    },
                    FsmSideEffect::HideOverlay,
                    FsmSideEffect::HideFloatingClock,
                    FsmSideEffect::StopWhitelistMonitor,
                    FsmSideEffect::StopVision,
                    FsmSideEffect::PlaySound {
                        kind: "inject".into(),
                    },
                ],
            ),

            (
                SystemState::FocusActive { session_id, .. }
                | SystemState::InterventionLevel1 { session_id, .. }
                | SystemState::InterventionLevel2 { session_id, .. }
                | SystemState::InterventionLevel3 { session_id, .. },
                FsmEvent::TestInjectLevel { level },
            ) if self.test_mode() => {
                let sid = session_id.clone();
                match level {
                    1 => {
                        let obs = 3;
                        *self.l1_observe_remaining.write() = obs;
                        (
                            Some(SystemState::InterventionLevel1 {
                                phone_hold_duration_secs: l1_threshold,
                                session_id: sid,
                                observe_remaining_secs: obs,
                            }),
                            vec![
                                FsmSideEffect::Log {
                                    event_type: "TEST_INJECT".into(),
                                    reason: Some("L1".into()),
                                    duration_secs: l1_threshold,
                                },
                                FsmSideEffect::ShowOverlay,
                                FsmSideEffect::PlaySound {
                                    kind: "inject".into(),
                                },
                            ],
                        )
                    }
                    2 => (
                        Some(SystemState::InterventionLevel2 {
                            phone_hold_duration_secs: l2_threshold,
                            session_id: sid,
                        }),
                        vec![
                            FsmSideEffect::Log {
                                event_type: "TEST_INJECT".into(),
                                reason: Some("L2".into()),
                                duration_secs: l2_threshold,
                            },
                            FsmSideEffect::ShowOverlay,
                            FsmSideEffect::PlayChime,
                            FsmSideEffect::PlaySound {
                                kind: "chime".into(),
                            },
                        ],
                    ),
                    3 => {
                        let (st, mut eff) = self.enter_l3(&sid, l3_threshold);
                        eff.push(FsmSideEffect::Log {
                            event_type: "TEST_INJECT".into(),
                            reason: Some("L3".into()),
                            duration_secs: l3_threshold,
                        });
                        eff.push(FsmSideEffect::ShowOverlay);
                        eff.push(FsmSideEffect::PlaySound {
                            kind: "severe".into(),
                        });
                        (st, eff)
                    }
                    _ => (None, vec![]),
                }
            }

            // 到点三选一
            (
                SystemState::AwaitSessionEndChoice { session_id },
                FsmEvent::ChooseContinue,
            ) => {
                let mins = 45u32; // 默认；上层可用设置覆盖
                (
                    Some(SystemState::FocusActive {
                        remaining_secs: mins * 60,
                        session_id: session_id.clone(),
                        debt_secs_owed: 0,
                    }),
                    vec![
                        FsmSideEffect::Log {
                            event_type: "SESSION_CONTINUE".into(),
                            reason: None,
                            duration_secs: mins * 60,
                        },
                        FsmSideEffect::ShowOverlay,
                        FsmSideEffect::StartVision,
                        FsmSideEffect::StartWhitelistMonitor,
                    ],
                )
            }
            (
                SystemState::AwaitSessionEndChoice { session_id },
                FsmEvent::ChooseRest,
            ) => (
                Some(SystemState::TemporaryPause {
                    elapsed_secs: 0,
                    reason: "到点休息".into(),
                    session_id: session_id.clone(),
                }),
                vec![
                    FsmSideEffect::Log {
                        event_type: "PAUSE_START".into(),
                        reason: Some("到点休息".into()),
                        duration_secs: 0,
                    },
                    FsmSideEffect::HideOverlay,
                    FsmSideEffect::ShowFloatingClock,
                    FsmSideEffect::StopWhitelistMonitor,
                    FsmSideEffect::StopVision,
                ],
            ),
            (SystemState::AwaitSessionEndChoice { .. }, FsmEvent::ChooseEnd) => (
                Some(SystemState::Idle),
                vec![
                    FsmSideEffect::Log {
                        event_type: "SESSION_END".into(),
                        reason: Some("user_end".into()),
                        duration_secs: 0,
                    },
                    FsmSideEffect::HideOverlay,
                    FsmSideEffect::HideFloatingClock,
                    FsmSideEffect::StopWhitelistMonitor,
                    FsmSideEffect::StopVision,
                ],
            ),

            (_, FsmEvent::StopSession) => {
                // #36 F1：手动停止进入三选一仪式（继续/休息/结束），不直接结束
                if let Some(sid) = state.session_id() {
                    (
                        Some(SystemState::AwaitSessionEndChoice {
                            session_id: sid.to_string(),
                        }),
                        vec![
                            FsmSideEffect::Log {
                                event_type: "SESSION_STOP_CHOICE".into(),
                                reason: Some("manual_stop".into()),
                                duration_secs: 0,
                            },
                            // 确保遮罩弹出，展示三选一（主窗也可能自行展示）
                            FsmSideEffect::ShowOverlay,
                            FsmSideEffect::StopWhitelistMonitor,
                        ],
                    )
                } else {
                    (
                        Some(SystemState::Idle),
                        vec![
                            FsmSideEffect::Log {
                                event_type: "SESSION_END".into(),
                                reason: Some("stop".into()),
                                duration_secs: 0,
                            },
                            FsmSideEffect::HideOverlay,
                            FsmSideEffect::HideFloatingClock,
                            FsmSideEffect::StopWhitelistMonitor,
                            FsmSideEffect::StopVision,
                        ],
                    )
                }
            },

            _ => (None, vec![]),
        }
    }

    fn enter_l3(&self, session_id: &str, hold_secs: u32) -> (Option<SystemState>, Vec<FsmSideEffect>) {
        let mut count = self.l3_count_in_session.write();
        *count += 1;
        if *count >= 2 {
            return (
                Some(SystemState::Idle),
                vec![
                    FsmSideEffect::Log {
                        event_type: "SESSION_END".into(),
                        reason: Some("second_l3".into()),
                        duration_secs: hold_secs,
                    },
                    FsmSideEffect::Log {
                        event_type: "L3".into(),
                        reason: Some("second_in_session".into()),
                        duration_secs: hold_secs,
                    },
                    FsmSideEffect::HideOverlay,
                    FsmSideEffect::HideFloatingClock,
                    FsmSideEffect::StopWhitelistMonitor,
                    FsmSideEffect::StopVision,
                ],
            );
        }
        (
            Some(SystemState::InterventionLevel3 {
                phone_hold_duration_secs: hold_secs,
                session_id: session_id.to_string(),
                escalate_elapsed_secs: 0,
            }),
            vec![FsmSideEffect::Log {
                event_type: "L3".into(),
                reason: None,
                duration_secs: hold_secs,
            }],
        )
    }
}
