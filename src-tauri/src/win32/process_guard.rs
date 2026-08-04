use serde::{Deserialize, Serialize};
use std::collections::HashSet;

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
}

fn is_likely_distraction(name: &str) -> bool {
    const HINTS: &[&str] = &[
        "wechat",
        "weixin",
        "qq.exe",
        "discord",
        "steam",
        "game",
        "telegram",
        "slack",
        "spotify",
        "chrome.exe",
        "msedge.exe",
        "firefox.exe",
        "douyin",
        "tiktok",
        "bilibili",
    ];
    // 若白名单未包含浏览器，浏览器也会被报——符合「白名单外即违规」；
    // HINTS 仅用于降低系统服务噪音：非 hint 且非常见名则忽略。
    if HINTS.iter().any(|h| name.contains(h)) {
        return true;
    }
    // 未知 exe：保守不报，避免海量 svchost 变种
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
