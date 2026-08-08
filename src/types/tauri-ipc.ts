export type SystemState =
  | { kind: "idle" }
  | {
      kind: "focus_active";
      remaining_secs: number;
      session_id: string;
      debt_secs_owed: number;
    }
  | {
      kind: "temporary_pause";
      elapsed_secs: number;
      reason: string;
      session_id: string;
    }
  | {
      kind: "intervention_level1";
      phone_hold_duration_secs: number;
      session_id: string;
      observe_remaining_secs: number;
    }
  | {
      kind: "intervention_level2";
      phone_hold_duration_secs: number;
      session_id: string;
    }
  | {
      kind: "intervention_level3";
      phone_hold_duration_secs: number;
      session_id: string;
      escalate_elapsed_secs: number;
    }
  | { kind: "await_session_end_choice"; session_id: string };

export interface SettingsRecord {
  setup_completed: boolean;
  default_focus_mins: number;
  debt_floor_secs: number;
  emergency_hotkey: string;
  debug_mode: boolean;
  test_mode: boolean;
  vision_enabled: boolean;
  prefer_cpu_inference: boolean;
  camera_name: string;
  roi_json: string;
  whitelist_json: string;
  pending_debt_secs: number;
  /** #11：周报 PNG 导出后自动打开所在目录 */
  auto_open_exports: boolean;
  /** #22：白名单违规处置：report | minimize | close_report */
  whitelist_action: string;
  /** #30：静音提示音 */
  sound_muted: boolean;
}

export interface WeeklyReport {
  total_focus_minutes: number;
  successful_pullbacks_count: number;
  total_borrowed_rest_minutes: number;
  golden_focus_hour_range: string;
  avg_focus_minutes: number;
  interrupted_count: number;
  vs_last_week_focus_delta_minutes: number;
}

export interface WhitelistHit {
  process_name: string;
  pid: number;
}

/** #14：data/models 目录下的 ONNX 模型文件元信息 */
export interface ModelEntry {
  name: string;
  size: number;
  modified: string;
}

/** #16：L3 原因记录 [created_at, reason] */
export type L3ReasonEntry = [string, string];

/** 归一化 ROI，坐标系 0..1 */
export interface RoiRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface DetectionInfo {
  has_phone: boolean;
  has_hand: boolean;
  phone_brightness: number;
  hand_phone_overlap: boolean;
  phone_score: number;
  backend: string;
}

export interface VisionStatus {
  running: boolean;
  enabled: boolean;
  detector: string;
  hold_secs: number;
  camera_name: string;
  has_preview: boolean;
  last_detection: DetectionInfo | null;
}

export const EVT = {
  fsm: "fsm_state_change",
  whitelist: "whitelist_hit",
  todayFocus: "today_focus_secs",
  debug: "debug_log",
  sessionEnd: "session_end_choice",
  openSettings: "open_settings",
  openReport: "open_report",
  playSound: "play_sound",
} as const;
