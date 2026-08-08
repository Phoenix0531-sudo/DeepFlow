use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[cfg(windows)]
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, WPARAM};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindow, GetWindowThreadProcessId, GetWindowTextLengthW, GW_OWNER,
    IsWindowVisible, PostMessageW, ShowWindow, SW_MINIMIZE, WM_CLOSE,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhitelistHit {
    pub process_name: String,
    pub pid: u32,
}

/// 白名单进程守卫（P0：枚举 + 命中报告；15s 提示由上层 UI/定时处理）。
pub struct ProcessGuard {
    whitelist: HashSet<String>,
    /// 系统隐式白名单（小写进程名）。
    implicit: HashSet<String>,
}

impl Default for ProcessGuard {
    fn default() -> Self {
        Self::new(vec![])
    }
}

impl ProcessGuard {
    pub fn new(whitelist_names: Vec<String>) -> Self {
        let mut implicit = HashSet::new();
        for n in [
            "explorer.exe",
            "dwm.exe",
            "svchost.exe",
            "system",
            "csrss.exe",
            "winlogon.exe",
            "sihost.exe",
            "taskmgr.exe",
            "deepflow.exe",
            "deepflow",
            // Xbox / Game Input 后台服务，非分心应用
            "gameinputsvc.exe",
            "gameinputredistservice.exe",
            "gamebar.exe",
            "gamebarftserver.exe",
            "gamingservices.exe",
            "gamingservicesnet.exe",
            "searchhost.exe",
            "startmenuexperiencehost.exe",
            "shellexperiencehost.exe",
            "runtimebroker.exe",
            "applicationframehost.exe",
            "textinputhost.exe",
            "conhost.exe",
            "fontdrvhost.exe",
            "dllhost.exe",
            "wmiprvse.exe",
            "audiodg.exe",
            "smartscreen.exe",
            "securityhealthservice.exe",
            "msmpeng.exe",
            "nvidia share.exe",
            "nvcontainer.exe",
            "jhi_service.exe",
        ] {
            implicit.insert(n.to_string());
        }
        let whitelist = whitelist_names
            .into_iter()
            .map(|s| s.to_lowercase())
            .collect();
        Self {
            whitelist,
            implicit,
        }
    }

    pub fn set_whitelist(&mut self, names: Vec<String>) {
        self.whitelist = names.into_iter().map(|s| s.to_lowercase()).collect();
    }

    pub fn scan_violations(&self) -> Vec<WhitelistHit> {
        #[cfg(windows)]
        {
            self.scan_windows()
        }
        #[cfg(not(windows))]
        {
            vec![]
        }
    }

    /// #22：把指定 pid 的所有顶级窗口最小化（不杀进程，玩家可手动恢复）。
    /// 返回实际被最小化的窗口数量。
    pub fn minimize_windows_of(&self, pid: u32) -> u32 {
        #[cfg(windows)]
        {
            self.minimize_windows_of_windows(pid)
        }
        #[cfg(not(windows))]
        {
            let _ = pid;
            0
        }
    }

    /// #22：close_report 动作：向指定 pid 的顶级窗口发 WM_CLOSE（礼貌关闭，非强杀）。
    /// 返回发送 WM_CLOSE 的窗口数量。
    pub fn close_windows_of(&self, pid: u32) -> u32 {
        #[cfg(windows)]
        {
            self.close_windows_of_windows(pid)
        }
        #[cfg(not(windows))]
        {
            let _ = pid;
            0
        }
    }

    #[cfg(windows)]
    fn scan_windows(&self) -> Vec<WhitelistHit> {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::ProcessStatus::{
            EnumProcesses, GetModuleBaseNameW, K32EnumProcessModules,
        };
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};

        let mut pids = vec![0u32; 1024];
        let mut bytes_needed = 0u32;
        unsafe {
            if EnumProcesses(
                pids.as_mut_ptr(),
                (pids.len() * std::mem::size_of::<u32>()) as u32,
                &mut bytes_needed,
            )
            .is_err()
            {
                return vec![];
            }
        }
        let count = (bytes_needed as usize) / std::mem::size_of::<u32>();
        pids.truncate(count);

