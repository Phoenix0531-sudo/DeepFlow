<!-- PR Checklist — 提交前请勾选,加速 review 与 portfolio trace -->

## 改动类型

- [ ] feat(新功能)
- [ ] fix(故障修复)
- [ ] docs(文档 / README / P1-STATUS)
- [ ] test(测试覆盖)
- [ ] refactor(无功能变化的整理)
- [ ] chore(依赖 / 配置 / 脚本)

## Commit message 规范

- [ ] 首行 ≤ 72 字
- [ ] 前缀用 feat / fix / docs / test / refactor / chore
- [ ] body 说清"Why"而不是"What"

## 一致性

- [ ] 涉及 IPC 命令增删 → 同步 `src/types/tauri-ipc.ts` 与 `src-tauri/src/lib.rs` 的 `invoke_handler` 注册数组
- [ ] 涉及 settings 字段 → 考虑向后兼容(旧 settings 加载是否失败、是否需要 default fallback)
- [ ] 涉及 FSM 状态或转移 → 写/改 regression test 锁定 expected effect(ShowOverlay / HideOverlay / StopVision 等)
- [ ] 涉及 SQLite 表结构 → 写迁移与回滚测试
- [ ] 涉及 README 数字(tests 数 / version) → 与代码 / Cargo.toml 同步

## 测试

- [ ] 本地跑通 `cd src-tauri; cargo test --lib`(输出 `test result: ok`)
- [ ] 改动了 FSM / settings / 路径策略 / updater 中任一处时确认有对应测试

## 安全 / 隐私

- [ ] 没有引入新网络调用(若有,延伸 README 的"安全与隐私"段说明)
- [ ] 摄像头帧 / L3 原因等敏感数据没有外传或落盘到意外路径
- [ ] 没有硬编码 secrets / token / 私钥在仓库中

---

<!-- 一句话总结:这个 PR 在做什么,以及为什么。 -->
