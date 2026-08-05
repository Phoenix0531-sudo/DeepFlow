# P1 编译修复记录

## 修复时间
2025-01-XX（接手旧会话损坏后的第一轮修复）

## 问题诊断

### 初始状态
- P0 五件套已在 commit `691dc24` 完成并通过编译
- P1 视觉管线代码已写入但存在编译阻塞：
  - `ndarray` 版本冲突（DeepFlow 0.16 vs ort 2.0.0-rc.13 需要 0.17）
  - ort 2.0.0-rc.13 API 变更导致字段访问错误

### 编译错误清单

#### 1. ort Session 字段私有化
```
error[E0616]: field `inputs` of struct `ort::session::Session` is private
   --> src\vision\detector.rs:131:21
    |
131 |             session.inputs.len(),
    |                     ^^^^^^ private field
```

**根因**：ort 2.0.0-rc.13 将 `Session.inputs` 和 `Session.outputs` 改为私有字段，需通过方法访问。

**修复**：
```rust
// 修复前
session.inputs.len()
session.outputs.len()

// 修复后
session.inputs().len()
session.outputs().len()
```

#### 2. Session builder 可变性
```
error[E0596]: cannot borrow `builder` as mutable, as it is not declared as mutable
   --> src\vision\detector.rs:123:23
```

**根因**：ort 2.0.0-rc.13 builder 方法消费 `self` 并返回新 builder，需要可变绑定或链式重新赋值。

**修复**：
```rust
// 修复前（链式调用）
let builder = Session::builder()?
    .with_optimization_level(GraphOptimizationLevel::Level3)?
    .with_intra_threads(2)?;

// 修复后（显式可变重新赋值）
let mut builder = Session::builder()?;
builder = builder.with_optimization_level(GraphOptimizationLevel::Level3)?;
builder = builder.with_intra_threads(2)?;
```

## 最终状态

### Cargo.toml 依赖
- `ndarray = "0.17"` （已升级以匹配 ort）
- `ort = { version = "2.0.0-rc.13", features = ["download-binaries", "ndarray"] }`
- `nokhwa = { version = "0.10", default-features = false, features = ["input-msmf"] }`
- `image = { version = "0.25", default-features = false, features = ["jpeg", "png"] }`

### 编译结果
```
✓ cargo check: 通过（10 个 dead_code 警告，预期内）
✓ npm run build: 通过（399 modules, 344KB JS, gzip 110KB）
```

### 未使用代码警告（预期内，P1 未完全接入）
- `AppState.data_dir` / `LocalLogger.db_path`
- `SystemFSM::subscribe` / `pending_debt_secs`
- `CameraController::is_running` / `list_cameras`
- `HybridDetector::model_search_paths`
- `MockDetector` struct
- `SlidingWindowFilter::current_hold_secs`

## 遗留工作（P1 集成）

### 未完成功能
1. **模型下载**：`scripts/download_yolo_onnx.py` 已存在但未执行，`data/models/` 目录可能无 ONNX 模型
2. **Vision Pipeline 启动**：`pipeline.rs` 已写入但未在 `app_state.rs` / `lib.rs` 中启动
3. **IPC 命令未连接**：
   - `get_vision_status` / `restart_vision` 已在 `commands.rs` 定义
   - 前端未实现对应 UI 调用
4. **DirectML 特性放弃**：当前使用 CPU 推理，未启用 GPU 加速（交接包记录 DirectML 曾尝试后放弃）

### 后续建议
1. 先运行 `python scripts/download_yolo_onnx.py` 下载模型（确认 Python 路径：`C:\Users\xianj\.local\bin\python3.12.exe`）
2. 在 `lib.rs` 中启动 `VisionPipeline` 并连接 FSM 事件
3. 前端 SetupWindow 接入摄像头预览与 ROI 选择
4. 测试 L1→L2→L3 完整干预闭环

## 重要约束（来自交接包）
- **债务下限**：3 分钟（180 秒）
- **L1 触发**：持机 60 秒 + 30 秒观察期 + 手动「知道了」按钮
- **L2 触发**：持机 120 秒，橙色全屏 + 声音，可忽略
- **L3 触发**：持机 180 秒或关闭摄像头，红色锁屏直到输入原因
- **视觉检测**：手机亮度 < 40 且无手机重叠 = 不算操作（黑屏手机放桌上）
- **模型回退链**：yolo11n.onnx → yolov8n.onnx → phone_detect.onnx → HeuristicDetector

## 编译命令（记录备查）
```powershell
# Rust 编译（需 x64 Native Tools 或 vcvars64.bat）
cd src-tauri
cargo check

# 前端构建
npm.cmd run build

# 开发模式（未在本次修复中测试）
npm.cmd run tauri dev
```
