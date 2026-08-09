//! #6 白名单强制策略分类与目标筛选。
//!
//! 上层 scan 命中违规进程后，根据 `SettingsRecord.whitelist_action` 决定对每条命中
//! 执行的动作。本模块负责把字符串配置映射到可测的 enum，并选出真正要执行强制动作
//! 的目标（report 不产生执行目标;minimize / close_report 对全部命中执行）。
//!
//! 真 GUI 端到端验证（启动外部进程 → scan → minimize/close → 观察效果）依赖真机
//! GUI 自动化,无法在 cargo test 闭环;模块IFIED 在内核做抽象 + 单测,真机步骤
//! 见 docs/whitelist-e2e.md。

use super::process_guard::WhitelistHit;

/// 强制策略类型。对应 settings.whitelist_action 取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhitelistActionKind {
    /// 仅报告命中事件（默认）。不主动动窗口。
    Report,
    /// 礼貌最小化命中进程的所有顶层窗口（不杀进程）。
    Minimize,
    /// 礼貌关闭命中进程的所有顶层窗口（不杀进程）。close_report 走本分支。
    CloseReport,
}

impl WhitelistActionKind {
    /// 是否实际执行强制动作（即需要拿 ProcessGuard 调 minimize/close）。Report 返回 false。
    pub fn should_enforce(self) -> bool {
        matches!(self, Self::Minimize | Self::CloseReport)
    }
}

/// 把 settings.whitelist_action 字符串映射为 enum。未知值归为 Report（最保守）。
/// 输入会被 to_lowercase 处理,大小写不敏感。
pub fn classify_whitelist_action(action: &str) -> WhitelistActionKind {
    match action.to_lowercase().as_str() {
        "minimize" => WhitelistActionKind::Minimize,
        "close_report" | "close" => WhitelistActionKind::CloseReport,
        _ => WhitelistActionKind::Report,
    }
}

/// 在命中集合中筛出应被强制动作处理的目标。Report 返回空 Vec;其余返回全部命中。
/// 返回引用,避免多余 clone;调用方按需取 pid/process_name 调 minimize/close。
pub fn select_targets<'a>(hits: &'a [WhitelistHit], kind: WhitelistActionKind) -> Vec<&'a WhitelistHit> {
    if kind.should_enforce() {
        hits.iter().collect()
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(name: &str, pid: u32) -> WhitelistHit {
        WhitelistHit {
            process_name: name.to_string(),
            pid,
        }
    }

    #[test]
    fn classify_known_strings() {
        assert_eq!(classify_whitelist_action("report"), WhitelistActionKind::Report);
        assert_eq!(classify_whitelist_action("minimize"), WhitelistActionKind::Minimize);
        assert_eq!(classify_whitelist_action("close_report"), WhitelistActionKind::CloseReport);
        // close 也归 close_report
        assert_eq!(classify_whitelist_action("close"), WhitelistActionKind::CloseReport);
    }

    #[test]
    fn classify_case_insensitive() {
        assert_eq!(classify_whitelist_action("MINIMIZE"), WhitelistActionKind::Minimize);
        assert_eq!(classify_whitelist_action("Close_Report"), WhitelistActionKind::CloseReport);
        assert_eq!(classify_whitelist_action("RePoRt"), WhitelistActionKind::Report);
    }

    #[test]
    fn classify_unknown_falls_back_to_report() {
        // "kill" / "destroy" / "" 等未配置取值 → 安全默认 Report
        assert_eq!(classify_whitelist_action("kill"), WhitelistActionKind::Report);
        assert_eq!(classify_whitelist_action(""), WhitelistActionKind::Report);
        assert_eq!(classify_whitelist_action("anything_else"), WhitelistActionKind::Report);
    }

    #[test]
    fn should_enforce_only_for_minimize_and_close() {
        assert!(!WhitelistActionKind::Report.should_enforce());
        assert!(WhitelistActionKind::Minimize.should_enforce());
        assert!(WhitelistActionKind::CloseReport.should_enforce());
    }

    #[test]
    fn select_targets_empty_for_report() {
        let hits = vec![hit("chrome.exe", 100), hit("wechat.exe", 200)];
        assert_eq!(select_targets(&hits, WhitelistActionKind::Report).len(), 0);
    }

    #[test]
    fn select_targets_all_for_minimize_and_close() {
        let hits = vec![hit("chrome.exe", 100), hit("wechat.exe", 200)];
        let m = select_targets(&hits, WhitelistActionKind::Minimize);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].pid, 100);
        assert_eq!(m[1].process_name, "wechat.exe");
        let c = select_targets(&hits, WhitelistActionKind::CloseReport);
        assert_eq!(c.len(), 2);
        assert_eq!(c[1].pid, 200);
    }

    #[test]
    fn select_targets_empty_when_hits_empty() {
        let empty: Vec<WhitelistHit> = vec![];
        assert_eq!(select_targets(&empty, WhitelistActionKind::Minimize).len(), 0);
        assert_eq!(select_targets(&empty, WhitelistActionKind::CloseReport).len(), 0);
        // dedup 在入参已做,这里不需重 dup
        let hits = vec![hit("chrome.exe", 100), hit("chrome.exe", 100)];
        assert_eq!(select_targets(&hits, WhitelistActionKind::Minimize).len(), 2);
    }
}
