# mqttc

跨平台桌面 MQTT 调试客户端。当前交付 Windows 最小可用版本：连接、订阅、发布、消息日志。协议为 MQTT 3.1.1。

## 环境

- Rust stable
- Windows / macOS / Linux 均可编译同一套源码

## 运行

```bash
cargo run
```

发布构建：

```bash
# 本机 Windows
cargo build --release

# 在 Linux 上交叉编译 Windows 64 位
cargo build --release --target x86_64-pc-windows-gnu
```

Windows 产物：

- 本机编译：`target/release/mqttc.exe`
- 交叉编译：`target/x86_64-pc-windows-gnu/release/mqttc.exe`

当前交叉编译好的文件已放到 当前工作区 `/dist/mqttc.exe`（约 36MB），拷到 Windows 后双击即可运行。

## 最小用法

1. 填写 Broker 主机与端口（默认 `127.0.0.1:1883`）
2. 点击连接
3. 订阅 Topic（默认 `mqttc/mvp/#`）
4. 发布文本消息，在日志中查看 SEND / RECV
