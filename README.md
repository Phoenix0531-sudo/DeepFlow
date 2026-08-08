# DeepFlow

Windows 本地刷题专注壳（Tauri 2 + React 19）：主屏遮罩、进程白名单、休息债务、SQLite 日志；P1 接入视觉三级干预。

## 环境

- Rust `stable-x86_64-pc-windows-msvc`（`RUSTUP_HOME` / `CARGO_HOME` → `E:\1_Code\rust\...`）
- MSVC Build Tools（本机：`E:\1_Code\vs-buildtools`）
- Node 18+ / npm
- WebView2
- （可选）Python 3.12 + ultralytics：导出 YOLO ONNX

编译前请在 **x64 Native Tools** 或先执行：

```bat
"E:\1_Code\vs-buildtools\VC\Auxiliary\Build\vcvars64.bat"
```

## 数据目录

按下列优先级选择（实现见 `src-tauri/src/lib.rs::resolve_data_dir`）：

1. 环境变量 `DEEPFLOW_DATA_DIR` 指定的绝对路径
2. 便携模式：可执行文件旁的 `data/`（检测 `DEEPFLOW_PORTABLE` 或标记文件 `portable.flag` / `.portable` / `DeepFlow.portable`）
3. 开发构建：项目根 `data/`（exe 路径含 `target/debug|release`）
4. 正式安装：`%LOCALAPPDATA%\DeepFlow\data`

子目录：`db/`（SQLite）、`logs/`、`models/`（ONNX，缺失时从 `seed_models/` 自动复制）、`exports/`（周报 PNG）。

前端「设置 → 路径信息」可调 `get_path_info` / `reveal_path`。

模型路径：`<data>/models/yolo11n.onnx`（无模型时回退启发式检测器）。

### #13 路径策略主题验证手册

四种模式需在各自环境下手工验收，重点检查 `get_path_info` 返回的 `mode` 字段与写入位置：

| 模式 | 触发条件 | 预期 mode | 验证步骤 |
| ---- | ------- | -------- | ------- |
| env | `DEEPFLOW_DATA_DIR=D:/tmp/df` | `env` | 启动后看设置页数据目录、写一条记录后确认出现在 D:/tmp/df |
| 便携 | exe 旁放 `portable.flag` | `portable` | exe 旁出现 data/；换机拷贝后路径跟随 exe |
| 开发 | exe 路径在 `target/debug|release` | `dev` | 仓库 data/ 被使用 |
| 安装 | `cargo tauri build` 生成的安装包安装 | `install` | `%LOCALAPPDATA%\DeepFlow\data` 被创建且写入 |

### #48 真装包路径验证清单（手工）

> 在正式安装包上逐项打勾；开发态 `cargo run` 不能代替此表。

1. **构建**
   - [ ] `npm run tauri build` 成功，产物含 `DeepFlow_x.y.z_x64-setup.exe`
2. **干净机 / 干净用户安装**
   - [ ] 安装后启动，设置页「数据目录」=`%LOCALAPPDATA%\DeepFlow\data`
   - [ ] `get_path_info.mode` = `install`
