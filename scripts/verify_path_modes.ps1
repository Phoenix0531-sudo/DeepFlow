# DeepFlow #48 路径策略验收脚本（基于 release 产物，不启动 GUI）
# 用法：在仓库根目录执行  powershell -File scripts/verify_path_modes.ps1

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$Exe = Join-Path $Root "src-tauri\target\release\deepflow.exe"
$Report = Join-Path $Root "docs\install-verify-report.md"

function Assert-True($cond, $msg) {
  if (-not $cond) { throw "FAIL: $msg" }
  Write-Host "OK  $msg"
}

$lines = @()
$lines += "# DeepFlow 装包/路径验收报告"
$lines += ""
$lines += "生成时间: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')"
$lines += ""

# 1) release 二进制存在
Assert-True (Test-Path $Exe) "release exe exists: $Exe"
$fi = Get-Item $Exe
$lines += "## 1. Release 二进制"
$lines += "- path: ``$($fi.FullName)``"
$lines += "- size: $($fi.Length) bytes"
$lines += "- mtime: $($fi.LastWriteTime)"
$lines += "- status: **PASS** (cargo release 构建成功)"
$lines += ""

# 2) 单元测试覆盖路径判定
$lines += "## 2. 路径判定单元测试"
Push-Location (Join-Path $Root "src-tauri")
$testOut = & cargo test --lib is_dev_exe is_portable_mode -- --nocapture 2>&1 | Out-String
Pop-Location
$pass = $testOut -match "test result: ok"
Assert-True $pass "path unit tests pass"
$lines += '```'
$lines += ($testOut -split "`n" | Select-Object -Last 12) -join "`n"
$lines += '```'
$lines += "- status: **PASS**"
$lines += ""

# 3) 便携标记文件探测（不启动 GUI，只检查标记文件逻辑依赖）
$lines += "## 3. 便携标记文件探测"
$tmp = Join-Path $env:TEMP "deepflow-path-verify"
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
$flag = Join-Path $tmp "portable.flag"
Set-Content -Path $flag -Value "1" -Encoding ascii
Assert-True (Test-Path $flag) "portable.flag can be created beside a staged dir"
$lines += "- staged dir: ``$tmp``"
$lines += "- portable.flag: present"
$lines += "- note: 实际 mode=portable 需把 deepflow.exe 拷到该目录后启动 GUI 验证 get_path_info"
$lines += "- status: **PARTIAL** (标记文件机制可写；GUI 启动项需人工)"
$lines += ""

# 4) env 覆盖目录可写
$lines += "## 4. DEEPFLOW_DATA_DIR 可写性"
$envDir = Join-Path $tmp "env-data"
New-Item -ItemType Directory -Force -Path $envDir | Out-Null
$probe = Join-Path $envDir "probe.txt"
Set-Content -Path $probe -Value "ok" -Encoding utf8
Assert-True (Test-Path $probe) "DEEPFLOW_DATA_DIR target is writable"
$lines += "- env dir: ``$envDir``"
$lines += "- status: **PASS** (目录可写；mode=env 需启动后看 get_path_info)"
$lines += ""

# 5) 安装包产物
$lines += "## 5. 安装包 bundle"
$bundle = Join-Path $Root "src-tauri\target\release\bundle"
if (Test-Path $bundle) {
  $files = Get-ChildItem $bundle -Recurse -File | Select-Object -ExpandProperty FullName
  $lines += "- bundle dir: ``$bundle``"
  foreach ($f in $files) { $lines += "  - $f" }
  $lines += "- status: **PASS**"
} else {
  $lines += "- bundle dir: missing"
  $lines += "- status: **FAIL/PARTIAL** — release exe 已出，但 WiX/NSIS 打包未成功（常见：GitHub 下载 WiX 超时）"
  $lines += "- 复现: ``tauri build`` 日志含 ``Downloading wix314-binaries.zip`` + ``timeout: global``"
}
$lines += ""

# 6) 系统集成配置存在性（静态）
$lines += "## 6. 系统集成静态核验"
$conf = Get-Content (Join-Path $Root "src-tauri\tauri.conf.json") -Raw
$cap = Get-Content (Join-Path $Root "src-tauri\capabilities\default.json") -Raw
Assert-True ($conf -match '"updater"') "tauri.conf has updater plugin block"
Assert-True ($cap -match 'autostart:allow-enable') "capabilities grant autostart"
Assert-True ($cap -match 'notification:default') "capabilities grant notification"
Assert-True ($cap -match 'updater:default') "capabilities grant updater"
$lines += "- updater block: present (pubkey/endpoints 仍可为空 = configured:false)"
$lines += "- capabilities: autostart + notification + updater"
$lines += "- status: **PASS** (静态)"
$lines += ""

$lines += "## 总结"
$lines += "| 项 | 结果 |"
$lines += "| -- | ---- |"
$lines += "| release exe | PASS |"
$lines += "| path unit tests | PASS |"
$lines += "| portable flag 可写 | PARTIAL |"
$lines += "| env dir 可写 | PASS |"
$lines += "| 安装包 MSI/NSIS | 取决于本次 bundle |"
$lines += "| 系统集成静态 | PASS |"
$lines += ""
$lines += "> GUI 级验收（设置页 mode 显示、自启、系统通知 toast）仍需人工启动一次 release/安装包。"

$dir = Split-Path $Report
if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
Set-Content -Path $Report -Value ($lines -join "`n") -Encoding utf8
Write-Host ""
Write-Host "Report written: $Report"
