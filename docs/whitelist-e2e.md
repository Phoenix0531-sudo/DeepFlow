# 白名单强制端到端手工验证清单（#6）

> 真机 GUI 端到端不在 cargo test 闭环内，本文件描述给前端 / 测试人员的手动步骤。

内核层抽象（`src-tauri/src/win32/whitelist_action.rs`）已覆盖：
- `classify_whitelist_action(&str) -> WhitelistActionKind`（report / minimize / close_report / close，未知归 report）
- `select_targets(&[WhitelistHit], kind) -> Vec<&WhitelistHit>`（report 返回空，其余返回全部）
- 通过 7 个单元测试在 `cargo test --lib` 闭环内。

本页描述的是**真机 GUI** 的端到端验证：是否真的最小化 / 关闭了被命中进程的窗口。

## 前置准备

1. 安装版 DeepFlow（或 cargo run dev）。
2. 准备 2 个易识别的"违规"进程：
   - 例如 Notepad（`notepad.exe`），打开一个窗口。
   - 例如 Chromium / Chrome（`chrome.exe`），打开一个普通窗口。
3. 确保 Notepad / Chrome **不在**白名单里。在 DeepFlow 设置页把白名单清空或确认不含这俩。
4. 在 settings 里把摄像头自动检测先关闭（避免 vision 抢动作），或确认允许静默运行。

## 用例 A：whitelist_action = report（默认，只报告）

1. 设置 → 高级 → `whitelist_action = report`（或留空）。
2. 启动 notepad 与 chrome 各一个窗口。
3. 触发一次白名单扫描（15s 定时器 / 手动重启 app + 等待）。
4. 期望：
   - 主屏遮罩/通知弹出 "检测到：chrome.exe, notepad.exe"。
   - **不**最小化、**不**关闭外部窗口。
   - 日志 `whitelist_action=Report`（debug 模式下可见；report 不打 debug 行）。

## 用例 B：whitelist_action = minimize（礼貌最小化）

1. 设置 → 高级 → `whitelist_action = minimize`，保存。
2. 启动 notepad 与 chrome 各一个窗口，置于前台可见。
3. 等待扫描或触发扫描。
4. 期望：
   - notepad 与 chrome 的顶层窗口均被最小化到任务栏。
   - 进程**未**被杀（任务管理器可见 chrome.exe / notepad.exe 仍在）。
   - debug 日志样例：`whitelist_action=Minimize chrome.exe pid=XXXX windows=1`。
5. 反例检查：白名单里的进程窗口**不应**被最小化（与未在白名单里的进程并排放，验证只命中违规者）。

## 用例 C：whitelist_action = close_report（礼貌关闭）

1. 设置 → 高级 → `whitelist_action = close_report`，保存。
2. 启动 notepad 与 chrome 各一个窗口。
3. 等待扫描或触发扫描。
4. 期望：
   - notepad 与 chrome 的顶层窗口被关闭（收到 WM_CLOSE，应用自处理关闭）。
   - 进程**未**被强制终止（chrome 多窗口场景下 chrome.exe 主进程可能不一定退出，但该窗口应消失）。
   - debug 日志样例：`whitelist_action=CloseReport chrome.exe pid=XXXX windows=1`。
5. 边界：若某窗口弹出"是否保存"对话框（如未保存的 notepad），不应被强制 kill。这是"礼貌"关闭的预期行为。

## 用例 D：进程无顶层窗口 / 已最小化

1. `whitelist_action = minimize`，启动一个没有窗口的脚本宿主进程（如待机中的 explorer 后台）。
2. 扫描后 `minimize_windows_of(pid)` 返回 0（debug 日志 `windows=0`），不应 panic。
3. 已最小化的窗口不应再次触发"恢复到最小化"操作（无副作用）。

## 用例 E：配置未知值回到 report

1. 通过 dev 工具或直接改 settings 把 `whitelist_action` 设为 `"kill"`（未支持取值）。
2. 扫描后应走 report 分支，不产生强制动作。
3. 单元测试 `classify_unknown_falls_back_to_report` 已覆盖字符串 → enum 的回退。

## 通过判定

- A 无强制动作；B 最小化且不杀；C 关闭窗口且不杀；D 不 panic；E 未知值安全。
- 上述 5 个都满足 → #6 白名单强制端到端验证通过。
