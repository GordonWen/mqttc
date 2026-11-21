# mqttc v1.1.0

[中文](#中文) | [English](#english)

一个简单轻便易用的 MQTT 调试客户端，支持 Windows、Linux 和 macOS。免安装，解压即用。

## 中文

## 下载

| 平台 | 文件 |
|---|---|
| Windows 10/11 x64 | `mqttc-v1.1.0-windows-x64.zip` |
| Linux x64（glibc 2.35+） | `mqttc-v1.1.0-linux-x64.tar.gz` |
| macOS 11+（Apple Silicon / Intel） | `mqttc-v1.1.0-macos-universal.zip` |

解压后运行 `mqttc`（macOS 打开 `mqttc.app`）。配置与导出的日志写在可执行文件同目录。

## 相比 v1.0.0

### 更新的内容

- **长会话更流畅**：日志列表只绘制最新 80 条，顶部可“显示更早”逐页回看，出现新消息时
  底部可“回到最新”；10 msg/s 流式接收时单核 CPU 从约 34% 降到 1% 以下。搜索、复制、
  导出仍覆盖全部记录。
- **收发计数**：会话标题行新增 `收 · N`、`发 · N` 计数，可一键清零；暂停日志时继续计数，
  系统与错误提示不计入。
- **手动滚动不再被顶走**：滚轮上滚自动冻结当前页并暂停跟随；点“回到最新”或重新勾选
  “自动滚动”即恢复。
- **窗口置顶**：标题栏新增图钉按钮，可让窗口固定在其他应用之上，重启后保持。
- **气泡自适应宽度**：短消息只占约三分之一宽度，长内容最长到列表的 85%，阅读长 JSON
  更省眼；主题、字节数与复制/详情入口随气泡同行排布。
- **滚动条归位**：滑动条贴住日志框右侧，与右对齐的发送气泡留出间距。
- **英文文档**：新增英文 README，以及架构、版本对照、发布流程与基准测试的英文版。

### 修复的问题

- 日志冻结在旧页后，达到保留上限时视图会滑回最新一条；现在固定在被冻结的那一页。
- 负载含 Unicode 段落分隔符（U+2029）时文本排版可能崩溃；显示与复制时统一替换为空格。
- “下方还有 N 条”横幅与设置页显示的保留条数不一致；现按真实上限显示，并说明更早的
  记录会被丢弃。

## 升级

直接替换可执行文件即可；`mqttc.json` 向后兼容，缺失字段使用默认值。

## 验证

- 逐项提交与改动：[v1.0.0...v1.1.0](https://github.com/GordonWen/mqttc/compare/v1.0.0...v1.1.0)
- 性能与资源对比：[docs/BENCHMARK.md](https://github.com/GordonWen/mqttc/blob/main/docs/BENCHMARK.md)
- 关键实测：[日志窗口化，34% → <1%](https://github.com/GordonWen/mqttc/commit/6b275e4)、
  [气泡自适应，27% → 19%](https://github.com/GordonWen/mqttc/commit/3b739ae)

## 说明

- 构建未签名。macOS 首次打开请右键 → 打开，或执行
  `xattr -dr com.apple.quarantine mqttc.app`。
- Linux 需要 `libxkbcommon`、Wayland 或 X11、`fontconfig`；详见包内 READ-ME。
- Windows 包本地构建；每个资产附 `.sha256`，Release 中另有合并的 `SHA256SUMS.txt`。

源码与问题反馈：https://github.com/GordonWen/mqttc · MIT 许可证

---

## English

A lightweight, easy-to-use MQTT debugging client for Windows, Linux and macOS. No
installation — unzip and run.

## Downloads

| Platform | File |
|---|---|
| Windows 10/11 x64 | `mqttc-v1.1.0-windows-x64.zip` |
| Linux x64 (glibc 2.35+) | `mqttc-v1.1.0-linux-x64.tar.gz` |
| macOS 11+ (Apple Silicon / Intel) | `mqttc-v1.1.0-macos-universal.zip` |

Unzip and run `mqttc` (open `mqttc.app` on macOS). The config and exported logs live next
to the executable.

## What's new since v1.0.0

### Improvements

- **Long sessions stay smooth**: the log list renders only the newest 80 rows, with a
  "Show older" button at the top to page back and a "Jump to latest" button when new
  messages arrive; streaming at 10 msg/s drops from ~34% of one core to under 1%. Search,
  copy and export still cover the full history.
- **Traffic counters**: the session header shows `In · N` / `Out · N` with a one-click
  reset. Messages keep counting while the list is paused; SYS and error rows are excluded.
- **Manual scrolling is no longer overridden**: scrolling up freezes the current page and
  stops following; "Jump to latest" or re-checking auto-scroll resumes it.
- **Always on top**: a new title-bar pushpin keeps the window above other apps and
  persists the choice.
- **Content-sized bubbles**: short messages take about a third of the width, long content
  grows up to 85% of the list; the topic, byte count and copy/detail actions follow the
  bubble.
- **Scrollbar placement**: the scrollbar hugs the right edge of the log frame, clear of
  right-aligned sent bubbles.
- **English docs**: an English README plus English versions of the architecture, parity,
  releasing and benchmark documents.

### Fixes

- A frozen log page could slide back to the newest entry once the retention limit evicted
  it; the frozen page now stays put.
- Payloads containing the Unicode paragraph separator (U+2029) could abort text shaping;
  display and copy now substitute a space.
- The "N newer entries below" banner and the settings page disagreed about the retention
  limit; both now show the real limit and note that older rows are dropped.

## Upgrade

Replace the executable; `mqttc.json` stays compatible and missing fields fall back to
defaults.

## Verification

- Commits and diff: [v1.0.0...v1.1.0](https://github.com/GordonWen/mqttc/compare/v1.0.0...v1.1.0)
- Performance and footprint: [docs/BENCHMARK.md](https://github.com/GordonWen/mqttc/blob/main/docs/BENCHMARK.md)
- Key measurements: [windowed log list, 34% → <1%](https://github.com/GordonWen/mqttc/commit/6b275e4),
  [content-sized bubbles, 27% → 19%](https://github.com/GordonWen/mqttc/commit/3b739ae)

## Notes

- Builds are unsigned. On macOS, right-click → Open, or run
  `xattr -dr com.apple.quarantine mqttc.app`.
- Linux needs `libxkbcommon`, Wayland or X11, and `fontconfig`; see READ-ME in the archive.
- The Windows build is made locally; every asset ships with a `.sha256`, and the release
  also carries a merged `SHA256SUMS.txt`.

Source and issues: https://github.com/GordonWen/mqttc · MIT License
