# 发布流程（维护者）

简体中文 | [English](RELEASING.en.md)

本文记录如何把仓库内容发布到 GitHub，以及 GitHub 端需要做的一次性设置。
正式发布版本为 **iced 版**（打包后命名为 `mqttc`）；egui / gpui 仅保留源码。

## 1. 一次性设置

本机：

```powershell
winget install GitHub.cli        # 如尚未安装
gh auth login                    # 浏览器授权，不要把 token 写进任何文件
gh auth status
```

GitHub 仓库 `GordonWen/mqttc`：

1. Settings → Actions → General：启用 Actions；Workflow permissions 选择
   **Read and write permissions**（工作流内也声明了 `permissions: contents: write`）。
2. Settings → General → Default branch 设为 `main`。
3. 如 `main` 有分支保护，清空历史前临时允许 force-push，完成后恢复。
4. 删除旧 Release、tag、旧分支（见第 2 节）。
5. 仓库描述：`A lightweight, cross-platform MQTT debugging client for Windows, Linux and macOS`。
   Topics：`mqtt`、`mqtt-client`、`rust`、`iced`、`portable`、`desktop`。
6. 未签名构建不需要任何 secret；后续做 macOS 签名/公证时再加 Apple 相关 secrets。

## 2. 清空远端并推送新历史（仅首次）

先尝试备份旧仓库（不改动远端）：

```powershell
git clone --mirror https://github.com/GordonWen/mqttc.git D:\QoderProj\mqttc-old-backup.git
```

> 2026-11-20 的实际备份因旧仓库约 172 MB、GitHub 下载持续超时未完成。
> v0.1.1 的源码与二进制仍完整保留在本地 `参考实现/` 目录；旧分支/tag 已在远端删除。

在工作区创建全新历史：

```powershell
cd D:\QoderProj\mqttc
git init -b main
git add .
git commit -m "mqttc 0.2.0: lightweight portable MQTT debug client (iced)"
git remote add origin https://github.com/GordonWen/mqttc.git
git push --force origin main
```

删除旧 Release / tag：

```powershell
gh release list -R GordonWen/mqttc --json tagName -q '.[].tagName' |
  ForEach-Object { gh release delete $_ -R GordonWen/mqttc -y --cleanup-tag }
```

若还有孤立 tag：`git ls-remote --tags origin` 查看后执行
`git push origin :refs/tags/<tag>`。

## 3. 发布新版本

1. 更新 `Cargo.toml` 的 `version`（工作区级）与 `CHANGELOG.md`。
2. 本地测试并打包 Windows：

   ```powershell
   cargo test -p mqttc-core
   cargo test -p mqttc-iced
   powershell -ExecutionPolicy Bypass -File scripts\release-windows.ps1
   ```

   产物：`dist\release\mqttc-v<version>-windows-x64.zip` 与 `.sha256`。

3. 提交、打 tag、推送：

   ```powershell
   git add -A
   git commit -m "release: v<version>"
   git tag v<version>
   git push origin main
   git push origin v<version>
   ```

4. `.github/workflows/release.yml` 自动构建：

   - Linux x64：`ubuntu-22.04` → `mqttc-v<version>-linux-x64.tar.gz`
   - macOS universal（arm64 + x64）：`macos-15` / `macos-15-intel` 分别构建后 `lipo` 合并
     → `mqttc-v<version>-macos-universal.zip`（内含 `mqttc.app`）
   - `release` job 创建 GitHub Release 并上传两个产物与 `SHA256SUMS.txt`。

   用 `gh run watch` 或 Actions 页面确认通过。

5. 上传 Windows 产物，并把它的哈希并入汇总文件：

   ```powershell
   gh release upload v<version> dist\release\mqttc-v<version>-windows-x64.zip `
     dist\release\mqttc-v<version>-windows-x64.zip.sha256 --clobber

   $tmp = Join-Path $env:TEMP "SHA256SUMS.txt"
   gh release download v<version> -p SHA256SUMS.txt -O $tmp --clobber
   Get-Content dist\release\mqttc-v<version>-windows-x64.zip.sha256 |
     Add-Content $tmp
   gh release upload v<version> $tmp --clobber
   ```

6. 编辑 Release 说明（必须中英文）：

   `docs/release-notes-v<version>.md` 同时包含中文和英文：先写中文部分，再用 `---`
   分隔并写英文部分；文件顶部可放 `[中文](#中文) | [English](#english)` 导航。

   ```powershell
   gh release edit v<version> --title "mqttc v<version>" `
     --notes-file docs\release-notes-v<version>.md
   ```

   Release 标题固定为 `mqttc v<version>`，不添加“ — 简单轻便的 MQTT 调试客户端”等副标题。
   CI 通过 `--generate-notes` 创建的正文只是临时占位，发布前必须替换成双语说明。

## 4. 产物与兼容性

| 平台 | 产物 | 运行要求 |
|---|---|---|
| Windows x64 | zip（`mqttc.exe` + `mqttc.json` + READ-ME） | Win10 1809+ / Win11；VC++ 运行库 |
| Linux x64 | tar.gz（`mqttc/`） | glibc 2.35+；libxkbcommon、Wayland/X11、fontconfig |
| macOS universal | zip（`mqttc.app`） | macOS 11+；未签名，首次右键→打开 |

- Linux 在 Ubuntu 22.04 上构建，以兼容较老的 glibc。
- macOS 未签名、未公证；如用户遇到 Gatekeeper 拦截，执行
  `xattr -dr com.apple.quarantine mqttc.app`。
- 每个资产都有独立的 `.sha256`，Release 另有合并的 `SHA256SUMS.txt`。

## 5. 回滚

- 发布错误时可 `gh release delete v<version> -y --cleanup-tag` 后重新打 tag。
- 工作流失败不会影响已推送的代码；修复后可用
  `gh workflow run release.yml -f tag=v<version>` 重跑。
- 旧仓库的完整 mirror 备份未完成；如需旧版本，可从本地 `参考实现/` 或 GitHub 支持恢复。
