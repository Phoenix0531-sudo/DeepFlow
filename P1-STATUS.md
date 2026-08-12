# P1 当前状态（磁盘为准）

更新时间：#14 / #15 / #16 收口 + CameraPreview 语法修复

## 编译 / 构建

```text
cargo check (src-tauri)
→ 0 errors · dead_code warnings only

cargo test sliding_window
→ 3 passed（含墙钟累计独立于 fps）

npm run build
→ tsc + vite OK
```

## #14 vision detection failure（墙钟 hold）

- `sliding_window.rs`：防抖仍用帧计数；latched 后用 `Instant` 墙钟累计 hold
- `pipeline.rs`：降采样 process_every=2，hold 不再依赖假定 fps
- 测试模式 `for_test_mode()`：debounce=2 / leave=0.6s
- 单测：`latches_and_reports_hold` / `release_resets_hold` / `wall_clock_accumulates_independent_of_fps`

## #15 test 时 camera preview

- `CameraPreview`：修复 `const token = bootToken.current` 损坏（曾写成非法语法，前端无法编译）
- Main 侧栏：`test_mode || vision.running` 时显示预览；test 且未 running 时 autoStart
- Overlay：测试预览面板 + hold/检测状态
- Setup：步骤 1–3 左侧常驻预览 + ROI

## #16 floating clock overflow

- 窗体：`280×220` → `260×168`（min 220×140）
- UI 重排：单行头（标题+原因截断）/ 大计时居中 / 债务 chip / 并排「恢复|跳过」
- `overflow-hidden` + shrink 分区，避免按钮被裁切

## 此前已完成（#1–#7 等）

- 卡死退出：force_exit / destroy overlay / Idle 双 ESC
- 音效 WebAudio：chime / severe / inject
- 测试注入 L1/L2/L3 + 退出；阈值 3/6/9/1
- UI 去 emoji + lucide；白名单过滤；债务下限预估

## 验证步骤

1. 设置勾选「测试模式」→ 保存
2. 主界面右侧应有摄像头预览；注入 L1/L2/L3 听提示音
3. 真机亮屏手机：约 3s L1 / 6s L2 / 9s L3（墙钟，不依赖帧率）
4. 临时休息：小闹钟 260×168 无溢出，恢复/跳过可点
5. Overlay 输入「测试」→ 立即回空闲

## 仍属 P2 / 未做

- 内置语音识别
- 正式安装包路径策略细化
