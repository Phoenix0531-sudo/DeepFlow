# Security Policy

DeepFlow 是一个**完全本地**运行的 Windows 桌面应用:摄像头帧只在内存推理、不落盘;L3 原因仅写入本机 SQLite;代码不通网络(除生产 HTTPS 自动更新检查那一行)。本项目本身的攻击面很小,但仍欢迎负责任披露。

## 报告漏洞

如果你发现安全漏洞,请**不要**开公开 issue,改为私发邮件给仓库 owner:

- 报告邮箱: 见仓库 owner 在 GitHub profile 上的联系信息(或通过 GitHub private vulnerability reporting:`Security` tab → `Report a vulnerability`)。

请在报告中包含:

- 漏洞影响与攻击路径
- 受影响版本(commit hash 或 release tag)
- 复现步骤(最小的就好)
- 期望的 mitigation 建议(可选)

## 响应时间

在合理工作负载下,期望响应时间:

| 节点 | 时间窗 |
| --- | --- |
| 初步确认收到 | 72 小时内 |
| 评估与定级 | 7 天内 |
| 修复或缓解发布 | 视严重度,通常 30 天内 |

## Scope

范围内的漏洞示例:

- 本地数据库 / 日志文件未授权访问或注入
- updater 签名校验可被绕过
- 摄像头帧不小心被持久化或外传
- IPC 命令缺权限校验导致越权写盘

范围外:

- 现代浏览器 / WebView2 自身的上游 bug(请报给 Microsoft)
- ONNX Runtime / ultralytics 上游 bug
- 用户主动把自己机器权限给到攻击者(对应的本地威胁假定)

## 使用 GitHub Security Advisory

如经评估确认为真实漏洞,将通过 GitHub Security Advisory 公开披露 + CVE 申请。报告者会得到 credit(默认署名,可匿名声请)。
