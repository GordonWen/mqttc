# mqttc v1.3.0

[中文](#中文) | [English](#english)

一个简单轻便易用的 MQTT 调试客户端，支持 Windows、Linux 和 macOS。免安装，解压即用。

## 中文

### 下载

| 平台 | 文件 |
|---|---|
| Windows 10/11 x64 | `mqttc-v1.3.0-windows-x64.zip` |
| Linux x64（glibc 2.35+） | `mqttc-v1.3.0-linux-x64.tar.gz` |
| macOS 11+（Apple Silicon / Intel） | `mqttc-v1.3.0-macos-universal.zip` |

解压后运行 `mqttc`（macOS 打开 `mqttc.app`）。配置与导出的日志写在可执行文件同目录。

### 相比 v1.2.0

- **MQTT 5.0 发布属性**：预设发布和自动回复可设置用户属性与消息过期时间；发布时按
  编辑器格式自动带 content-type（JSON 为 `application/json`）和 UTF-8 标志。
- **请求 / 响应回复**：自动回复可跟随请求的 Response Topic，并原样回传
  Correlation Data；未勾选或请求未带时仍用固定回复 Topic。
- **确认状态可见**：SEND 记录显示排队 / 已发 / 等待确认 / 等待完成 / 已确认 / 被拒 /
  超时，增加 15 秒确认超时与断链标记。
- **发布前预检**：连接后显示 CONNACK 能力摘要，按保留、最高 QoS、报文大小、通配符
  和共享订阅限制提前拦截不合规操作。
- **手动 ACK**：可以暂扣入站 QoS 1/2 消息，由界面确认后再回复。
- **TLS 密钥日志**：可选导出 SSLKEYLOGFILE 格式密钥，便于本地抓包排查。
- **版本适配**：3.1.1 下隐藏 MQTT 5 属性行，连接页区分 Clean Session / Clean Start。
- **交互统一**：预设与回复规则共用四向排序箭头，发送按钮移到负载行末尾。

### 升级

直接替换可执行文件即可；`mqttc.json` 向后兼容。

### 验证

- 逐项提交与改动：[v1.2.0...v1.3.0](https://github.com/GordonWen/mqttc/compare/v1.2.0...v1.3.0)
- 自动化测试：core 85 项单元测试、3 项 e2e、8 项 roundtrip（1 项 TLS 默认忽略）、
  5 项故障注入与 iced 20 项冒烟测试全部通过。

### 说明

- 构建未签名。macOS 首次打开请右键 → 打开，或执行
  `xattr -dr com.apple.quarantine mqttc.app`。
- Linux 需要 `libxkbcommon`、Wayland 或 X11、`fontconfig`；详见包内 README。

---

## English

A small, portable MQTT debugging client for Windows, Linux and macOS. No installer.

### Download

| Platform | File |
|---|---|
| Windows 10/11 x64 | `mqttc-v1.3.0-windows-x64.zip` |
| Linux x64 (glibc 2.35+) | `mqttc-v1.3.0-linux-x64.tar.gz` |
| macOS 11+ (Apple Silicon / Intel) | `mqttc-v1.3.0-macos-universal.zip` |

Unzip and run `mqttc` (open `mqttc.app` on macOS). Config and exported logs live next to
the executable.

### Changes since v1.2.0

- **MQTT 5.0 publish properties**: presets and auto replies can set user properties and
  message expiry; publishes declare content-type (JSON as `application/json`) and a UTF-8
  payload format based on the editor format.
- **Request / response replies**: auto replies can follow the request's Response Topic and
  echo its Correlation Data; the fixed reply topic is still used when disabled or absent.
- **Visible acknowledgement state**: SEND rows show queued / sent / awaiting ack /
  awaiting comp / acked / rejected / timeout, with a 15 s timeout and link-loss marking.
- **Pre-flight checks**: a CONNACK capability summary plus checks against retain, max QoS,
  packet size, wildcard and shared-subscription limits before publishing or subscribing.
- **Manual ACK**: hold inbound QoS 1/2 messages until the UI confirms them.
- **TLS key log**: optional SSLKEYLOGFILE-format keys for local capture.
- **Version-aware UI**: MQTT 5 rows stay hidden on 3.1.1, and the connection page labels
  Clean Session / Clean Start per version.
- **Consistent controls**: presets and reply rules share the four-way reorder arrows, with
  Send moved to the end of the payload row.

### Upgrade

Replace the executable; `mqttc.json` stays backward compatible.

### Verification

- Commits and diff: [v1.2.0...v1.3.0](https://github.com/GordonWen/mqttc/compare/v1.2.0...v1.3.0)
- Automated tests: 85 core unit tests, 3 e2e, 8 roundtrip (1 TLS test ignored by default),
  5 fault-injection and 20 iced smoke tests all pass.

### Notes

- Builds are unsigned. On macOS, right-click → Open the first time, or run
  `xattr -dr com.apple.quarantine mqttc.app`.
- Linux needs `libxkbcommon`, Wayland or X11, and `fontconfig`; see the README in the package.
