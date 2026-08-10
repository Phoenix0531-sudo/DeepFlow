# DeepFlow 自动更新发布流程

> 本文描述如何从空骨架（`tauri.conf.json` 里 `updater.pubkey=""`、`endpoints=[]`）走到可实际分发更新的状态。
> 当前状态：**未配置**。前端"检查更新"会返回 `configured: false` 并提示"更新功能尚未配置"。

## 1. 生成签名密钥

使用 Tauri 自带 signer 生成一对密钥：

```bash
# 安装 tauri-cli（如未安装）
cargo install tauri-cli --version "^2"

# 生成密钥对，会输出 pubkey 与私钥（私钥需妥善保管，绝不入库）
tauri signer sign --generate-key
```

输出形如：

```
Pubkey:  dwTJB...（公开，写入 tauri.conf.json）
Key:     unescaped://（私钥，用于签名，必须保密）
```

## 2. 配置 tauri.conf.json

将 pubkey 写入，并填入更新清单端点：

```jsonc
{
  "plugins": {
    "updater": {
      "pubkey": "dwTJB...',           // 来自上一步的 Pubkey
      "endpoints": [
        "https://your-domain.com/deepflow/updates.json"
      ],
      "windows": {
        "installMode": "passive"       // passive | basicUi | quiet
      }
    }
  }
}
```

