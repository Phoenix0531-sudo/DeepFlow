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
