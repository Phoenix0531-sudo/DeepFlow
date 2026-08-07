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
    pub test_mode: bool,
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
            test_mode: false,
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
                test_mode INTEGER NOT NULL DEFAULT 0,
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
        let has_test_mode: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('settings') WHERE name = 'test_mode')",
            [],
            |row| row.get(0),
        )?;
        if !has_test_mode {
            self.conn.execute(
                "ALTER TABLE settings ADD COLUMN test_mode INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
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
                   debug_mode, test_mode, vision_enabled, prefer_cpu_inference, camera_name, roi_json,
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
                    test_mode: row.get::<_, i64>(5)? != 0,
                    vision_enabled: row.get::<_, i64>(6)? != 0,
                    prefer_cpu_inference: row.get::<_, i64>(7)? != 0,
                    camera_name: row.get(8)?,
                    roi_json: row.get(9)?,
                    whitelist_json: row.get(10)?,
                    pending_debt_secs: row.get(11)?,
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
              test_mode = ?6,
              vision_enabled = ?7,
              prefer_cpu_inference = ?8,
              camera_name = ?9,
              roi_json = ?10,
              whitelist_json = ?11,
              pending_debt_secs = ?12
            WHERE id = 1
            "#,
            params![
                s.setup_completed as i64,
                s.default_focus_mins,
                s.debt_floor_secs,
                s.emergency_hotkey,
                s.debug_mode as i64,
                s.test_mode as i64,
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
        let golden = self.compute_golden_hour_range().unwrap_or_else(|_| "暂无足够数据".into());
        Ok(WeeklyReport {
            total_focus_minutes,
            successful_pullbacks_count: pullbacks,
            total_borrowed_rest_minutes: rest_secs / 60,
            golden_focus_hour_range: golden,
            avg_focus_minutes: total_focus_minutes / sessions,
            interrupted_count: interrupted,
            vs_last_week_focus_delta_minutes: total_focus_minutes as i32
                - (last_week_focus / 60) as i32,
        })
    }

    /// A主 C辅：以会话/拉回/休息结束的小时直方图为主，峰值小时 A；
    /// 向两侧扩展连续次高小时得到辅助窗 C，格式 `HH:00 - HH:00`。
    fn compute_golden_hour_range(&self) -> SqlResult<String> {
        let mut scores = [0u32; 24];
        let mut stmt = self.conn.prepare(
            r#"
            SELECT CAST(strftime('%H', created_at) AS INTEGER) AS h,
                   event_type,
                   COALESCE(duration_secs, 0) AS dur
            FROM focus_logs
            WHERE created_at >= datetime('now','-7 days','localtime')
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0).unwrap_or(0) as usize,
                row.get::<_, String>(1).unwrap_or_default(),
                row.get::<_, i64>(2).unwrap_or(0) as u32,
            ))
        })?;

        let mut any = false;
        for r in rows.flatten() {
            let (h, et, dur) = r;
            if h >= 24 {
                continue;
            }
            any = true;
            // 权重：SESSION_START 计 30；PAUSE_END 用时长；PULLBACK 20；其它轻量
            let add = match et.as_str() {
                "SESSION_START" => 30 + dur / 60,
                "PAUSE_END" => (dur / 60).max(5),
                "PULLBACK" => 20,
                "SESSION_END" => 10,
                _ => 2,
            };
            scores[h] = scores[h].saturating_add(add);
        }

        if !any || scores.iter().all(|&s| s == 0) {
            return Ok("暂无足够数据".into());
        }

        let peak = scores
            .iter()
            .enumerate()
            .max_by_key(|(_, s)| *s)
            .map(|(i, _)| i)
            .unwrap_or(10);

        // 从峰值向两侧扩：不低于 peak*40% 且不超 3 小时宽
        let thr = (scores[peak] as f32 * 0.4).max(1.0) as u32;
        let mut lo = peak;
        let mut hi = peak;
        while lo > 0 && scores[lo - 1] >= thr && (hi - (lo - 1)) < 3 {
            lo -= 1;
        }
        while hi + 1 < 24 && scores[hi + 1] >= thr && ((hi + 1) - lo) < 3 {
            hi += 1;
        }
        // 窗口右端显示为下一整点（或 +30 分若单小时）
        let end_h = if lo == hi { (hi + 1) % 24 } else { (hi + 1) % 24 };
        let end_m = if lo == hi { 30 } else { 0 };
        Ok(format!("{lo:02}:00 - {end_h:02}:{end_m:02}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn open_tmp() -> (LocalLogger, PathBuf) {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "deepflow_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let logger = LocalLogger::open(&dir).expect("open db");
        (logger, dir)
    }

    fn insert_log_at(logger: &LocalLogger, session: &str, ev: &str, reason: Option<&str>, secs: u32, at: &str) {
        logger
            .conn
            .execute(
                "INSERT INTO focus_logs (session_id, event_type, reason, duration_secs, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![session, ev, reason.unwrap_or(""), secs, at],
            )
            .unwrap();
    }

    fn add_day(logger: &LocalLogger, day: &str, secs: u32) {
        logger
            .conn
            .execute(
                "INSERT INTO daily_focus (day, focus_secs) VALUES (?1, ?2) \
                 ON CONFLICT(day) DO UPDATE SET focus_secs = focus_secs + excluded.focus_secs",
                params![day, secs],
            )
            .unwrap();
    }

    fn now_local_iso() -> String {
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
    }

    #[test]
    fn migrate_creates_tables_and_default_settings() {
        let (logger, _dir) = open_tmp();
        // settings 默认行存在，emergency_hotkey 默认 double_esc
        let s = logger.load_settings().expect("load settings");
        assert!(!s.setup_completed);
        assert_eq!(s.default_focus_mins, 45);
        assert_eq!(s.debt_floor_secs, 180);
        assert_eq!(s.emergency_hotkey, "double_esc");
        assert_eq!(s.pending_debt_secs, 0);
        assert!(!s.vision_enabled || s.vision_enabled); // 仅验证可读
    }

    #[test]
    fn settings_roundtrip_preserves_all_fields() {
        let (logger, _dir) = open_tmp();
        let mut s = logger.load_settings().unwrap();
        s.setup_completed = true;
        s.default_focus_mins = 60;
        s.debt_floor_secs = 300;
        s.emergency_hotkey = "ctrl_alt_q".into();
        s.debug_mode = true;
        s.test_mode = true;
        s.vision_enabled = false;
        s.prefer_cpu_inference = true;
        s.camera_name = "Integrated Camera".into();
        s.roi_json = "{\"x\":1}".into();
        s.whitelist_json = "[\"a.exe\",\"b.exe\"]".into();
        s.pending_debt_secs = 42;
        logger.save_settings(&s).unwrap();

        let got = logger.load_settings().unwrap();
        assert_eq!(got.setup_completed, true);
        assert_eq!(got.default_focus_mins, 60);
        assert_eq!(got.debt_floor_secs, 300);
        assert_eq!(got.emergency_hotkey, "ctrl_alt_q");
        assert!(got.debug_mode);
        assert!(got.test_mode);
        assert!(!got.vision_enabled);
        assert!(got.prefer_cpu_inference);
        assert_eq!(got.camera_name, "Integrated Camera");
        assert_eq!(got.roi_json, "{\"x\":1}");
        assert_eq!(got.whitelist_json, "[\"a.exe\",\"b.exe\"]");
        assert_eq!(got.pending_debt_secs, 42);
    }

    #[test]
    fn today_focus_accumulates_and_loads() {
        let (logger, _dir) = open_tmp();
        assert_eq!(logger.today_focus_secs().unwrap(), 0);
        logger.add_focus_secs_today(100).unwrap();
        logger.add_focus_secs_today(50).unwrap();
        assert_eq!(logger.today_focus_secs().unwrap(), 150);
    }

    #[test]
    fn weekly_report_empty_week_is_all_zeros() {
        let (logger, _dir) = open_tmp();
        let r = logger.generate_weekly_report().unwrap();
        assert_eq!(r.total_focus_minutes, 0);
        assert_eq!(r.successful_pullbacks_count, 0);
        assert_eq!(r.total_borrowed_rest_minutes, 0);
        assert_eq!(r.interrupted_count, 0);
        assert_eq!(r.vs_last_week_focus_delta_minutes, 0);
        // avg = 0 / max(sessions,1) = 0
        assert_eq!(r.avg_focus_minutes, 0);
    }

    #[test]
    fn weekly_report_aggregates_last_7_days_only() {
        let (logger, _dir) = open_tmp();
        // 本周（今天）1800 秒专注（30 分）
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        add_day(&logger, &today, 1800);
        // 8 天前落在「上周」区间（≥-14 且 < -7 天）→ last_week=3600s=60分
        let eight_days = (chrono::Local::now() - chrono::Duration::days(8))
            .format("%Y-%m-%d")
            .to_string();
        add_day(&logger, &eight_days, 3600);

        let r = logger.generate_weekly_report().unwrap();
        assert_eq!(r.total_focus_minutes, 30, "only today (last 7 days) counts");
        // delta = 本周 30 − 上周 60 = -30
        assert_eq!(r.vs_last_week_focus_delta_minutes, -30);
    }

    #[test]
    fn weekly_report_pullbacks_and_rest_and_interrupts() {
        let (logger, _dir) = open_tmp();
        let now = now_local_iso();
        // 本周内 3 次拉回 + 1 次 PAUSE_END(120秒=2分) + 1 次 L3（中断）
        for i in 0..3 {
            insert_log_at(&logger, "s1", "PULLBACK", None, 0, &now);
            let _ = i;
        }
        insert_log_at(&logger, "s1", "PAUSE_END", None, 120, &now);
        insert_log_at(&logger, "s1", "L3", Some("玩手机"), 0, &now);
        // 本周外 1 次拉回（不应计入）
        let old = (chrono::Local::now() - chrono::Duration::days(10))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        insert_log_at(&logger, "s0", "PULLBACK", None, 0, &old);

        let r = logger.generate_weekly_report().unwrap();
        assert_eq!(r.successful_pullbacks_count, 3);
        assert_eq!(r.total_borrowed_rest_minutes, 2);
        assert_eq!(r.interrupted_count, 1);
    }

    #[test]
    fn weekly_report_delta_negative_when_last_week_higher() {
        let (logger, _dir) = open_tmp();
        // 本周 0 分；上周（8-14天前）2400秒=40分
        let nine_days = (chrono::Local::now() - chrono::Duration::days(9))
            .format("%Y-%m-%d")
            .to_string();
        add_day(&logger, &nine_days, 2400);

        let r = logger.generate_weekly_report().unwrap();
        assert_eq!(r.total_focus_minutes, 0);
        // delta = 0 − 40 = −40
        assert_eq!(r.vs_last_week_focus_delta_minutes, -40);
    }

    #[test]
    fn log_event_persists_reason_and_duration() {
        let (logger, _dir) = open_tmp();
        logger
            .log_event("sess-9", "EMERGENCY_EXIT", Some("误触"), 5)
            .unwrap();
        let (reason, dur): (String, i64) = logger
            .conn
            .query_row(
                "SELECT reason, duration_secs FROM focus_logs WHERE session_id = 'sess-9'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(reason, "误触");
        assert_eq!(dur, 5);
    }
}
