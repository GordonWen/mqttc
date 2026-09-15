# mqttc

跨平台桌面 MQTT 调试客户端。当前交付 Windows 与 Linux 最小可用版本：连接、订阅、发布、消息日志。协议为 MQTT 3.1.1。

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

# 本机 Linux AMD64
cargo build --release
```

产物：

- Windows 本机编译：`target/release/mqttc.exe`
- Windows 交叉编译：`target/x86_64-pc-windows-gnu/release/mqttc.exe`
- Linux AMD64：`target/release/mqttc`

已构建好的文件放在 当前工作区 `/dist/`：

- `/dist/mqttc.exe`（约 37MB，拷到 Windows 后双击即可运行）
- `/dist/mqttc-linux-amd64`（约 25MB）

## Linux 说明

适用于 Ubuntu、Debian 的 AMD64（x86_64）桌面环境。

```bash
chmod +x mqttc-linux-amd64
./mqttc-linux-amd64
```

系统要求：

- glibc 2.35 及以上，即 Ubuntu 22.04+、Debian 12+
- 图形后端为 X11 或 Wayland，需要有可用的桌面会话

运行时依赖通常是桌面系统的常见库。若提示缺少共享库，安装：

```bash
sudo apt-get install -y libfontconfig1 libfreetype6 libexpat1 zlib1g libpng16-16 libbrotli1 libx11-6 libxkbcommon-x11-0
```

## 最小用法

1. 填写 Broker 主机与端口（默认 `127.0.0.1:1883`）
2. 需要认证时填写用户名和密码，留空则匿名连接
3. 点击连接
4. 订阅 Topic（默认 `mqttc/mvp/#`），可多次订阅不同 Topic，每个订阅在「订阅列表」中显示一行，点右侧 `×` 取消订阅
5. 发布文本消息，在日志中查看 SEND / RECV。Payload 为多行编辑框，可直接回车换行，换行会原样发送

日志区支持 Topic 与 Payload 多行换行显示，右上角「自动滚动」勾选后新日志会自动滚到底部。点击「保存日志」会把当前全部日志导出为 `logs/mqttc-YYYYMMDD-HHMMSS.log`，每行带完整时间戳：

```
[2026-09-15 10:20:30] [RECV] mqttc/mvp/hello hello from mqttc
```

连接参数会写入可执行文件同目录的 `mqttc.json`，下次打开自动加载。
