# mqttc

<img src="assets/icon.png" alt="mqttc" width="96">

跨平台桌面 MQTT 调试客户端，用 Rust 和 [Slint](https://slint.dev) 编写，界面参考 [MQTTX](https://github.com/emqx/MQTTX)。支持 MQTT 3.1.1 与 MQTT 5.0。

[MQTTX](https://github.com/emqx/MQTTX) 很好用。发起本项目，是为了得到一份拷贝即可运行、体积和内存占用更小的桌面客户端。

适合两类读者：

- 需要快速验证 MQTT 连通性、观察消息流的开发者、测试与运维人员
- 想通过完整可运行的小项目学习 Rust 桌面开发与 Slint 的初学者

## 下载

预编译包在 [GitHub Releases](https://github.com/GordonWen/mqttc/releases)：

| 平台 | 文件 | 说明 |
|------|------|------|
| Windows AMD64 | `mqttc.exe` | 约 16MB，双击运行 |
| Linux AMD64 | `mqttc-linux-amd64` | 约 21MB，见下方 Linux 说明 |
| macOS | `mqttc-macos-universal.zip` | Apple Silicon + Intel 通用包 |

源码构建见 [构建](#构建)。完整操作见 [使用说明](.monkeycode/docs/USER_GUIDE.md)。

## 最短上手路径

1. 启动一个可访问的 Broker，例如本地 Mosquitto 的 `127.0.0.1:1883`
2. 点左侧 `+`，填写主机与端口，选择 MQTT 3.1.1 或 5.0；需要认证时再填用户名和密码
3. 点顶栏「连接」。成功后进入会话页
4. 点左侧订阅列表底部「新建订阅」，填写 Topic（例如 `mqttc/mvp/#`）与 QoS 后确认
5. 在底部填写 Topic 与 Payload 后点「发布」；收到的消息出现在中间气泡列表

## 功能

- **多连接**：左侧列表同时保存并运行多条 Broker 连接，各自独立日志、订阅、预设与自动回复
- **传输**：TCP 或 TLS（系统 CA、自定义 CA、客户端证书，也可关闭校验）
- **协议**：MQTT 3.1.1 / 5.0；Client ID 留空时自动生成；Keep Alive、超时、Clean Session / Clean Start、Last Will
- **订阅**：`+` / `#` 通配符，QoS 0/1/2；左侧纵向列表可筛选、取消、隐藏；重连后自动恢复
- **发布**：文本 / JSON / Base64 / Hex；JSON 可校验并美化；定时发布 1–3600 秒，切到其它连接后后台继续
- **发布预设**：每连接独立列表，会话页底部一排发送按钮
- **自动回复**：按 Topic 过滤器与 Payload 包含匹配，第一条命中即回复；自身回投可跳过
- **日志**：RECV / SEND / SYS / ERR 聊天气泡；搜索、类型筛选、自动滚动；每连接最多 200 条；导出到 `logs/`
- **外观**：跟随系统 / 浅色 / 深色 / 夜间；字号小中大特大；密度紧凑 / 标准 / 宽松；中 / 英界面

配置写入可执行文件同目录的 `mqttc.json`，下次打开自动加载。旧版单连接配置会自动迁移。

## 界面

左侧是连接列表与次级入口，右侧按页切换：

| 入口 | 用途 |
|------|------|
| 左侧 `+` / 顶栏「编辑」 | 连接配置 |
| 选中连接 / 「返回会话」 | 会话：顶栏、左侧订阅列表、中间消息、底部发布 |
| 左侧「设置」 | 皮肤、字号、密度、语言 |
| 左侧「自动回复」 | 自动回复规则 |
| 左侧「发布预设」 | 发布预设编辑 |
| 左侧「关于」 | 项目介绍与版本号 |

订阅列表顶部可隐藏；隐藏后顶栏「断开」后面出现「订阅列表」可重新显示。

## 构建

环境：Rust stable。Windows / macOS / Linux 共用同一套源码。Linux 需要 `build-essential`、`pkg-config`、`libfontconfig1-dev`、`libxcb*`、`libxkbcommon-dev`。

```bash
# 开发运行
cargo run

# 测试（不依赖窗口）
cargo test

# 本机 release
cargo build --release

# Linux 上交叉编译 Windows
cargo build --release --target x86_64-pc-windows-gnu

# macOS 打包 .app（需在 Mac 上执行）
bash scripts/package-macos.sh
```

无显示环境时 `cargo run` 无法弹出窗口，属预期。`cargo test` 当前 36 项，覆盖编解码、配置迁移、皮肤键、订阅显隐、日志筛选、通配符与自动回复。

产物：

- Windows 本机：`target/release/mqttc.exe`
- Windows 交叉编译：`target/x86_64-pc-windows-gnu/release/mqttc.exe`
- Linux AMD64：`target/release/mqttc`
- macOS：`target/release/mqttc`，打包后为 `dist/mqttc.app`；GitHub Actions 在 tag `v*` 时编出通用包

## Linux 说明

适用于 Ubuntu 22.04+、Debian 12+ 的 AMD64 桌面（glibc 2.35+，X11 或 Wayland）。

```bash
chmod +x mqttc-linux-amd64
./mqttc-linux-amd64
```

若提示缺少共享库：

```bash
sudo apt-get install -y libfontconfig1 libfreetype6 libexpat1 zlib1g libpng16-16 libbrotli1 libx11-6 libxkbcommon-x11-0
```

## macOS 说明

系统要求 macOS 11.0+。预编译包为 Apple Silicon + Intel 通用二进制。本机编译需要 Xcode Command Line Tools：`xcode-select --install`。

```bash
cargo test
bash scripts/package-macos.sh
open dist/mqttc.app
```

首次打开若提示未验证开发者，在「系统设置 → 隐私与安全性」中允许，或执行：

```bash
xattr -dr com.apple.quarantine dist/mqttc.app
```

界面字体使用 PingFang SC 与 Menlo。窗口图标来自 `assets/icon.png`。

## 项目结构

```text
mqttc/
├── Cargo.toml              # 依赖、Slint 特性、release 优化
├── build.rs                # 编译 .slint；Windows 嵌入图标
├── ui/app-window.slint     # 主题、双语文案、六个页面
├── src/
│   ├── main.rs             # 状态、回调、事件、编解码、日志
│   ├── mqtt.rs             # MQTT 会话、TLS、事件循环线程
│   ├── config.rs           # 配置读写与旧格式迁移
│   └── i18n.rs             # 中英文切换
├── assets/                 # 图标与 Windows 资源
├── scripts/                # macOS 打包
├── dist/                   # 本机构建产物，不随源码提交
└── .monkeycode/docs/       # 使用、架构、开发文档
```

阅读顺序：`src/main.rs` 前 350 行弄清 `AppState` → `ui/app-window.slint` 前部弄清 `Theme` / `I18n` → `register_callbacks` → `src/mqtt.rs` → `src/config.rs`。更完整的分层与数据流见 [架构与源码导读](.monkeycode/docs/ARCHITECTURE_GUIDE.md)。

## 文档

- [使用说明](.monkeycode/docs/USER_GUIDE.md)
- [架构与源码导读](.monkeycode/docs/ARCHITECTURE_GUIDE.md)
- [架构设计](.monkeycode/docs/ARCHITECTURE.md)
- [接口定义](.monkeycode/docs/INTERFACES.md)
- [开发指南](.monkeycode/docs/DEVELOPER_GUIDE.md)
- [文档索引](.monkeycode/docs/INDEX.md)
- [参与贡献](CONTRIBUTING.md)
- [更新日志](CHANGELOG.md)
- [安全政策](SECURITY.md)

## 开源协议

本项目以 [MIT License](LICENSE) 发布。Copyright (c) 2026 Gordon Wen。界面使用 [Slint](https://slint.dev)，按免版税许可在关于页展示 Made with Slint。参与本仓库即表示同意 [行为准则](CODE_OF_CONDUCT.md)。
