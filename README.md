# mqttc

跨平台桌面 MQTT 调试客户端，功能与界面参考 [MQTTX](https://github.com/emqx/MQTTX)。当前交付 Windows 与 Linux 版本；macOS 需在本机编译。功能包括多连接管理、订阅、发布、发布预设、自动回复、消息日志。协议支持 MQTT 3.1.1 与 MQTT 5.0。

更完整的使用说明见 当前工作区 `/.monkeycode/docs/USER_GUIDE.md`。

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

# 本机 macOS（Apple Silicon 或 Intel，需在 Mac 上执行）
cargo build --release
bash scripts/package-macos.sh
```

产物：

- Windows 本机编译：`target/release/mqttc.exe`
- Windows 交叉编译：`target/x86_64-pc-windows-gnu/release/mqttc.exe`
- Linux AMD64：`target/release/mqttc`
- macOS 本机：`target/release/mqttc`，打包后为 `dist/mqttc.app`

已构建好的文件放在 当前工作区 `/dist/`：

- `/dist/mqttc.exe`（约 16MB，拷到 Windows 后双击即可运行）
- `/dist/mqttc-linux-amd64`（约 21MB）
- macOS `.app` 需在 Mac 上运行 `scripts/package-macos.sh` 生成

## 测试

```bash
cargo test
```

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

## macOS 说明

当前 Linux 构建环境无法交叉编译 Darwin 二进制，macOS 包需要在 Mac 上完成本机编译。

```bash
# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

# 拉取源码后
cargo test
bash scripts/package-macos.sh
open dist/mqttc.app
```

系统要求：

- macOS 11.0 及以上
- Apple Silicon（`aarch64-apple-darwin`）或 Intel（`x86_64-apple-darwin`）均可，按本机架构编译
- Xcode Command Line Tools：`xcode-select --install`
- 首次打开若提示未验证开发者，可在「系统设置 → 隐私与安全性」中允许，或执行：

```bash
xattr -dr com.apple.quarantine dist/mqttc.app
```

窗口图标使用 `assets/icon.png`；`.app` 的 Dock / Finder 图标由脚本用同一张图生成 `mqttc.icns`。界面字体在 macOS 上使用 PingFang SC 与 Menlo。

## 最小用法

界面按 MQTTX 拆成五页：左侧始终是连接列表；右侧默认是会话页，点 `+` 或「编辑」进入连接配置页，点「自动回复」进入规则页，点「发布预设」进入预设编辑页，点「设置」进入皮肤与语言页。

1. 左侧连接列表点击 `+` 进入配置页，填写名称、主机与端口（默认 `127.0.0.1:1883`），并选择 MQTT 3.1.1 或 5.0
2. 需要认证时填写用户名和密码，留空则匿名连接。同一页还可设置 Keep Alive、连接超时、Clean Session、SSL/TLS 与 Last Will。勾选 SSL 会把默认端口切到 8883。MQTT 5.0 的 Clean Session 对应 Clean Start
3. 点右上角「连接」。成功后自动进入会话页。多个连接可同时在线，点左侧条目切换
4. 会话页左侧是订阅栏：选择 QoS 后点「订阅」。点击某条订阅可按该 Topic（含通配符）筛选消息，再点一次取消筛选
5. 会话页中间是聊天气泡。底部发布栏一行放下 Topic、QoS、Retain、格式和定时，下面是矮 payload 输入框。再往下是当前连接的预设发送按钮，固定一行，超出时横向滑动。点哪条就发哪组 Topic、Payload、QoS、Retain 与格式，不改底部表单。定时发布仍用底部表单
6. 左侧「自动回复」打开独立规则页。每条规则有自己的启用勾选、匹配 Topic、Payload 包含，以及固定回复的 Topic / Payload / QoS / Retain / 格式。勾选后才参与匹配；同一条入站消息只触发第一条命中的规则。自身回投默认为跳过
7. 左侧「发布预设」打开独立编辑页，条数可增减。会话页只负责点发送

删除连接：点左侧条目右侧的 `×`。

Payload 格式说明：

- 文本：原样发送
- JSON：发送前校验合法性，点「美化」可格式化编辑框内容
- Base64 / Hex：编辑框内容按该编码解码为字节后发送

日志区支持 Topic 与 Payload 多行换行显示，右上角「自动滚动」勾选后新日志会自动滚到底部，「接收格式」可切换日志中 Payload 的显示方式。筛选只影响界面展示，保存日志仍导出当前连接的全部记录。点击「保存日志」会把当前连接的全部日志导出为 `logs/mqttc-YYYYMMDD-HHMMSS.log`，每行带完整时间戳：

```
[2026-09-15 10:20:30] [RECV] mqttc/mvp/hello hello from mqttc
```

左侧连接列表底部进入「设置」页，可切换跟随系统、黑、白、蓝、绿、紫皮肤，以及中文 / English 界面语言。跟随系统时按操作系统深浅色在黑、白之间切换。蓝色为海军蓝、绿色为青绿、紫色为深紫罗兰，与炭黑底色区分开。选择会随其它界面偏好写入 `mqttc.json`。 Windows 标题栏颜色会跟随当前皮肤，下次启动也会按上次皮肤着色；程序图标为雷达风格。QoS、消息类型和文本格式使用紧凑下拉，窗口默认宽度约 1360px。

全部连接与界面偏好写入可执行文件同目录的 `mqttc.json`，下次打开自动加载；旧版单连接配置会自动迁移。