3. **写入验证**
   - [ ] 完成一次专注会话后，`%LOCALAPPDATA%\DeepFlow\data\deepflow.db` 存在且增长
   - [ ] 导出周报 PNG 落在 `%LOCALAPPDATA%\DeepFlow\data\exports\`
4. **便携对照**
   - [ ] 把安装产物旁放 `portable.flag` 再启动，mode 切到 `portable`，数据写到 exe 旁 `data/`
5. **环境变量覆盖**
   - [ ] 设置 `DEEPFLOW_DATA_DIR=D:\\tmp\\df` 启动，mode=`env`，写入该目录
6. **系统集成（#23/#29/#33）**
   - [ ] 开启「登录自启」后，任务管理器「启动」可见 DeepFlow
   - [ ] 「测试通知」弹出系统通知中心 toast
   - [ ] 「检查更新」在未配置 endpoint/pubkey 时提示「未配置」（非「已是最新」）

```bat
python scripts/download_yolo_onnx.py
```

## 开发

```bat
npm.cmd install
npm.cmd run tauri dev
```

仅检查 Rust：

```bat
cd src-tauri
cargo check
```

前端构建：

```bat
npm.cmd run build
```

## 阶段

- **P0**（done）：多窗、FSM、托盘、遮罩、白名单扫描、债务、设置、调试日志
- **P1**（done）：摄像头 + ONNX 检测 + L1–L3 + Setup 预览/ROI、滑窗墙钟 hold
- **P2**（done）：周报 PNG 导出（`export_weekly_report_png`，时间戳文件名）、紧急热键可配（设置页四选一：双击 ESC / F9 / Ctrl+Shift+E / Ctrl+Alt+Q，保存后热更新）、内置语音识别（Web Speech API，mic 按钮）、数据目录四模式策略 + 便携标记/seed 模型复制
- **P3**（进行中）：进程白名单强制最小化/关闭（`whitelist_action`）、摄像头下拉统一、聚焦时长上下限、Setup 可配债务下限、白名单进程搜索、债务结算、模型自管理 UI（`list_models`/`reseed_models`）、周报历史周查看、L3 原因回看、托盘菜单扩展、CSP 收紧、浮钟置顶可调、开机自启/系统通知/自动更新骨架、关于页版本信息、通知全覆盖（到点/会话结束/白名单/L3）

## 紧急退出热键

设置页「紧急热键」字段选 4 种之一；`save_settings` 后 `keyboard_hook` 原子热更新（无需重启）。

**副作用**：紧急退出会立即结束当前专注会话，该次会话不计入今日专注；若处于 L3 颂罚，会写入 `EMERGENCY_EXIT` 日志事件（`focus_logs` 表，`event_type='emergency_exit'`，带 `reason='紧急退出'`），并在周报中以「中断」统计。同时隐藏浮钟、复位遮罩契约。

## 干预约定（摘要）

- 债务下限：3 分钟（180 秒）
- L1：持机 60 秒 + 30 秒观察 +「知道了」
- L2：持机 120 秒，橙色全屏 + 声音，可忽略
- L3：持机 180 秒或关摄像头，红色锁屏直到输入原因
- 黑屏手机（亮度 < 40 且无手-机重叠）不算操作

## 测试模式

1. 设置勾选「测试模式」并保存（阈值 L1=3s / L2=6s / L3=9s，放下约 1s 恢复）
2. 主界面或 Overlay：**注入 L1/L2/L3** 可无需手机验证 FSM
3. Overlay 对话框输入 **「测试」**（或 test / 退出测试）→ 立即结束会话
4. L2/L3 有 WebAudio 提示音；L3 持续不配合会加重（测试约 5s）

## 测试

```bat
cd src-tauri
cargo test
```

现覆盖：键盘热键解析、启发式检测器、`is_operating_phone` 边界、周报 PNG 生成、SQLite 日志/周报聚合、路径策略判定、FSM 集成冲烟（Start/L1/L3/原因/紧急退出）、历史周聚合、L3 原因回看。

## 主要 IPC 命令

设置：`get_settings` / `save_settings`、路径：`get_path_info` / `reveal_path`、会话：`start_focus_session` / `stop_session` / `request_temporary_pause` / `resume_focus_session` / `skip_debt_and_resume`、周报：`get_weekly_report` / `get_weekly_report_at` / `export_weekly_report_png`、L3 原因：`get_l3_reasons`、模型管理：`list_models` / `reseed_models`、视觉：`get_vision_status` / `restart_vision` / `get_available_cameras`、进程：`list_running_processes`、测试注入：`test_inject_level` / `test_exit_session`、 Setup：`check_setup` / `start_setup` / `open_setup_window`。
