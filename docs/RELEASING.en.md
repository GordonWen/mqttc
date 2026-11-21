# Release process (maintainers)

[简体中文](RELEASING.md) | English

This document describes how to publish the repository to GitHub and the one-time setup
required on the GitHub side. The official release is the **Iced build** (packaged as
`mqttc`); egui / gpui are source-only.

## 1. One-time setup

Local machine:

```powershell
winget install GitHub.cli        # if not installed
gh auth login                    # browser authorization; never write the token to any file
gh auth status
```

GitHub repository `GordonWen/mqttc`:

1. Settings → Actions → General: enable Actions; set Workflow permissions to
   **Read and write permissions** (the workflow also declares `permissions: contents: write`).
2. Settings → General → Default branch: set to `main`.
3. If `main` is protected, temporarily allow force-push before wiping history, then
   restore the protection afterwards.
4. Delete old releases, tags and old branches (see section 2).
5. Repository description:
   `A lightweight, cross-platform MQTT debugging client for Windows, Linux and macOS`.
   Topics: `mqtt`, `mqtt-client`, `rust`, `iced`, `portable`, `desktop`.
6. Unsigned builds need no secrets; add Apple-related secrets later if macOS
   signing/notarization is introduced.

## 2. Wipe the remote and push new history (first time only)

First try to back up the old repository (without changing the remote):

```powershell
git clone --mirror https://github.com/GordonWen/mqttc.git D:\QoderProj\mqttc-old-backup.git
```

> The actual backup attempt on 2026-11-20 did not complete because the old repository was
> about 172 MB and GitHub downloads kept timing out. The v0.1.1 source and binaries are
> still fully preserved in the local `参考实现/` directory; old branches/tags have been
> deleted from the remote.

Create a fresh history in the workspace:

```powershell
cd D:\QoderProj\mqttc
git init -b main
git add .
git commit -m "mqttc 0.2.0: lightweight portable MQTT debug client (iced)"
git remote add origin https://github.com/GordonWen/mqttc.git
git push --force origin main
```

Delete old releases / tags:

```powershell
gh release list -R GordonWen/mqttc --json tagName -q '.[].tagName' |
  ForEach-Object { gh release delete $_ -R GordonWen/mqttc -y --cleanup-tag }
```

If orphan tags remain, list them with `git ls-remote --tags origin` and run
`git push origin :refs/tags/<tag>`.

## 3. Releasing a new version

1. Update `version` in `Cargo.toml` (workspace level) and update `CHANGELOG.md`.
2. Test and package Windows locally:

   ```powershell
   cargo test -p mqttc-core
   cargo test -p mqttc-iced
   powershell -ExecutionPolicy Bypass -File scripts\release-windows.ps1
   ```

   Artifacts: `dist\release\mqttc-v<version>-windows-x64.zip` and `.sha256`.

3. Commit, tag and push:

   ```powershell
   git add -A
   git commit -m "release: v<version>"
   git tag v<version>
   git push origin main
   git push origin v<version>
   ```

4. `.github/workflows/release.yml` builds automatically:

   - Linux x64: `ubuntu-22.04` → `mqttc-v<version>-linux-x64.tar.gz`
   - macOS universal (arm64 + x64): `macos-15` / `macos-15-intel` build separately, then
     `lipo` merges them → `mqttc-v<version>-macos-universal.zip` (contains `mqttc.app`)
   - The `release` job creates the GitHub Release and uploads both artifacts plus
     `SHA256SUMS.txt`.

   Confirm success with `gh run watch` or the Actions page.

5. Upload the Windows artifact and merge its hash into the summary file:

   ```powershell
   gh release upload v<version> dist\release\mqttc-v<version>-windows-x64.zip `
     dist\release\mqttc-v<version>-windows-x64.zip.sha256 --clobber

   $tmp = Join-Path $env:TEMP "SHA256SUMS.txt"
   gh release download v<version> -p SHA256SUMS.txt -O $tmp --clobber
   Get-Content dist\release\mqttc-v<version>-windows-x64.zip.sha256 |
     Add-Content $tmp
   gh release upload v<version> $tmp --clobber
   ```

6. Edit the release notes (Chinese + English required):

   `docs/release-notes-v<version>.md` must contain both Chinese and English: Chinese
   first, then `---`, then English. You can put `[中文](#中文) | [English](#english)`
   navigation at the top.

   ```powershell
   gh release edit v<version> --title "mqttc v<version>" `
     --notes-file docs\release-notes-v<version>.md
   ```

   The release title is always `mqttc v<version>`; do not add a subtitle such as
   “ — 简单轻便的 MQTT 调试客户端”. The `--generate-notes` body created by CI is only a
   temporary placeholder; replace it with bilingual notes before publishing the release.

## 4. Artifacts and compatibility

| Platform | Artifact | Requirements |
|---|---|---|
| Windows x64 | zip (`mqttc.exe` + `mqttc.json` + READ-ME) | Win10 1809+ / Win11; VC++ runtime |
| Linux x64 | tar.gz (`mqttc/`) | glibc 2.35+; libxkbcommon, Wayland/X11, fontconfig |
| macOS universal | zip (`mqttc.app`) | macOS 11+; unsigned, first launch via right-click → Open |

- Linux is built on Ubuntu 22.04 for compatibility with older glibc.
- macOS builds are unsigned and not notarized. If Gatekeeper blocks the app, run
  `xattr -dr com.apple.quarantine mqttc.app`.
- Each asset has its own `.sha256`, and the Release also provides a merged
  `SHA256SUMS.txt`.

## 5. Rollback

- If a release is wrong, run `gh release delete v<version> -y --cleanup-tag` and re-tag.
- A workflow failure does not affect pushed code; after fixing it, rerun with
  `gh workflow run release.yml -f tag=v<version>`.
- The full mirror backup of the old repository was not completed; to recover old versions,
  use the local `参考实现/` directory or GitHub support.
