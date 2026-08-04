use rusqlite::{params, Connection, OptionalExtension, Result as SqlResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WeeklyReport {
    pub total_focus_minutes: u32,
    pub successful_pullbacks_count: u32,
    pub total_borrowed_rest_minutes: u32,
    pub golden_focus_hour_range: String,
    pub avg_focus_minutes: u32,
    pub interrupted_count: u32,
    pub vs_last_week_focus_delta_minutes: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SettingsRecord {
    pub setup_completed: bool,
    pub default_focus_mins: u32,
    pub debt_floor_secs: u32,
    pub emergency_hotkey: String,
    pub debug_mode: bool,
    pub vision_enabled: bool,
    pub prefer_cpu_inference: bool,
    pub camera_name: String,
    pub roi_json: String,
    pub whitelist_json: String,
    pub pending_debt_secs: u32,
}

impl Default for SettingsRecord {
    fn default() -> Self {
        Self {
            setup_completed: false,
            default_focus_mins: 45,
            debt_floor_secs: 180,
            emergency_hotkey: "double_esc".into(),
            debug_mode: false,
            vision_enabled: true,
            prefer_cpu_inference: false,
            camera_name: String::new(),
            roi_json: String::new(),
            whitelist_json: "[]".into(),
            pending_debt_secs: 0,
        }
    }
}

pub struct LocalLogger {
    conn: Connection,
    db_path: PathBuf,
}

impl LocalLogger {
    pub fn open(data_dir: &Path) -> SqlResult<Self> {
        std::fs::create_dir_all(data_dir).ok();
        let db_path = data_dir.join("deepflow.db");
        let conn = Connection::open(&db_path)?;
        let logger = Self { conn, db_path };
        logger.migrate()?;
        Ok(logger)
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    fn migrate(&self) -> SqlResult<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS focus_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                reason TEXT,
                duration_secs INTEGER DEFAULT 0,
                created_at TEXT DEFAULT (datetime('now','localtime'))
            );
            CREATE TABLE IF NOT EXISTS settings (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                setup_completed INTEGER NOT NULL DEFAULT 0,
                default_focus_mins INTEGER NOT NULL DEFAULT 45,
                debt_floor_secs INTEGER NOT NULL DEFAULT 180,
                emergency_hotkey TEXT NOT NULL DEFAULT 'double_esc',
                debug_mode INTEGER NOT NULL DEFAULT 0,
                vision_enabled INTEGER NOT NULL DEFAULT 1,
                prefer_cpu_inference INTEGER NOT NULL DEFAULT 0,
                camera_name TEXT NOT NULL DEFAULT '',
                roi_json TEXT NOT NULL DEFAULT '',
                whitelist_json TEXT NOT NULL DEFAULT '[]',
                pending_debt_secs INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS daily_focus (
                day TEXT PRIMARY KEY,
                focus_secs INTEGER NOT NULL DEFAULT 0
            );
            INSERT OR IGNORE INTO settings (id) VALUES (1);
            "#,
        )?;
        Ok(())
    }

    pub fn log_event(
        &self,
        session_id: &str,
        event_type: &str,
        reason: Option<&str>,
        duration_secs: u32,
    ) -> SqlResult<()> {
        self.conn.execute(
            "INSERT INTO focus_logs (session_id, event_type, reason, duration_secs) VALUES (?1, ?2, ?3, ?4)",
            params![
                session_id,
                event_type,
                reason.unwrap_or(""),
                duration_secs
            ],
        )?;
        Ok(())
    }

    pub fn add_focus_secs_today(&self, secs: u32) -> SqlResult<()> {
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();
        self.conn.execute(
            r#"
            INSERT INTO daily_focus (day, focus_secs) VALUES (?1, ?2)
            ON CONFLICT(day) DO UPDATE SET focus_secs = focus_secs + excluded.focus_secs
            "#,
            params![day, secs],
        )?;
        Ok(())
    }

    pub fn today_focus_secs(&self) -> SqlResult<u32> {
        let day = chrono::Local::now().format("%Y-%m-%d").to_string();
        let secs: u32 = self
            .conn
            .query_row(
                "SELECT focus_secs FROM daily_focus WHERE day = ?1",
                params![day],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0);
        Ok(secs)
    }

    pub fn load_settings(&self) -> SqlResult<SettingsRecord> {
        self.conn.query_row(
            r#"
            SELECT setup_completed, default_focus_mins, debt_floor_secs, emergency_hotkey,
                   debug_mode, vision_enabled, prefer_cpu_inference, camera_name, roi_json,
                   whitelist_json, pending_debt_secs
            FROM settings WHERE id = 1
            "#,
            [],
            |row| {
                Ok(SettingsRecord {
                    setup_completed: row.get::<_, i64>(0)? != 0,
                    default_focus_mins: row.get(1)?,
                    debt_floor_secs: row.get(2)?,
                    emergency_hotkey: row.get(3)?,
                    debug_mode: row.get::<_, i64>(4)? != 0,
                    vision_enabled: row.get::<_, i64>(5)? != 0,
                    prefer_cpu_inference: row.get::<_, i64>(6)? != 0,
                    camera_name: row.get(7)?,
                    roi_json: row.get(8)?,
                    whitelist_json: row.get(9)?,
                    pending_debt_secs: row.get(10)?,
                })
            },
        )
    }

    pub fn save_settings(&self, s: &SettingsRecord) -> SqlResult<()> {
        self.conn.execute(
            r#"
            UPDATE settings SET
              setup_completed = ?1,
              default_focus_mins = ?2,
              debt_floor_secs = ?3,
              emergency_hotkey = ?4,
              debug_mode = ?5,
              vision_enabled = ?6,
              prefer_cpu_inference = ?7,
              camera_name = ?8,
              roi_json = ?9,
              whitelist_json = ?10,
              pending_debt_secs = ?11
            WHERE id = 1
            "#,
            params![
                s.setup_completed as i64,
                s.default_focus_mins,
                s.debt_floor_secs,
                s.emergency_hotkey,
                s.debug_mode as i64,
                s.vision_enabled as i64,
                s.prefer_cpu_inference as i64,
                s.camera_name,
                s.roi_json,
                s.whitelist_json,
                s.pending_debt_secs,
            ],
        )?;
        Ok(())
    }

    pub fn generate_weekly_report(&self) -> SqlResult<WeeklyReport> {
        let pullbacks: u32 = self.conn.query_row(
            "SELECT COUNT(*) FROM focus_logs WHERE event_type = 'PULLBACK' AND created_at >= datetime('now','-7 days','localtime')",
            [],
            |row| row.get(0),
        )?;

        let rest_secs: u32 = self.conn.query_row(
            "SELECT COALESCE(SUM(duration_secs),0) FROM focus_logs WHERE event_type = 'PAUSE_END' AND created_at >= datetime('now','-7 days','localtime')",
            [],
            |row| row.get(0),
        )?;

        let focus_secs: u32 = self.conn.query_row(
            "SELECT COALESCE(SUM(focus_secs),0) FROM daily_focus WHERE day >= date('now','-7 days','localtime')",
            [],
            |row| row.get(0),
        )?;

        let last_week_focus: u32 = self.conn.query_row(
            "SELECT COALESCE(SUM(focus_secs),0) FROM daily_focus WHERE day >= date('now','-14 days','localtime') AND day < date('now','-7 days','localtime')",
            [],
            |row| row.get(0),
        )?;

        let interrupted: u32 = self.conn.query_row(
            "SELECT COUNT(*) FROM focus_logs WHERE event_type IN ('EMERGENCY_EXIT','SEVERE','L3') AND created_at >= datetime('now','-7 days','localtime')",
            [],
            |row| row.get(0),
        )?;

        let sessions: u32 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM focus_logs WHERE event_type = 'SESSION_START' AND created_at >= datetime('now','-7 days','localtime')",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0)
            .max(1);

        let total_focus_minutes = focus_secs / 60;
        Ok(WeeklyReport {
            total_focus_minutes,
            successful_pullbacks_count: pullbacks,
            total_borrowed_rest_minutes: rest_secs / 60,
            golden_focus_hour_range: "10:00 - 11:30".into(), // P2: A主C辅算法
            avg_focus_minutes: total_focus_minutes / sessions,
            interrupted_count: interrupted,
            vs_last_week_focus_delta_minutes: total_focus_minutes as i32
                - (last_week_focus / 60) as i32,
        })
    }
}
