# 开发指南

## 环境要求

- Rust stable（rustup）
- 系统库（Linux 编译）：`build-essential`、`pkg-config`、`libfontconfig1-dev`、`libxcb*`、`libxkbcommon-dev`
- Windows：MSVC Build Tools 或与 Rust GNU 工具链匹配的环境
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

## 项目结构说明

- `ui/app-window.slint`：界面布局与数据绑定，含 `Segmented` 分段选择器组件
- `src/main.rs`：窗口创建与回调、多连接运行时状态、Payload 编解码
- `src/mqtt.rs`：单连接 MQTT 会话与事件
- `src/config.rs`：多连接配置 `mqttc.json` 的读写与旧格式迁移
- `build.rs`：编译 `.slint` 文件

## 开发规范

- 模块、类型、函数使用 Rust 惯例命名
- UI 文案使用中文
- 不在代码中硬编码密钥；用户名密码由界面输入并写入本地配置
- 提交信息使用 `feat` / `fix` / `chore` 前缀
- 新增的纯逻辑（编解码、配置迁移）需附单元测试

## 测试

```bash
cargo test
```

覆盖 Payload 编解码（文本 / JSON / Base64 / Hex）、格式索引与键的往返、以及配置的序列化与旧格式迁移。

## 常见任务

- 改界面：编辑 `ui/app-window.slint` 后 `cargo build`
- 改协议行为：编辑 `src/mqtt.rs`
- 改连接模型或持久化：编辑 `src/config.rs` 与 `src/main.rs` 的 `AppState`
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
```

Windows 发布产物：

- 本机：`target/release/mqttc.exe`
- 交叉编译：`target/x86_64-pc-windows-gnu/release/mqttc.exe`

产物为单文件 exe，依赖 Windows 系统 DLL（gdi32、user32、opengl32 等），无需额外安装 MinGW 运行库。macOS / Linux 本机产物名为 `mqttc`。
