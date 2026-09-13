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
cargo build --release
```

Windows 产物：`target/release/mqttc.exe`

## 最小用法

1. 填写 Broker 主机与端口（默认 `127.0.0.1:1883`）
2. 点击连接
3. 订阅 Topic（默认 `mqttc/mvp/#`）
4. 发布文本消息，在日志中查看 SEND / RECV
