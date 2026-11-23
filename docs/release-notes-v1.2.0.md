# mqttc v1.2.0

[中文](#中文) | [English](#english)

一个简单轻便易用的 MQTT 调试客户端，支持 Windows、Linux 和 macOS。免安装，解压即用。

## 中文

## 下载

| 平台 | 文件 |
|---|---|
| Windows 10/11 x64 | `mqttc-v1.2.0-windows-x64.zip` |
| Linux x64（glibc 2.35+） | `mqttc-v1.2.0-linux-x64.tar.gz` |
| macOS 11+（Apple Silicon / Intel） | `mqttc-v1.2.0-macos-universal.zip` |

解压后运行 `mqttc`（macOS 打开 `mqttc.app`）。配置与导出的日志写在可执行文件同目录。

## 相比 v1.1.0

### 更新的内容

- **预设可以排顺序**：每条发布预设新增置顶 / 上移 / 下移 / 置底按钮，发布条严格按列表
  顺序排列；“预设回复”标签更名为“预设发布”，行内发送按钮带说明。
- **发布框自动换行**：长文本按发布框宽度换行，不再横向溢出到窗口边缘；视口固定四行，
  只在内容超出时出现滚动条，位置与日志框右侧对齐，滚轮滚动视口，光标在末尾时自动跟随。
- **只用 Ctrl+Enter 发布**：在发布框内按 Enter 只换行，Ctrl+Enter 才发送；焦点在其他
  位置时同样生效，按钮旁的提示同步更新。
- **详情页更好用**：日志详情可切换 Text / JSON / Base64 / Hex 显示格式，并分别复制
  Topic、Payload 或整条消息。
- **右键复制**：右键气泡的主题复制主题，右键内容复制消息本身，工具栏短暂提示
  “已复制 Topic / Payload / 整条”；工具栏按钮更名为“复制最新记录”。
- **订阅筛选可清除**：点击订阅条目按 Topic 筛选日志后，过滤条显示可点击的筛选标签，
  一键恢复全部记录。
- **重叠订阅只记一条**：同一条消息匹配多条订阅时，日志、收发计数与自动回复都只处理
  一次，不再重复显示或重复计数。
- **过滤更轻**：关键字过滤结果与序号查找改为增量缓存，长时间保留大量日志时刷新开销更低。

### 修复的问题

- 一条消息命中多条订阅时，日志里显示两次、计数也是两次；现在按投递内容合并为一条。

## 升级

直接替换可执行文件即可；`mqttc.json` 向后兼容。旧配置里的预设 `pinned` 字段会被忽略，
预设顺序即当前列表顺序，可在“预设发布”页用置顶 / 上移 / 下移 / 置底重新调整。

## 验证

- 逐项提交与改动：[v1.1.0...v1.2.0](https://github.com/GordonWen/mqttc/compare/v1.1.0...v1.2.0)
- 自动化测试：core 75 项单元测试、3 项 e2e、7 项 roundtrip（1 项 TLS 默认忽略）与
  iced 17 项冒烟测试全部通过。
- 本地实测：发布框换行与滚动条按需显示、滚轮滚动与末尾跟随；重叠订阅去重；预设排序
  与发布顺序。

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
| Windows 10/11 x64 | `mqttc-v1.2.0-windows-x64.zip` |
| Linux x64 (glibc 2.35+) | `mqttc-v1.2.0-linux-x64.tar.gz` |
| macOS 11+ (Apple Silicon / Intel) | `mqttc-v1.2.0-macos-universal.zip` |

Unzip and run `mqttc` (open `mqttc.app` on macOS). The config and exported logs live next
to the executable.

## What's new since v1.1.0

### Improvements

- **Reordered presets**: every publish preset row now has top / up / down / bottom buttons,
  and the publish strip follows plain list order. The "Preset replies" tab is now
  "Publish presets", and the row Send action explains itself in a tooltip.
- **Wrapped payload editor**: long text wraps to the publish box instead of overflowing to
  the window edge. The viewport keeps four lines, shows a scrollbar only when the text
  overflows, sits on the same right edge as the log list's bar, and the wheel scrolls the
  viewport with the caret following at the end.
- **Ctrl+Enter publishes**: Enter inside the payload box only inserts a newline; Ctrl+Enter
  sends from anywhere, and the hint next to the button matches.
- **Better detail page**: switch between Text / JSON / Base64 / Hex and copy the topic,
  the payload or the whole message.
- **Right-click copy**: right-click a bubble's topic to copy the topic, or its payload to
  copy the message; the toolbar briefly confirms "Topic copied / Payload copied / Message
  copied", and "Copy last line" becomes "Copy latest record".
- **Clearable subscription filter**: clicking a subscription filters logs by topic, and the
  filter chip can be clicked to show everything again.
- **One row per message**: a message matching several subscriptions is logged, counted and
  auto-replied once instead of once per subscription.
- **Lighter filtering**: the filtered log view and sequence lookup are cached incrementally,
  so keyword filters stay cheap with a long history.

### Fixes

- A message matching several subscriptions was logged twice and counted twice; copies are
  now folded into one row.

## Upgrade

Replace the executable; `mqttc.json` stays compatible. The old per-preset `pinned` flag is
ignored and presets publish in list order; use the top / up / down / bottom controls on the
"Publish presets" page to arrange them.

## Verification

- Commits and diff: [v1.1.0...v1.2.0](https://github.com/GordonWen/mqttc/compare/v1.1.0...v1.2.0)
- Tests: 75 core unit tests, 3 e2e tests, 7 roundtrip tests (1 TLS test ignored by default)
  and 17 Iced smoke tests all pass.
- Local checks: payload wrapping with the on-demand scrollbar, wheel scrolling and
  end-of-text follow, overlapping-subscription folding, and preset ordering/publish order.

## Notes

- Builds are unsigned. On macOS, right-click → Open, or run
  `xattr -dr com.apple.quarantine mqttc.app`.
- Linux needs `libxkbcommon`, Wayland or X11, and `fontconfig`; see READ-ME in the archive.
- The Windows build is made locally; every asset ships with a `.sha256`, and the release
  also carries a merged `SHA256SUMS.txt`.

Source and issues: https://github.com/GordonWen/mqttc · MIT License
