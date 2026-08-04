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
  vision_enabled: boolean;
  prefer_cpu_inference: boolean;
  camera_name: string;
  roi_json: string;
  whitelist_json: string;
  pending_debt_secs: number;
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

export const EVT = {
  fsm: "fsm_state_change",
  whitelist: "whitelist_hit",
  todayFocus: "today_focus_secs",
  debug: "debug_log",
  sessionEnd: "session_end_choice",
  openSettings: "open_settings",
  openReport: "open_report",
} as const;