- `endpoints` 可以是多个，Tauri 会依次尝试，取第一个返回 200/304 的。
- 端点需返回符合 [Tauri updater 协议](https://v2.tauri.app/plugin/updater/) 的 JSON 清单。

## 3. 构建带签名的安装包

```bash
# 设置私钥环境变量（CI 中注入）
export TAURI_SIGNING_PRIVATE_KEY="unescaped://..."   # 来自第 1 步的 Key
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""          # 密钥口令，可为空

# 正常构建，会输出带 .sig 签名文件的安装包
npm run tauri build
```

构建产物会多出 `DeepFlow_0.1.0_x64-setup.exe.sig` 等签名文件，**这是 updater 校验安装包完整性的依据**。

## 4. 部署更新清单

在你的静态服务器上放置 `updates.json`，示例：

```json
{
  "version": "0.1.1",
  "notes": "修复xxx，新增yyy",
  "pub_date": "2026-08-08T12:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "signature": "<把 .sig 文件内容粘这里>",
      "url": "https://your-domain.com/deepflow/DeepFlow_0.1.1_x64-setup.exe"
    }
  }
}
```

- `version` 高于当前应用版本时，Tauri 才认为有更新。
- `signature` 必须是第 3 步生成的 `.sig` 文件内容（字符串）。
- `url` 指向同版本安装包的下载地址。

## 5. 自检清单

- [ ] `pubkey` 已填，非空
- [ ] `endpoints` 至少一条可达 URL
- [ ] CI 注入了 `TAURI_SIGNING_PRIVATE_KEY`
- [ ] 构建产物含 `.sig` 文件
- [ ] `updates.json` 的 `signature` 与本次 `.sig` 一致
- [ ] `updates.json` 的 `version` 高于当前 `tauri.conf.json` 的 `version`

配置完成后，应用内"设置 → 系统集成 → 检查更新"应能检出并下载安装。

## 6. 降级 / 回退说明

Tauri updater 在签名校验失败或下载异常时会中止安装，不会替换现有二进制；
新版本安装失败不影响旧版本运行。私钥一旦泄露需立即重新生成密钥对并发布新版本。

---

## 7. 生产发布全流程 checklist (v0.2 示例)

从冷骨架走到分发包的完整 8 步。不依赖 .auto/ 内任何资源 (那份在 .gitignore中不进入仓库).

### 7.1 准备签名密钥 (首次或复用)

```bash
# 生成新密钥对(输出 Pubkey 到 stdout, 私钥写 -w 指定文件)
# 在 PowerShell 里跑(--ci 免输密码, 适合脚本化):
npm run tauri -- signer generate --ci -w ./df-tauri-private.key
```

- Pubkey 输出贴到 `tauri.conf.json` 的 `plugins.updater.pubkey`。
- 私钥 `./df-tauri-private.key` **绝不入库** — 立刻转移到 1Password / KeePass / 加密 USB。
- 重新生成密钥会作废现有 pubkey, 需同步 tauri.conf.json。
- **不要**把私钥存在 `$env:TEMP`(本机重启会清, 木马可访问)。

### 7.2 配置 tauri.conf.json 的 endpoints (仅生产)

```jsonc
{
  "plugins": {
    "updater": {
      "pubkey": "dwRB...",            // 你刚生成的公钥
      "endpoints": [
        "https://your-cdn.com/deepflow/v0.2/updates.json"  // 生产 CDN / GH releases
      ],
      "windows": { "installMode": "passive" }
    }
  }
}
```

端点可多列 fallback, Tauri 依次试, 取第一个返回 200/304 的。

### 7.3 提升版本号

同时改 `tauri.conf.json > version` 与 `package.json > version`(保持一致)。

### 7.4 构建带签名的 setup.exe

```bash
# PowerShell 中现在准备环境变量:
$env:TAURI_SIGNING_PRIVATE_KEY="$(Get-Content ./df-tauri-private.key -Raw)"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""           # 设密码则改这里
# 构建会自动签名产物出一个 .sig 文件
npm run tauri build -- --bundles nsis
```

最终产物:
- `src-tauri/target/release/bundle/nsis/DeepFlow_0.2.0_x64-setup.exe`
- `src-tauri/target/release/bundle/nsis/DeepFlow_0.2.0_x64-setup.exe.sig`

**若 tauri build 静默未签** (PowerShell 子进程可能未继承 `TAURI_SIGNING_PRIVATE_KEY`, 静默产未签 setup.exe):见 §8.2 手动补签名。

### 7.5 生成 updates.json

```bash
node docs/scripts/gen-updates.js 0.2.0 \
  src-tauri/target/release/bundle/nsis/DeepFlow_0.2.0_x64-setup.exe \
  src-tauri/target/release/bundle/nsis/DeepFlow_0.2.0_x64-setup.exe.sig \
  ./release/0.2.0 \
  https://your-cdn.com/deepflow/v0.2.0
```

产出 `release/0.2.0/updates.json` + 拷贝好的 setup/sig。

### 7.6 部署 assets 到静态 HTTP/HTTPS 服务器

将 `release/0.2.0/` 下全部 3 个文件 (updates.json + setup.exe + .sig) upload 到端点基础路径。

推荐容器:
- **GitHub Releases** — 不同 version 用不同 Release tag, 使 updates.json 的 url 指 GH releases资产链接。GitHub Releases 是优先选择 (免费 + CDN + 可验、免维护服务器)。
- **Cloudflare R2 / S3 + CloudFront** — 高性能与传统 HTTPS 端点。
- **Nginx / Caddy** — 自建控制度高但需 TLS 证书与维护。
- **静态托管** (Vercel/Netlify/Cloudflare Pages) — 需️避免 CORS 问题。

### 7.7 验证 updates.json 可访问

```bash
# 端点 GET /updates.json 必须 status=200 且 Content-Type: application/json
curl -i https://your-cdn.com/deepflow/v0.2.0/updates.json
# 应看 version, platforms.windows-x86_64.signature, url, pub_date
```

### 7.8 本机安装旧版端点验证闭不环

1. 安装 v0.1.0 setup.exe (你现有的)。
2. 启動 → 设置 → 关于 → 检查更新, 应返回 `available:true, version:0.2.0`。
3. 点击下载安装, 等 Tauri updater 从端点取 sig + setup + 校验 + NSIS passive 安装。
4. 安装完应用重启 为 v0.2.0, 设置关于页版本号同步。

---

## 8. 已知坑 (本仓库踩过的 实录)

### 8.1 `dangerousInsecureTransportProtocol` 绝不入库

- 字段名**必须 camelCase**(`dangerousInsecureTransportProtocol`), 不是 snake_case (`dangerous_insecure_transport_protocol`)。后者静默被忽略不生效。
- 本机 http://localhost:8787 验证可用低速场。生产 必须 delete 并填 https endpoints。
- 现仓库 tauri.conf.json 目前 不含本字段; 发布前 review 确保不含。

### 8.2 PowerShell 下自动签名不生效的耐莎策略

`tauri build` 不自动签 setup.exe 的常见原因:PowerShell 子进程未继承 `TAURI_SIGNING_PRIVATE_KEY`, 静默 产未签。需手补:

```bash
# PowerShell 手动签名 (cmd /c 拼接 避阴引号报错 公週卷)
cmd /c "tauri signer sign -f DeepFlow_0.2.0_x64-setup.exe -k ./df-tauri-private.key -p """
```

产物输出 `.sig` 文件 (默认 420 字节 左右)。

### 8.3 pi read / Get-Content 对密钥 mask 与 `_comment` 字段

- pi 工具会将 Key 文件内容渲染为 `[密钥]` mask。私钥/pkey 生成后相关操作不应走 pi read/Get-Content。需用 `[System.IO.File]::ReadAllBytes()` 读字节再处理, 或 `node fs.readFile` 脚本。
- `tauri.conf.json > plugins.updater > _comment` 字段不影响 updater 运行, 但可存首次配置信息。本仓库使用此字段可入库。
