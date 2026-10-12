# 开发指南

## 环境要求

- Rust stable（rustup），当前开发环境 rustc 1.98.1
- 系统库（Linux 编译）：`build-essential`、`pkg-config`、`libfontconfig1-dev`、`libxcb*`、`libxkbcommon-dev`
- Windows：MSVC Build Tools，或 GNU 工具链 `x86_64-pc-windows-gnu`（Linux 交叉编译需 `gcc-mingw-w64-x86-64`）
- macOS：Xcode Command Line Tools，在本机执行 `cargo build --release`（无法从 Linux 交叉编译 Darwin）
- 一台可访问的 MQTT Broker（本地 Mosquitto 或 `test.mosquitto.org:1883`）

## 快速开始

```bash
# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

# 编译
cargo build

# 运行
cargo run
```

无显示环境时 `cargo run` 无法弹出窗口，属预期。`cargo test` 不依赖窗口。

## 项目结构说明

- `ui/app-window.slint`：MQTTX 式多页布局，含连接配置、会话、设置、自动回复、发布预设、关于（含 `AboutSlint` 署名），以及 `Select`、聊天气泡、预设发送按钮、自动回复规则行
- `src/main.rs`：窗口创建与回调、多连接运行时、Payload 编解码、日志筛选、定时发布、自动回复匹配发送、发布预设增删
- `src/mqtt.rs`：单连接 MQTT 会话与事件，含协议版本、Keep Alive / Clean Session / 超时 / Last Will / TLS
- `src/config.rs`：多连接配置 `mqttc.json` 的读写、旧格式迁移、发布预设与自动回复规范化
- `src/i18n.rs`：进程级中/英切换
- `build.rs`：编译 `.slint` 文件；Windows 额外嵌入图标并链接 dwmapi/user32
- `scripts/package-macos.sh`：在 Mac 上打包 `dist/mqttc.app`
- `dist/`：已构建的 Windows exe 与 Linux AMD64 二进制
- `LICENSE`：MIT 开源协议全文

## 开发规范

- 模块、类型、函数使用 Rust 惯例命名
- 界面文案走 `I18n.t("中文", "English")` 与 `i18n::t`，不要只写中文
- 不在代码中硬编码密钥；用户名密码由界面输入并写入本地配置
- 提交信息使用 `feat` / `fix` / `chore` / `docs` 前缀
- 新增的纯逻辑（编解码、配置迁移、规则匹配）需附单元测试
- 改 Web 以外的桌面源码后，需要时刷新 `dist/mqttc.exe` 与 `dist/mqttc-linux-amd64`

## 测试

```bash
cargo test
```

当前覆盖：Payload 编解码（文本 / JSON / Base64 / Hex）、格式 / 皮肤 / 协议 / 语言索引与键往返、日志筛选、定时间隔校验、MQTT 通配符过滤器、配置序列化与旧格式迁移、发布预设缺字段加载与 QoS/格式规范化、自动回复缺字段加载、Echo Policy 规范化、`select_auto_reply_rule` 第一命中 / 未勾选 / 空匹配 Topic / 大小写包含 / 空包含、自身回投字节比较。

`cargo test` 峰值内存约 2.1GiB。release 构建因 LTO 更慢。无显示环境冒烟属预期。

## 常见任务

- 改界面：编辑 `ui/app-window.slint` 后 `cargo build`
- 改协议行为：编辑 `src/mqtt.rs`
- 改连接模型或持久化：编辑 `src/config.rs` 与 `src/main.rs` 的 `AppState`
- 改自动回复匹配或发送：编辑 `src/main.rs` 中 `select_auto_reply_rule` / `maybe_auto_reply`
- 增加日志种类：扩展 `LogEntry.kind` 并在 UI 着色

## 构建与发布

```bash
# 调试构建
cargo build

# 本机发布构建
cargo build --release

# Linux 交叉编译 Windows 64 位 exe
# 需要：rustup target add x86_64-pc-windows-gnu
# 以及 gcc-mingw-w64-x86-64
cargo build --release --target x86_64-pc-windows-gnu

# macOS 本机发布（必须在 Mac 上执行）
cargo build --release
bash scripts/package-macos.sh
```

Windows 发布产物：

- 本机：`target/release/mqttc.exe`
- 交叉编译：`target/x86_64-pc-windows-gnu/release/mqttc.exe`

Linux 本机产物名为 `mqttc`。已拷贝到 `dist/` 的文件：

- `dist/mqttc.exe`（约 16MB）
- `dist/mqttc-linux-amd64`（约 21MB）

产物为单文件，Windows 依赖系统 DLL（gdi32、user32 等），无需额外安装 MinGW 运行库。macOS 本机产物同样名为 `mqttc`，打包后为 `dist/mqttc.app`。
