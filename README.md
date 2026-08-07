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

`D:\3_Code_Projects\DeepFlow\data\`（db / logs / models / exports）

模型路径：`data/models/yolo11n.onnx`（无模型时回退启发式检测器）。

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

- **P0**：多窗、FSM、托盘、遮罩、白名单扫描、债务、设置、调试日志
- **P1**（进行中）：摄像头 + ONNX 检测 + L1–L3 + Setup 预览/ROI
- **P2**：周报 PNG、模型增强

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