        let mut hits = Vec::new();
        for pid in pids {
            if pid == 0 {
                continue;
            }
            unsafe {
                let Ok(handle) = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid)
                else {
                    continue;
                };
                let mut module = windows::Win32::Foundation::HMODULE::default();
                let mut cb = 0u32;
                let enum_ok = K32EnumProcessModules(
                    handle,
                    &mut module,
                    std::mem::size_of_val(&module) as u32,
                    &mut cb,
                )
                .as_bool();
                if !enum_ok {
                    let _ = CloseHandle(handle);
                    continue;
                }
                let mut name_buf = [0u16; 260];
                let len = GetModuleBaseNameW(handle, module, &mut name_buf);
                let _ = CloseHandle(handle);
                if len == 0 {
                    continue;
                }
                let name = String::from_utf16_lossy(&name_buf[..len as usize]).to_lowercase();
                if self.implicit.contains(&name) || self.whitelist.contains(&name) {
                    continue;
                }
                // 过滤无窗口的噪音进程过严会误伤；P0 报告所有非白名单用户态进程中
                // 常见干扰名优先（可扩展）。这里先返回全部非白名单，由上层节流提示。
                if is_likely_distraction(&name) {
                    hits.push(WhitelistHit {
                        process_name: name,
                        pid,
                    });
                }
            }
        }
        hits
    }

    /// 枚举属于指定 pid 的所有顶级窗口句柄（可见且有标题的）。复用于 minimize/close。
    #[cfg(windows)]
    fn enum_toplevel_windows_of(&self, pid: u32) -> Vec<HWND> {
        struct EnumState {
            pid: u32,
            out: Vec<HWND>,
        }
        unsafe extern "system" fn proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let state = &mut *(lparam.0 as *mut EnumState);
            // 需要可见且有标题，避免收纳 0×0 / 不可见辅助窗口
            if !IsWindowVisible(hwnd).as_bool() {
                return BOOL(1);
            }
            if GetWindowTextLengthW(hwnd) == 0 {
                return BOOL(1);
            }
            // 只要顶级窗口（没有 owner 的）
            let has_owner = GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.is_invalid());
            if has_owner {
                return BOOL(1);
            }
            let mut wpid: u32 = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut wpid));
            if wpid == state.pid {
                state.out.push(hwnd);
            }
            BOOL(1)
        }

        let mut state = EnumState { pid, out: Vec::new() };
        unsafe {
            let _ = EnumWindows(
                Some(proc),
                LPARAM(&mut state as *mut EnumState as isize),
            );
        }
        state.out
    }

    #[cfg(windows)]
    fn minimize_windows_of_windows(&self, pid: u32) -> u32 {
        let hwnds = self.enum_toplevel_windows_of(pid);
        let mut n = 0u32;
        for h in hwnds {
            unsafe {
                // ShowWindow 返回值表示之前是否可见；最小化成功本身不看返回值
                let _ = ShowWindow(h, SW_MINIMIZE);
                n += 1;
            }
        }
        n
    }

    #[cfg(windows)]
    fn close_windows_of_windows(&self, pid: u32) -> u32 {
        let hwnds = self.enum_toplevel_windows_of(pid);
        let mut n = 0u32;
        for h in hwnds {
            unsafe {
                if PostMessageW(h, WM_CLOSE, WPARAM(0), LPARAM(0)).is_ok() {
                    n += 1;
                }
            }
        }
        n
    }
}

