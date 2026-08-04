# DeepFlow

Windows 本地刷题专注壳（Tauri 2 + React 19）：主屏遮罩、进程白名单、休息债务、SQLite 日志；P1 接入视觉三级干预。

## 环境

- Rust `stable-x86_64-pc-windows-msvc`（`RUSTUP_HOME` / `CARGO_HOME` → `E:\1_Code\rust\...`）
- MSVC Build Tools（本机：`E:\1_Code\vs-buildtools`）
- Node 18+ / npm
- WebView2

编译前请在 **x64 Native Tools** 或先执行：

```bat
"E:\1_Code\vs-buildtools\VC\Auxiliary\Build\vcvars64.bat"
```

## 数据目录

`D:\3_Code_Projects\DeepFlow\data\`（db / logs / models / exports）

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

## 阶段

- **P0**（当前）：多窗、FSM、钩子、遮罩、白名单扫描、债务、设置、调试日志
- **P1**：摄像头 + ONNX 检测 + L1–L3
- **P2**：周报 PNG、模型增强
