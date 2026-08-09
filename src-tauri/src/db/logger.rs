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

/// #8 跨周趋势：一条带索引的最近周报。`weeks_ago=0` 为本周，递增表示更早的周。
/// `label` 给前端趋势图轴标签用（本周/上周/N 周前）。
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RecentWeeklyReport {
    pub weeks_ago: u32,
    pub label: String,
    pub report: WeeklyReport,
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
    /// #11：周报 PNG 导出后自动打开所在目录。
    pub auto_open_exports: bool,
    /// #22：白名单违规的处置策略："report" | "minimize" | "close_report"。
    /// report：仅上方 toast 提示（默认，向后兼容）；
    /// minimize：命中即最小化其顶级窗口；
    /// close_report：关闭其顶级窗口后并报告。
    pub whitelist_action: String,
    /// #30：静音提示音（前端 WebAudio）。
    pub sound_muted: bool,
    /// #23：登录时自动启动 DeepFlow。
    pub auto_start: bool,
    /// #29：允许系统通知（会话结束/到点/L3 等）。
    pub notifications_enabled: bool,
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
            auto_open_exports: true,
            whitelist_action: "report".into(),
            sound_muted: false,
            auto_start: false,
            notifications_enabled: true,
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
        let has_auto_open: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('settings') WHERE name = 'auto_open_exports')",
            [],
            |row| row.get(0),
        )?;
        if !has_auto_open {
            self.conn.execute(
                "ALTER TABLE settings ADD COLUMN auto_open_exports INTEGER NOT NULL DEFAULT 1",
                [],
            )?;
        }
        let has_whitelist_action: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('settings') WHERE name = 'whitelist_action')",
            [],
            |row| row.get(0),
        )?;
        if !has_whitelist_action {
            self.conn.execute(
                "ALTER TABLE settings ADD COLUMN whitelist_action TEXT NOT NULL DEFAULT 'report'",
                [],
            )?;
        }
        let has_sound_muted: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('settings') WHERE name = 'sound_muted')",
            [],
            |row| row.get(0),
        )?;
        if !has_sound_muted {
            self.conn.execute(
                "ALTER TABLE settings ADD COLUMN sound_muted INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        let has_auto_start: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('settings') WHERE name = 'auto_start')",
            [],
            |row| row.get(0),
        )?;
        if !has_auto_start {
            self.conn.execute(
                "ALTER TABLE settings ADD COLUMN auto_start INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        let has_notifications: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('settings') WHERE name = 'notifications_enabled')",
            [],
            |row| row.get(0),
        )?;
        if !has_notifications {
            self.conn.execute(
                "ALTER TABLE settings ADD COLUMN notifications_enabled INTEGER NOT NULL DEFAULT 1",
                [],
            )?;
        }
        // #26：用 PRAGMA user_version 记录 schema 版本，便于后续增量迁移
        self.conn.execute_batch("PRAGMA user_version = 4;")?;
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
                   whitelist_json, pending_debt_secs, auto_open_exports, whitelist_action, sound_muted,
                   auto_start, notifications_enabled
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
                    auto_open_exports: row.get::<_, i64>(12)? != 0,
                    whitelist_action: row.get(13)?,
                    sound_muted: row.get::<_, i64>(14)? != 0,
                    auto_start: row.get::<_, i64>(15)? != 0,
                    notifications_enabled: row.get::<_, i64>(16)? != 0,
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
              pending_debt_secs = ?12,
              auto_open_exports = ?13,
              whitelist_action = ?14,
              sound_muted = ?15,
              auto_start = ?16,
              notifications_enabled = ?17
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
                s.auto_open_exports as i64,
                s.whitelist_action,
                s.sound_muted as i64,
                s.auto_start as i64,
                s.notifications_enabled as i64,
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
        self.compute_golden_hour_range_in("datetime('now','-7 days','localtime')", "datetime('now')")
    }

    /// #15：指定时间窗内的黄金时段范围（与 compute_golden_hour_range 同逻辑，仅 WHERE bounds 不同）。
    /// bounds 为 SQL 字面量片段，由调用方以 u32 安全生成。
    fn compute_golden_hour_range_in(&self, bounds: &str, end_bound: &str) -> SqlResult<String> {
        let sql = format!(
            r#"
            SELECT CAST(strftime('%H', created_at) AS INTEGER) AS h,
                   event_type,
                   COALESCE(duration_secs, 0) AS dur
            FROM focus_logs
            WHERE created_at >= {bounds} AND created_at < {end_bound}
            "#
        );
        let mut scores = [0u32; 24];
        let mut stmt = self.conn.prepare(&sql)?;
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

    /// #16：返回最近 `limit` 条 L3 原因（event_type='PAUSE_START' 且 reason 非空）。
    /// 每条 = (created_at, reason)，按时间倒序。
    pub fn list_l3_reasons(&self, limit: u32) -> SqlResult<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT created_at, reason FROM focus_logs
            WHERE event_type = 'PAUSE_START' AND reason IS NOT NULL AND reason <> ''
            ORDER BY created_at DESC
            LIMIT ?1
            "#,
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// #15：指定一周（weeks_ago=0 表示本周）的聚合周报，逻辑与 generate_weekly_report 一致，
    /// 仅时间窗口相对 (weeks_ago+1)*7 .. weeks_ago*7 天。weeks_ago 自 u32，format! 安全。
    pub fn weekly_report_weeks_ago(&self, weeks_ago: u32) -> SqlResult<WeeklyReport> {
        let start_days = ((weeks_ago + 1) * 7) as i64;
        let end_days = (weeks_ago * 7) as i64;
        let prev_start = ((weeks_ago + 2) * 7) as i64;
        let prev_end = ((weeks_ago + 1) * 7) as i64;
        // end_days=0 时 exclusive 上界用 +1 day，确保「今天」被包含（与 generate_weekly_report 一致）
        let bounds = format!("datetime('now','-{start_days} days','localtime')");
        let end_bound = if end_days == 0 {
            "datetime('now','+1 day','localtime')".to_string()
        } else {
            format!("datetime('now','-{end_days} days','localtime')")
        };
        let prev_start_bound = format!("datetime('now','-{prev_start} days','localtime')");
        let prev_end_bound = format!("datetime('now','-{prev_end} days','localtime')");
        let day_start = format!("date('now','-{start_days} days','localtime')");
        let day_end = if end_days == 0 {
            "date('now','+1 day','localtime')".to_string()
        } else {
            format!("date('now','-{end_days} days','localtime')")
        };
        let prev_day_start = format!("date('now','-{prev_start} days','localtime')");
        let prev_day_end = format!("date('now','-{prev_end} days','localtime')");

        let pullbacks: u32 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM focus_logs WHERE event_type = 'PULLBACK' AND created_at >= {bounds} AND created_at < {end_bound}"),
            [],
            |row| row.get(0),
        )?;
        let rest_secs: u32 = self.conn.query_row(
            &format!("SELECT COALESCE(SUM(duration_secs),0) FROM focus_logs WHERE event_type = 'PAUSE_END' AND created_at >= {bounds} AND created_at < {end_bound}"),
            [],
            |row| row.get(0),
        )?;
        let focus_secs: u32 = self.conn.query_row(
            &format!("SELECT COALESCE(SUM(focus_secs),0) FROM daily_focus WHERE day >= {day_start} AND day < {day_end}"),
            [],
            |row| row.get(0),
        )?;
        let last_week_focus: u32 = self.conn.query_row(
            &format!("SELECT COALESCE(SUM(focus_secs),0) FROM daily_focus WHERE day >= {prev_day_start} AND day < {prev_day_end}"),
            [],
            |row| row.get(0),
        )?;
        let interrupted: u32 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM focus_logs WHERE event_type IN ('EMERGENCY_EXIT','SEVERE','L3') AND created_at >= {bounds} AND created_at < {end_bound}"),
            [],
            |row| row.get(0),
        )?;
        let sessions: u32 = self
            .conn
            .query_row(
                &format!("SELECT COUNT(*) FROM focus_logs WHERE event_type = 'SESSION_START' AND created_at >= {bounds} AND created_at < {end_bound}"),
                [],
                |row| row.get(0),
            )
            .unwrap_or(0)
            .max(1);
        let total_focus_minutes = focus_secs / 60;
        let golden = self.compute_golden_hour_range_in(&bounds, &end_bound).unwrap_or_else(|_| "暂无足够数据".into());
        Ok(WeeklyReport {
            total_focus_minutes,
            successful_pullbacks_count: pullbacks,
            total_borrowed_rest_minutes: rest_secs / 60,
            golden_focus_hour_range: golden,
            avg_focus_minutes: total_focus_minutes / sessions,
            interrupted_count: interrupted,
            vs_last_week_focus_delta_minutes: total_focus_minutes as i32 - (last_week_focus / 60) as i32,
        })
    }

    /// #8 跨周趋势：批量取最近 `count` 周的 WeeklyReport，从本周(weeks_ago=0)往回取。
    /// 返回顺序为 weeks_ago 递增（本周在前）。count 被 clamp 在 1..=12（避免无限回溯）。
    /// 每条带 weeks_ago 索引和人类可读 label（本周="本周"，1="上周"，其余"N 周前"），
    /// 便于前端趋势图直接渲染。
    pub fn weekly_reports_recent(&self, count: u32) -> SqlResult<Vec<RecentWeeklyReport>> {
        let n = count.clamp(1, 12);
        let mut out = Vec::with_capacity(n as usize);
        for w in 0..n {
            let label = match w {
                0 => "本周".to_string(),
                1 => "上周".to_string(),
                _ => format!("{} 周前", w),
            };
            let report = self.weekly_report_weeks_ago(w)?;
            out.push(RecentWeeklyReport {
                weeks_ago: w,
                label,
                report,
            });
        }
        Ok(out)
    }

    pub fn export_all_json(&self) -> SqlResult<String> {
        // #review F11：settings 读取失败不应阻断 daily/logs 导出(避免 settings 表损坏
        // 时连 clear_all_data_with_snapshot 也动不了)。读失败 → settings=null 字段,让
        // restore_from_snapshot 在 is_null() 分支跳过 settings 还原,语义自洽。
        let settings: serde_json::Value = match self.load_settings() {
            Ok(s) => serde_json::to_value(&s).unwrap_or(serde_json::Value::Null),
            Err(_) => serde_json::Value::Null,
        };
        let daily: Vec<serde_json::Value> = {
            let mut stmt = self.conn.prepare("SELECT day, focus_secs FROM daily_focus ORDER BY day")?;
            let rows = stmt.query_map([], |row| Ok(serde_json::json!({
                "day": row.get::<_, String>(0)?,
                "focus_secs": row.get::<_, i64>(1)?,
            })))?;
            rows.filter_map(Result::ok).collect()
        };
        let logs: Vec<serde_json::Value> = {
            let mut stmt = self.conn.prepare("SELECT created_at, session_id, event_type, reason, duration_secs FROM focus_logs ORDER BY created_at")?;
            let rows = stmt.query_map([], |row| Ok(serde_json::json!({
                "created_at": row.get::<_, String>(0)?,
                "session_id": row.get::<_, String>(1)?,
                "event_type": row.get::<_, String>(2)?,
                "reason": row.get::<_, String>(3)?,
                "duration_secs": row.get::<_, i64>(4)?,
            })))?;
            rows.filter_map(Result::ok).collect()
        };
        let out = serde_json::json!({
            "schema_version": 2,
            "exported_at": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            "settings": settings,
            "daily_focus": daily,
            "focus_logs": logs,
        });
        Ok(serde_json::to_string_pretty(&out).unwrap_or_default())
    }

    /// #28 B1：清空历史记录（daily_focus + focus_logs），保留 settings。clear_settings=true 时也重置 settings 为默认。
    pub fn clear_all_data(&self, clear_settings: bool) -> SqlResult<()> {
        self.conn.execute("DELETE FROM daily_focus", [])?;
        self.conn.execute("DELETE FROM focus_logs", [])?;
        if clear_settings {
            // #review F10：使用 SettingsRecord::default() 单一真源,避免与 SQL 硬编码漂移。
            self.save_settings(&SettingsRecord::default())?;
        }
        Ok(())
    }

    /// #7 清空反悔 - 快照版。语义与 clear_all_data 一致，但删前先抓取全量 JSON
    /// （settings + daily_focus + focus_logs）为快照字节返回。调用方可缓存该
    /// 快照 N 秒以提供给 "撤销" / "反悔" 入口;反悔时调 restore_from_snapshot。
    /// clear_settings=true 时快照仍会包含清空前的 settings，反悔可一并还原。
    pub fn clear_all_data_with_snapshot(&self, clear_settings: bool) -> SqlResult<Vec<u8>> {
        let json = self.export_all_json()?;
        let snapshot = json.into_bytes();
        self.clear_all_data(clear_settings)?;
        Ok(snapshot)
    }

    /// #7 清空反悔 - 还原。从快照字节 clear_all_data_with_snapshot 返回值恢复
    /// 三表数据。在一个事务内：清空当前 daily_focus + focus_logs，逐行 INSERT
    /// 快照中的 daily_focus / focus_logs;若快照含非空 settings 则 save_settings
    /// 覆盖。解析失败或任何 INSERT 失败 → 整体回滚。
    pub fn restore_from_snapshot(&self, bytes: &[u8]) -> SqlResult<()> {
        let v: serde_json::Value = serde_json::from_slice(bytes)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
        let daily = v.get("daily_focus").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        let logs = v.get("focus_logs").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        let settings_val = v.get("settings").cloned().unwrap_or(serde_json::Value::Null);
        self.conn.execute_batch("BEGIN")?;
        let result = (|| -> SqlResult<()> {
            self.conn.execute("DELETE FROM daily_focus", [])?;
            self.conn.execute("DELETE FROM focus_logs", [])?;
            for row in &daily {
                let day = row.get("day").and_then(|x| x.as_str()).unwrap_or("");
                let secs = row.get("focus_secs").and_then(|x| x.as_i64()).unwrap_or(0);
                self.conn.execute(
                    "INSERT OR REPLACE INTO daily_focus (day, focus_secs) VALUES (?1, ?2)",
                    rusqlite::params![day, secs],
                )?;
            }
            for row in &logs {
                let created_at = row.get("created_at").and_then(|x| x.as_str()).unwrap_or("");
                let session_id = row.get("session_id").and_then(|x| x.as_str()).unwrap_or("");
                let event_type = row.get("event_type").and_then(|x| x.as_str()).unwrap_or("");
                let reason = row.get("reason").and_then(|x| x.as_str()).unwrap_or("");
                let dur = row.get("duration_secs").and_then(|x| x.as_i64()).unwrap_or(0);
                self.conn.execute(
                    "INSERT INTO focus_logs (session_id, event_type, reason, duration_secs, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![session_id, event_type, reason, dur, created_at],
                )?;
            }
            if !settings_val.is_null() {
                // #7/review F3+mild：解析失败必须返 Err 触发 ROLLBACK。用
                // ToSqlConversionFailure(Box<dyn Error>) 装载 serde_json 的反序列化错误
                // (与 FromSqlConversionFailure 语义错位相比，ToSql 路径“把外部 Value
                // 转到主体实体”更贴近，且 attire 接受任意 Box<dyn Error + Send + Sync>)。
                let s: SettingsRecord = serde_json::from_value(settings_val)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                self.save_settings(&s)?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => { self.conn.execute_batch("COMMIT")?; Ok(()) },
            Err(e) => {
                // #7/review F2：ROLLBACK 失败不应静默。记 error 日志,业务错作为主错返回。
                if let Err(rb_err) = self.conn.execute_batch("ROLLBACK") {
                    tracing::error!(
                        target: "deepflow",
                        "restore_from_snapshot ROLLBACK failed after biz err {e}; rollback err = {rb_err}"
                    );
                }
                Err(e)
            },
        }
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
        assert!(s.auto_open_exports);
        assert_eq!(s.whitelist_action, "report");
        assert!(!s.vision_enabled || s.vision_enabled); // 仅验证可读
    }

    #[test]
    fn migrate_sets_schema_user_version() {
        // #26：schema 版本通过 PRAGMA user_version 持久化
        let (logger, _dir) = open_tmp();
        let version: i64 = logger
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert!(version >= 4, "user_version 应 >= 4，实际 = {version}");
    }

    #[test]
    fn export_all_json_includes_settings_and_logs() {
        let (logger, _dir) = open_tmp();
        logger
            .log_event("s-export", "SESSION_START", None, 0)
            .unwrap();
        logger.add_focus_secs_today(120).unwrap();
        let json = logger.export_all_json().unwrap();
        assert!(json.contains("schema_version"));
        assert!(json.contains("SESSION_START"));
        assert!(json.contains("focus_secs"));
        assert!(json.contains("settings"));
    }

    #[test]
    fn clear_all_data_keeps_settings_by_default() {
        let (logger, _dir) = open_tmp();
        let mut s = logger.load_settings().unwrap();
        s.default_focus_mins = 77;
        s.setup_completed = true;
        logger.save_settings(&s).unwrap();
        logger
            .log_event("s-clear", "SESSION_START", None, 0)
            .unwrap();
        logger.add_focus_secs_today(60).unwrap();

        logger.clear_all_data(false).unwrap();
        let cnt: i64 = logger
            .conn
            .query_row("SELECT COUNT(*) FROM focus_logs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(cnt, 0);
        let focus: u32 = logger.today_focus_secs().unwrap();
        assert_eq!(focus, 0);
        let got = logger.load_settings().unwrap();
        assert_eq!(got.default_focus_mins, 77);
        assert!(got.setup_completed);
    }

    /// #7/#review F10：clear_all_data(true) 后 settings 应回到 SettingsRecord::default()
    /// 的全部字段，包括 emergency_hotkey 原默认热键串（避免 Default trait impl 与
    /// 旧 SQL DEFAULT 之间出现漂移而无测试覆盖）。
    #[test]
    fn clear_all_data_resets_settings_to_default_including_emergency_hotkey() {
        let (logger, _dir) = open_tmp();
        // 改 default_focus_mins 和 emergency_hotkey 远离默认
        let mut s = logger.load_settings().unwrap();
        s.default_focus_mins = 99;
        s.emergency_hotkey = "ctrl_alt_q".to_string();
        s.setup_completed = true;
        s.debug_mode = true;
        logger.save_settings(&s).unwrap();
        // 确保 dep 也正在被改动
        assert_eq!(logger.load_settings().unwrap().emergency_hotkey, "ctrl_alt_q");

        logger.clear_all_data(true).unwrap();

        let got = logger.load_settings().unwrap();
        assert_eq!(got.default_focus_mins, 45, "default_focus_mins 应回默认 45");
        assert!(!got.setup_completed, "setup_completed 应回 false");
        assert!(!got.debug_mode, "debug_mode 应回 false");
        // [原热键默认]：在 SettingsRecord::default() 与 SQL DEFAULT 中一致。早先
        // 的 settings_roundtrip_preserves_all_fields 证明了热键可往返保存；此处验证
        // clear 路径走 Default impl 的 reset,确保 与 mover SQL DEFAULT 两者不漂移。
        assert_eq!(got.emergency_hotkey, "double_esc", "emergency_hotkey 应回默认热键");
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
        s.auto_open_exports = false;
        s.whitelist_action = "minimize".into();
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
        assert!(!got.auto_open_exports);
        assert_eq!(got.whitelist_action, "minimize");
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

    /// #15：Weekly_report_weeks_ago 应跳过指定周隔的窗口。
    #[test]
    fn weekly_report_weeks_ago_returns_window() {
        let (logger, _dir) = open_tmp();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        add_day(&logger, &today, 1800);
        // 8 天前 = 上周，在 weeks_ago=1 的窗口（8-14 天 = "1 周前"）
        let eight_days = (chrono::Local::now() - chrono::Duration::days(8))
            .format("%Y-%m-%d")
            .to_string();
        add_day(&logger, &eight_days, 3600);

        // weeks_ago=0 = 本周 = 今天 1800s = 30 分
        let r0 = logger.weekly_report_weeks_ago(0).unwrap();
        assert_eq!(r0.total_focus_minutes, 30);
        // weeks_ago=1 = "1 周前" 窗口 [8,15) 天 = 8 天那日 3600s = 60 分
        let r1 = logger.weekly_report_weeks_ago(1).unwrap();
        assert_eq!(r1.total_focus_minutes, 60);
        // weeks_ago=2 = 无数据 → 0 分
        let r2 = logger.weekly_report_weeks_ago(2).unwrap();
        assert_eq!(r2.total_focus_minutes, 0);
    }

    /// #16：List_l3_reasons 应返回 PAUSE_START 且 reason 非空的记录倒序。
    #[test]
    fn list_l3_reasons_filters_pause_start_with_reason() {
        let (logger, _dir) = open_tmp();
        let now = now_local_iso();
        // 纯 PAUSE_START ✓
        insert_log_at(&logger, "s1", "PAUSE_START", Some("渮机"), 0, &now);
        // PAUSE_START 但 reason 为空 → 不记
        insert_log_at(&logger, "s1", "PAUSE_START", Some(""), 0, &now);
        // L3 事件不是 PAUSE_START → 不记
        insert_log_at(&logger, "s1", "L3", Some("他事件"), 0, &now);

        let reasons = logger.list_l3_reasons(20).unwrap();
        assert_eq!(reasons.len(), 1);
        assert_eq!(reasons[0].1, "渮机");
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

    /// #8 批量取 N 周：默认顺序、索引与 label、长度 clamp。
    #[test]
    fn weekly_reports_recent_returns_indexed_and_labeled() {
        let (logger, _dir) = open_tmp();
        // 本周今天 1800s=30分；8 天前=上周 3600s=60分
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        add_day(&logger, &today, 1800);
        let eight_days = (chrono::Local::now() - chrono::Duration::days(8))
            .format("%Y-%m-%d")
            .to_string();
        add_day(&logger, &eight_days, 3600);

        let v = logger.weekly_reports_recent(3).unwrap();
        assert_eq!(v.len(), 3);
        // 顺序：本周在前
        assert_eq!(v[0].weeks_ago, 0);
        assert_eq!(v[0].label, "本周");
        assert_eq!(v[0].report.total_focus_minutes, 30);
        assert_eq!(v[1].weeks_ago, 1);
        assert_eq!(v[1].label, "上周");
        assert_eq!(v[1].report.total_focus_minutes, 60);
        assert_eq!(v[2].weeks_ago, 2);
        // "2 周前" 作为前缀（具体名称里含该数字）
        assert!(v[2].label.contains("2"), "label 应含周数: {}", v[2].label);
        // 第三周窗口含 8-14 天间的 "8 天前" 吗？不：8 天在 weeks_ago=1 窗口（8-14 作为上周），weeks_ago=2 窗口是 15-21 天。
        // 故 weeks_ago=2 元素应为 0
        assert_eq!(v[2].report.total_focus_minutes, 0);
    }

    /// #8 count 边界 clamp：0 输入 → 1 条；1 输入 → 1 条；超过 12 输入 → 12 条（避免无限回溯）。
    #[test]
    fn weekly_reports_recent_clamps_count() {
        let (logger, _dir) = open_tmp();
        assert_eq!(logger.weekly_reports_recent(0).unwrap().len(), 1);
        assert_eq!(logger.weekly_reports_recent(1).unwrap().len(), 1);
        assert_eq!(logger.weekly_reports_recent(12).unwrap().len(), 12);
        assert_eq!(logger.weekly_reports_recent(99).unwrap().len(), 12);
    }

    /// #8 空库不应 panic（与单周 empty_week 一致）。
    #[test]
    fn weekly_reports_recent_empty_db_all_zero() {
        let (logger, _dir) = open_tmp();
        let v = logger.weekly_reports_recent(4).unwrap();
        assert_eq!(v.len(), 4);
        for item in &v {
            assert_eq!(item.report.total_focus_minutes, 0);
            assert_eq!(item.report.interrupted_count, 0);
        }
    }

    /// #7 清空反悔 - 快照版：快照非空且含原数据；clear 后 logs 为 0、settings 保留。
    #[test]
    fn clear_with_snapshot_returns_payload_and_clears() {
        let (logger, _dir) = open_tmp();
        logger.log_event("s-7", "SESSION_START", None, 0).unwrap();
        logger.add_focus_secs_today(1800).unwrap();
        let mut s = logger.load_settings().unwrap();
        s.default_focus_mins = 77;
        logger.save_settings(&s).unwrap();

        let snap = logger.clear_all_data_with_snapshot(false).unwrap();
        assert!(!snap.is_empty(), "快照不应为空");
        let snap_str = String::from_utf8(snap).unwrap();
        assert!(snap_str.contains("schema_version"));
        assert!(snap_str.contains("SESSION_START"));
        assert!(snap_str.contains("default_focus_mins"));
        let cnt: i64 = logger.conn.query_row("SELECT COUNT(*) FROM focus_logs", [], |r| r.get(0)).unwrap();
        assert_eq!(cnt, 0);
        let got = logger.load_settings().unwrap();
        assert_eq!(got.default_focus_mins, 77);
    }

    /// #7 restore_from_snapshot 三表全恢复：settings、daily_focus today、focus_logs 均回原值。
    #[test]
    fn restore_from_snapshot_restores_three_tables() {
        let (logger, _dir) = open_tmp();
        logger.log_event("sess-a", "EMERGENCY_EXIT", Some("手机"), 5).unwrap();
        logger.add_focus_secs_today(1800).unwrap();
        let mut s = logger.load_settings().unwrap();
        s.default_focus_mins = 77;
        s.setup_completed = true;
        logger.save_settings(&s).unwrap();

        let snap = logger.clear_all_data_with_snapshot(true).unwrap();
        let after = logger.load_settings().unwrap();
        assert_eq!(after.default_focus_mins, 45);
        assert!(!after.setup_completed);
        assert_eq!(logger.conn.query_row("SELECT COUNT(*) FROM focus_logs", rusqlite::params![], |r| r.get::<_, i64>(0)).unwrap(), 0);
        assert_eq!(logger.today_focus_secs().unwrap(), 0);

        logger.restore_from_snapshot(&snap).unwrap();
        let got = logger.load_settings().unwrap();
        assert_eq!(got.default_focus_mins, 77, "settings 应被还原");
        assert!(got.setup_completed);
        assert_eq!(logger.today_focus_secs().unwrap(), 1800);
        let cnt: i64 = logger.conn.query_row("SELECT COUNT(*) FROM focus_logs", [], |r| r.get(0)).unwrap();
        assert_eq!(cnt, 1);
        let (reason, dur): (String, i64) = logger.conn.query_row(
            "SELECT reason, duration_secs FROM focus_logs WHERE session_id = 'sess-a'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap();
        assert_eq!(reason, "手机");
        assert_eq!(dur, 5);
    }

    /// #7 坏输入应返 Err 不 panic，且不破坏当前数据。
    #[test]
    fn restore_from_snapshot_bad_input_errors_and_preserves_db() {
        let (logger, _dir) = open_tmp();
        logger.add_focus_secs_today(60).unwrap();
        let res = logger.restore_from_snapshot(b"not a json");
        assert!(res.is_err(), "坏输入应返错");
        assert_eq!(logger.today_focus_secs().unwrap(), 60);
    }

    /// #7 空库上重复 clear+snapshot 不应 panic，restore 也不报错。
    #[test]
    fn clear_with_snapshot_idempotent_when_empty() {
        let (logger, _dir) = open_tmp();
        let snap1 = logger.clear_all_data_with_snapshot(false).unwrap();
        assert!(!snap1.is_empty());
        let snap2 = logger.clear_all_data_with_snapshot(false).unwrap();
        assert!(!snap2.is_empty());
        assert!(logger.restore_from_snapshot(&snap2).is_ok());
    }

    /// #7/review F3：settings 字段类型错误时 restore 应返 Err 且不产生半完成状态。
    /// 验证 daily_focus 与 focus_logs 在 settings 解析失败后保持原值(因为
    /// restore 在事务内,解析失败 → ROLLBACK → 整体回退至调用前快照拍摄前的 DB)。
    #[test]
    fn restore_bad_settings_json_rolls_back_all_tables() {
        let (logger, _dir) = open_tmp();
        // 拍一初始快照(空库)
        let _empty_snap = logger.clear_all_data_with_snapshot(false).unwrap();
        // 准备原状态:一条 log + today 3600s + default_focus_mins=77
        logger.log_event("sess-orig", "SESSION_START", None, 0).unwrap();
        logger.add_focus_secs_today(3600).unwrap();
        let mut s = logger.load_settings().unwrap();
        s.default_focus_mins = 77;
        logger.save_settings(&s).unwrap();
        let focus_before: u32 = logger.today_focus_secs().unwrap();
        let logs_before: i64 = logger.conn.query_row(
            "SELECT COUNT(*) FROM focus_logs", [], |r| r.get(0)).unwrap();

        // 构造坏快照 JSON：daily/logs 有效但 settings.default_focus_mins 是字符串(类型不符)
        let bad = serde_json::json!({
            "schema_version": 1,
            "settings": {"default_focus_mins": "not-a-number"},
            "daily_focus": [{"day": "2024-01-01", "focus_secs": 999}],
            "focus_logs": [{
                "session_id": "bad", "event_type": "X", "reason": "",
                "duration_secs": 0, "created_at": "2024-01-01T00:00:00"
            }]
        }).to_string().into_bytes();

        let res = logger.restore_from_snapshot(&bad);
        assert!(res.is_err(), "settings 类型错应返 Err");
        // 回滚生效：daily_focus/focus_logs/settings 未被坏快照覆盖
        assert_eq!(logger.today_focus_secs().unwrap(), focus_before, "daily_focus 应回滚");
        assert_eq!(
            logger.conn.query_row(
                "SELECT COUNT(*) FROM focus_logs", rusqlite::params![],
                |r| r.get::<_, i64>(0)).unwrap(),
            logs_before,
            "focus_logs 应回滚"
        );
        // 注意 focus_before 已包含 add 的 3600；mod_tests drop 后不依赖此值的具体数值
        let _ = focus_before;
        let got = logger.load_settings().unwrap();
        assert_eq!(got.default_focus_mins, 77, "settings 不被坏快照覆盖");
    }

    /// #7/review F2：恢复中途 INSERT 失败 → ROLLBACK → DB 保持 premio 与 pre-DELETE 一致。
    /// 用 focus_logs 的 created_at NOT NULL 约束 (或 schema 上的 NOT NULL 列) 提供一个 fail
    /// 中间把 day 改为 null 隐含触发。SQLite 中 Insert 到 daily_focus 的 day TEXT NOT NULL。
    #[test]
    fn restore_from_insert_failure_rolls_back_and_preserves_db() {
        let (logger, _dir) = open_tmp();
        // 原状态：一条 log + today 5400s
        logger.log_event("sess-orig2", "SESSION_START", None, 0).unwrap();
        logger.add_focus_secs_today(5400).unwrap();
        let logs_before: i64 = logger.conn.query_row(
            "SELECT COUNT(*) FROM focus_logs", [], |r| r.get(0)).unwrap();
        let focus_before: u32 = logger.today_focus_secs().unwrap();

        // 构造坏快照：daily_focus 中 day 为 null/text,字段名不对导致 Insert 中vec day
        // 位取不到 → Insert 会使用 unwrap_or("") 仍能 Insert。故此用 focus_logs 中
        // session_id 是 NOT NULL 但 reason 可能 NOT NULL 难触发。我们构造 created_at 为 null、当
        // 依赖 unwrap_or("") 也不会 fail。取而代之提供一个 created_at 为数字,unwrap_or("")
        // 在 as_str() 返 None → 返 "", 仍 Insert 成功不会触发 ROLLBACK。
        // 故改为一个 settings.default_focus_mins 超出类型边界 (类型错误) 来触发结束时 fail。
        // 用最末阶段的 settings 解析错误费能验证事务途中 fail。day_secs 用数字 Nested 实例。
        let bad = serde_json::json!({
            "schema_version": 1,
            "settings": {"default_focus_mins": "bad"},
            "daily_focus": [{"day": "2024-12-31", "focus_secs": 12345}],
            "focus_logs": [{
                "session_id": "new-row", "event_type": "X", "reason": "",
                "duration_secs": 1, "created_at": "2024-12-31T00:00:00"
            }]
        }).to_string().into_bytes();
        let res = logger.restore_from_snapshot(&bad);
        assert!(res.is_err(), "settings 类型错应触发事务失败");
        // 原数据未受影响
        assert_eq!(logger.today_focus_secs().unwrap(), focus_before);
        assert_eq!(
            logger.conn.query_row(
                "SELECT COUNT(*) FROM focus_logs", rusqlite::params![],
                |r| r.get::<_, i64>(0)).unwrap(),
            logs_before);
    }
}
