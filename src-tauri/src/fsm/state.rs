use serde::{Deserialize, Serialize};

/// 系统有限状态（与前端 tauri-ipc 对齐，使用外部标签枚举）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SystemState {
    Idle,
    FocusActive {
        remaining_secs: u32,
        session_id: String,
        debt_secs_owed: u32,
    },
    TemporaryPause {
        elapsed_secs: u32,
        reason: String,
        session_id: String,
    },
    InterventionLevel1 {
        phone_hold_duration_secs: u32,
        session_id: String,
    },
    InterventionLevel2 {
        phone_hold_duration_secs: u32,
        session_id: String,
    },
    InterventionLevel3 {
        phone_hold_duration_secs: u32,
        session_id: String,
        escalate_elapsed_secs: u32,
    },
    AwaitSessionEndChoice {
        session_id: String,
    },
}

impl Default for SystemState {
    fn default() -> Self {
        Self::Idle
    }
}

impl SystemState {
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::Idle => None,
            Self::FocusActive { session_id, .. }
            | Self::TemporaryPause { session_id, .. }
            | Self::InterventionLevel1 { session_id, .. }
            | Self::InterventionLevel2 { session_id, .. }
            | Self::InterventionLevel3 { session_id, .. }
            | Self::AwaitSessionEndChoice { session_id } => Some(session_id.as_str()),
        }
    }

    pub fn is_focus_ticking(&self) -> bool {
        matches!(self, Self::FocusActive { .. })
    }

    pub fn is_intervening(&self) -> bool {
        matches!(
            self,
            Self::InterventionLevel1 { .. }
                | Self::InterventionLevel2 { .. }
                | Self::InterventionLevel3 { .. }
        )
    }

    pub fn is_pausing(&self) -> bool {
        matches!(self, Self::TemporaryPause { .. })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FsmEvent {
    StartSession { focus_duration_mins: u32 },
    UserRequestPause { reason: String },
    ResumeFocus,
    SkipDebtAndResume,
    DoubleEscPressed,
    VisionPhoneDetectedUpdate { hold_secs: u32 },
    CameraBlockedOrCovered,
    SessionTimerTick,
    PauseTimerTick,
    InterventionTick,
    AcknowledgeLevel2,
    SubmitL3Reason { reason: String },
    ChooseContinue,
    ChooseRest,
    ChooseEnd,
    StopSession,
}

/// 状态迁移时附带的副作用（写库、开窗等由上层执行）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FsmSideEffect {
    Log {
        event_type: String,
        reason: Option<String>,
        duration_secs: u32,
    },
    ShowOverlay,
    HideOverlay,
    ShowFloatingClock,
    HideFloatingClock,
    StartWhitelistMonitor,
    StopWhitelistMonitor,
    StartVision,
    StopVision,
    PlayChime,
    SevereEscalate,
}
