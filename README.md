# mqttc

<img src="assets/icon.png" alt="mqttc" width="96">

跨平台桌面 MQTT 调试客户端，用 Rust 和 [Slint](https://slint.dev) 编写，界面与交互参考 [MQTTX](https://github.com/emqx/MQTTX)。Windows 与 Linux 提供预编译产物，macOS 可在本机编译。协议支持 MQTT 3.1.1 与 MQTT 5.0。

[MQTTX](https://github.com/emqx/MQTTX) 很好用。发起本项目，是为了得到一份拷贝即可运行、体积和内存占用更小的桌面客户端。

这个项目适合两类读者：

- 需要快速验证 MQTT 连通性、观察消息流的开发者、测试与运维人员
- 想通过一个完整可运行的小项目，学习 Rust 桌面开发与 Slint 声明式 UI 的初学者

## 已实现的功能

### 连接管理

- 同时保存并运行多条 Broker 连接，左侧列表随时切换，各连接互不干扰
- 支持 TCP 与 TLS；TLS 可校验系统 CA、指定自定义 CA、附带客户端证书，也可关闭校验用于内网自签环境
- 可选 MQTT 3.1.1 或 MQTT 5.0；Client ID 留空时自动生成
- 可配置用户名密码、Keep Alive、连接超时、Clean Session（MQTT 5.0 对应 Clean Start）、Last Will
- 新建、编辑、删除连接；删除时断开该连接并停止其后台任务

### 订阅

- 连接后按 Topic 订阅，支持 `+`、`#` 通配符与 QoS 0/1/2
- 可同时订阅多条，每条可单独取消
- 点击订阅条目按该过滤器筛选消息，再点一次取消筛选
- 重连成功后按已保存列表自动重新订阅

### 发布

- 按 Topic、QoS、Retain 发布消息
- Payload 支持文本、JSON、Base64、Hex 四种格式；JSON 发送前校验，并可一键美化
- 定时循环发布，间隔 1 到 3600 秒；切换到其它连接后仍在后台继续
- 发布结果实时写入日志，失败时以 ERR 记录原因

### 发布预设

- 每条连接拥有独立的预设列表，在专门页面增减与编辑
- 会话页底部按列表显示预设发送按钮，固定一行，超出时横向滑动
- 点击按钮直接发送该组 Topic、Payload、QoS、Retain 与格式，不改底部表单，也不影响正在运行的定时发布

### 自动回复

- 每条连接拥有独立的规则列表，在专门页面增减与编辑
- 每条规则包含启用开关、匹配 Topic（支持通配符）、Payload 包含文本，以及固定回复的 Topic、Payload、QoS、Retain、格式
- 入站消息先记 RECV，再按列表顺序取第一条命中的已启用规则
- 自身回投可选「跳过」或「再匹配」；选择跳过时，刚发出的自动回复被 Broker 回投不会再次触发规则

### 日志与调试

- 按连接分别记录 RECV、SEND、SYS、ERR 四类事件，收发消息以聊天气泡左右对齐
- 每条连接内存中最多保留 200 条
- 支持关键字搜索、消息类型筛选、接收显示格式切换、自动滚动、清空
- 「保存日志」导出该连接全部记录到 `logs/mqttc-YYYYMMDD-HHMMSS.log`，每行带完整时间戳
- 筛选只影响界面展示，导出结果始终包含全部记录

### 外观与语言

- 皮肤：跟随系统、黑、白、蓝、绿、紫
- 跟随系统时按操作系统深浅色在黑、白之间切换
- 界面语言：中文、English，切换立即生效
- Windows 标题栏颜色跟随当前皮肤

## 界面导览

左侧始终是连接列表，右侧按页切换。页面用 `page` 整数标识，定义在 `ui/app-window.slint` 与 `src/main.rs` 顶部注释中。

| page | 入口 | 用途 |
|------|------|------|
| 0 | 左侧 `+` 或「编辑」 | 连接配置 |
| 1 | 选中连接 / 「返回会话」 | 会话：订阅、消息、发布、预设按钮 |
| 2 | 左侧「设置」 | 皮肤与语言 |
| 3 | 左侧「自动回复」 | 自动回复规则 |
| 4 | 左侧「发布预设」 | 发布预设编辑 |
| 5 | 左侧「关于」 | 项目介绍与版本号 |

窗口默认宽度约 1360px。启动后进入当前连接的会话页；点左侧 `+` 或「编辑」打开配置页。

## 最短上手路径

1. 启动一个可访问的 Broker，例如本地 Mosquitto 的 `127.0.0.1:1883`
2. 点左侧 `+`，填写主机与端口，选择 MQTT 3.1.1 或 5.0；需要认证时再填用户名和密码
3. 点右上角「连接」。成功后自动进入会话页
4. 在左侧订阅栏选择 QoS 后点「订阅」，例如 `mqttc/mvp/#`
5. 在底部填写 Topic 与 Payload 后点「发布」；收到的消息会出现在中间的气泡列表

完整的界面操作、自动回复与发布预设说明见 [使用说明](.monkeycode/docs/USER_GUIDE.md)。

## 项目结构

```text
mqttc/
├── Cargo.toml              # 依赖、Slint 特性、release 优化
├── build.rs                # 编译 .slint；Windows 嵌入图标并链接系统库
├── ui/app-window.slint     # 全部界面：主题、双语文案、组件、六个页面
├── src/
│   ├── main.rs             # 应用入口：状态、回调、事件处理、编解码、日志
│   ├── mqtt.rs             # MQTT 会话封装：协议版本、TLS、事件循环线程
│   ├── config.rs           # 配置结构、读写、旧格式迁移与字段规范化
│   └── i18n.rs             # 进程级中英文切换
├── assets/                 # 图标与 Windows 资源脚本
├── scripts/                # macOS 打包脚本与图标生成脚本
├── dist/                   # 本机构建产物，不随源码提交；发布包见 GitHub Releases
└── .monkeycode/docs/       # 使用、架构、开发文档
```

## 源码怎么读（给 Rust / Slint 初学者）

项目只有四个 Rust 文件和一份 Slint 文件，可以按「先骨架，再细节」的顺序阅读：

1. `src/main.rs` 的前 350 行：`AppState`、`ConnRuntime`、`with_state`，先弄清状态放在哪里
2. `ui/app-window.slint` 前 110 行：数据结构、`Theme`、`I18n`，先弄清界面能看到什么
3. `src/main.rs` 的 `register_callbacks`：每个用户操作对应哪段逻辑
4. `src/mqtt.rs`：连接如何建立、事件如何回到界面
5. `src/config.rs`：配置如何落盘与迁移

理解这个项目只需要抓住三点：

- Slint 负责「界面长什么样」，用声明式属性绑定表达，不写命令式布局代码
- Rust 主线程持有全部状态，Slint 只能在创建它的线程访问
- 每条连接有一个独立的 MQTT 工作线程，通过 `invoke_from_event_loop` 把事件送回主线程

想深入线程模型、数据流与 Slint 语法，请阅读 [架构与源码导读](.monkeycode/docs/ARCHITECTURE_GUIDE.md)。

## 环境

- Rust stable
- Windows / macOS / Linux 均可编译同一套源码
- Linux 编译需要 `build-essential`、`pkg-config`、`libfontconfig1-dev`、`libxcb*`、`libxkbcommon-dev`
- Windows 交叉编译需要 `x86_64-pc-windows-gnu` target 与 `gcc-mingw-w64-x86-64`

## 运行

```bash
# 开发构建并运行
cargo run
```

无显示环境时 `cargo run` 无法弹出窗口，属预期；`cargo test` 不依赖窗口。

## 发布构建

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

预编译 Windows / Linux 包发布在 [GitHub Releases](https://github.com/gordonwen/mqttc/releases)。本机构建时也可拷到 `dist/`：

- `dist/mqttc.exe`（约 16MB，拷到 Windows 后双击即可运行）
- `dist/mqttc-linux-amd64`（约 20MB）
- macOS `.app` 需在 Mac 上运行 `scripts/package-macos.sh` 生成

## 测试

```bash
cargo test
```

当前共有 33 个单元测试，覆盖 Payload 编解码、配置迁移与字段规范化、日志筛选、MQTT 通配符匹配、自动回复规则选择与自身回投判断等纯逻辑。测试集中在 `src/main.rs`、`src/config.rs`、`src/mqtt.rs`、`src/i18n.rs` 底部的 `#[cfg(test)]` 模块。

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

## 使用细节

### 连接

配置页可设置名称、主机、端口、协议、认证、Keep Alive、连接超时、Clean Session、SSL/TLS 与 Last Will。勾选 SSL 会把默认端口切到 8883。多个连接可以同时在线，点左侧条目切换。

删除连接：点左侧条目右侧的 `×`。

### Payload 格式

- 文本：原样发送
- JSON：发送前校验合法性，点「美化」可格式化编辑框内容
- Base64 / Hex：编辑框内容按该编码解码为字节后发送

### 日志

日志区支持 Topic 与 Payload 多行换行显示。右上角「自动滚动」勾选后新日志会自动滚到底部，「接收格式」可切换日志中 Payload 的显示方式。筛选只影响界面展示，保存日志仍导出当前连接的全部记录。点击「保存日志」会写出：

```text
[2026-09-15 10:20:30] [RECV] mqttc/mvp/hello hello from mqttc
```

### 自动回复与发布预设

两者的编辑入口都在左侧栏，各自有独立页面。会话页只负责触发发送。自动回复的规则顺序就是匹配顺序，同一条入站消息只触发第一条命中的已启用规则。

### 设置与关于

左侧连接列表底部进入「设置」页，可切换皮肤与界面语言。选择会随其它界面偏好写入 `mqttc.json`。「关于」在「设置」下方，展示项目介绍、说明、版本号与 Made with Slint 署名。

## 配置文件

全部连接与界面偏好写入可执行文件同目录的 `mqttc.json`，下次打开自动加载；旧版单连接配置会自动迁移。缺发布预设或自动回复字段时对应列表为空；自身回投缺省为跳过。

## 文档

- [使用说明](.monkeycode/docs/USER_GUIDE.md)
- [架构与源码导读](.monkeycode/docs/ARCHITECTURE_GUIDE.md)（面向初学者）
- [架构设计](.monkeycode/docs/ARCHITECTURE.md)（面向维护者）
- [接口定义](.monkeycode/docs/INTERFACES.md)
- [开发指南](.monkeycode/docs/DEVELOPER_GUIDE.md)
- [文档索引](.monkeycode/docs/INDEX.md)
- [参与贡献](CONTRIBUTING.md)
- [更新日志](CHANGELOG.md)
- [安全政策](SECURITY.md)

## 开源协议

本项目以 [MIT License](LICENSE) 发布。Copyright (c) 2026 Gordon Wen。全文见仓库根目录 `LICENSE`。界面使用 [Slint](https://slint.dev)，按免版税许可在关于页展示 Made with Slint。参与本仓库即表示同意 [行为准则](CODE_OF_CONDUCT.md)。