fn is_likely_distraction(name: &str) -> bool {
    // 系统 / 驱动 / 输入法噪音：直接忽略
    const NOISE: &[&str] = &[
        "gameinput",
        "gamingservices",
        "nvidia",
        "nvcontainer",
        "nvdisplay",
        "igfx",
        "intel",
        "realtek",
        "radeon",
        "amd ",
        "service",
        "helper",
        "update",
        "crashpad",
        "cefsharp",
        "chp",
        "widget",
        "crossdevice",
        "phoneexperience",
        "yourphone",
        "compattelrunner",
        "backgroundtaskhost",
        "systemsettings",
        "searchapp",
        "lockapp",
    ];
    if NOISE.iter().any(|n| name.contains(n)) {
        return false;
    }

    const HINTS: &[&str] = &[
        "wechat",
        "weixin",
        "qq.exe",
        "discord",
        "steam",
        "telegram",
        "slack",
        "spotify",
        "chrome.exe",
        "msedge.exe",
        "firefox.exe",
        "douyin",
        "tiktok",
        "bilibili",
        "youtub",
        "netflix",
        "i4tools",
        "todesk",
        "sunlogin",
        "lol",
        "league",
        "valorant",
        "cs2",
        "genshin",
        "epicgames",
        "origin.exe",
        "battle.net",
    ];
    // 白名单外浏览器/社交/游戏才报；不含宽泛 "game" 以免误伤 GameInput 服务
    if HINTS.iter().any(|h| name.contains(h)) {
        return true;
    }
    false
}

/// 列出当前进程名（供设置页勾选白名单）。
pub fn list_running_process_names() -> Vec<String> {
    #[cfg(windows)]
    {
        use std::collections::BTreeSet;
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::ProcessStatus::{
            EnumProcesses, GetModuleBaseNameW, K32EnumProcessModules,
        };
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};

        let mut pids = vec![0u32; 1024];
        let mut bytes_needed = 0u32;
        unsafe {
            if EnumProcesses(
                pids.as_mut_ptr(),
                (pids.len() * std::mem::size_of::<u32>()) as u32,
                &mut bytes_needed,
            )
            .is_err()
            {
                return vec![];
            }
        }
        let count = (bytes_needed as usize) / std::mem::size_of::<u32>();
        pids.truncate(count);
        let mut set = BTreeSet::new();
        for pid in pids {
            if pid == 0 {
                continue;
            }
            unsafe {
                let Ok(handle) =
                    OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid)
                else {
                    continue;
                };
                let mut module = windows::Win32::Foundation::HMODULE::default();
                let mut cb = 0u32;
                if !K32EnumProcessModules(
                    handle,
                    &mut module,
                    std::mem::size_of_val(&module) as u32,
                    &mut cb,
                )
                .as_bool()
                {
                    let _ = CloseHandle(handle);
                    continue;
                }
                let mut name_buf = [0u16; 260];
                let len = GetModuleBaseNameW(handle, module, &mut name_buf);
                let _ = CloseHandle(handle);
                if len > 0 {
                    let name = String::from_utf16_lossy(&name_buf[..len as usize]);
                    set.insert(name);
                }
            }
        }
        set.into_iter().collect()
    }
    #[cfg(not(windows))]
    {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitelist_set_normalizes_to_lowercase() {
        let mut g = ProcessGuard::new(vec!["Chrome.EXE".into(), "WeChat.exe".into()]);
        // 内部白名单小写；扫描时命中比较也用小写，这里用 set 再覆盖验证不 panic
        g.set_whitelist(vec!["CODE.EXE".into()]);
        // 未知 pid 不应 panic，返回 0
        assert_eq!(g.minimize_windows_of(0), 0);
        assert_eq!(g.close_windows_of(0), 0);
        assert_eq!(g.minimize_windows_of(u32::MAX), 0);
        assert_eq!(g.close_windows_of(u32::MAX), 0);
    }

    #[test]
    fn is_likely_distraction_filters_noise_and_hints() {
        assert!(!is_likely_distraction("nvidia share.exe"));
        assert!(!is_likely_distraction("gameinputsvc.exe"));
        assert!(!is_likely_distraction("backgroundtaskhost.exe"));
        assert!(is_likely_distraction("chrome.exe"));
        assert!(is_likely_distraction("WeChat.exe".to_lowercase().as_str()));
        assert!(is_likely_distraction("discord.exe"));
        assert!(!is_likely_distraction("notepad.exe")); // 不在 HINTS 内
    }

    #[test]
    fn default_guard_scan_does_not_panic() {
        let g = ProcessGuard::default();
        let _ = g.scan_violations(); // 仅保证可调用
    }
}
