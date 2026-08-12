---
name: Feature Request
about: 提议一个新能力或行为改进
title: "[FEAT] <一句话主张>"
labels: ["feature", "triage"]
assignees: ["Phoenix0531-sudo"]
---

## 主张

<!-- 谁能从这件事得到什么?他们现在是怎么做的? -->

## 为什么不是已有的方案

<!-- 现有 FSM / Settings / 白名单 / 周报 已经做到什么程度,为什么不够? -->

## 提议

<!-- 一段机制想法。不强求实现细节,但请说清楚 L1/L2/L3 是哪一级、是否要新 IPC 命令、要不要新 settings 字段 -->

## 兼容性影响

- [ ] 要新增 / 改 IPC 命令 → 同步 `src/types/tauri-ipc.ts` 与 `src-tauri/src/lib.rs` invoke_handler
- [ ] 要改 settings schema → 考虑向后兼容 / 迁移
- [ ] 要改 FSM 状态或转移 → 加 regression test 锁定 effect
- [ ] 要改 SQLite 表结构 → 写迁移 + 回滚验证

## 备注

<!-- 任何其他想法 -->
