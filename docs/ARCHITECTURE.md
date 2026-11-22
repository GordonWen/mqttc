# 架构说明

简体中文 | [English](ARCHITECTURE.en.md)

本文描述 workspace 结构、共享核心 `mqttc-core` 的职责划分、线程与事件模型，以及三套 UI
如何以最小差异接入同一份业务逻辑。

## 1. Workspace 结构

```text
Cargo.toml            workspace: core, apps/*, tools/test-broker
core/                 mqttc-core —— 与 UI 无关的全部业务
apps/egui/            eframe 0.37（glow/OpenGL）
apps/iced/            Iced 0.14（tiny-skia 软件渲染，发布版）
apps/gpui/            GPUI 0.2.2（原生 GPU）
tools/test-broker/    内嵌 rumqttd 0.20 的本地 Broker + retained fixture + 流式发布
tools/bench/          PowerShell 采样 / 场景编排 / fixture（外部服务器配置仅本机）
scripts/              构建、打包与发布脚本
.github/workflows/    release.yml：tag 触发 Linux x64 与 macOS universal 构建
```

早期 Slint 版本（v0.1.1）与 `bench/ref-build` 仅作为本机性能基线保留，不随仓库分发，
也不在 workspace 内；本仓库的任何构建命令都不会触碰它们。

发布时只构建 `mqttc-iced`，产物重命名为 `mqttc`。Windows 由 `scripts/release-windows.ps1`
本地打包；Linux 与 macOS 由 `.github/workflows/release.yml` 构建，产物与 Windows 一起
附加到同一个 GitHub Release。详见 [RELEASING.md](RELEASING.md)。

## 2. 共享核心 `mqttc-core`

三个 UI 只做“画像素 + 路由输入”，所有会产生行为差异的逻辑都在核心内，只有一份实现：

| 模块 | 职责 |
|---|---|
| `model` | `PayloadFormat`、`Skin`、`ConnStatus`、`LogRecord`、Client ID 生成、数值钳制（Keep Alive 1..65535、Timeout 1..300、QoS 0..2、定时 1..3600） |
| `config` | `ConnectionConfig` / `PublishPreset`（按列表顺序，可手动调序）/ `AutoReplyRule` / `AppConfig`（含 `custom_palette`、`font_scale`）及 serde 默认值；`mqttc.json` 读写；旧版单连接迁移；`active_id` 修复；预设/规则补齐；原子写（临时文件 + rename，Windows 回退直写） |
| `payload` | text / json / base64 / hex 编解码，hex 容错（空格、逗号、冒号、连字符），JSON 美化，日志渲染 |
| `filter` | MQTT `+` / `#` 通配符匹配；关键字 / 类型 / 订阅主题筛选 |
| `i18n` | 进程级 `AtomicU8` 语言 + `t(zh, en)`；工作线程也能调用 |
| `theme` | 7 套皮肤（System 解析为黑/白，Custom 由 `custom_palette` 解析）；27 个命名颜色的 `Palette`，`#rrggbb` 覆盖逐槽回退，各 UI 转成自己的颜色类型 |
| `platform` | 系统深浅色检测：Windows 读注册表（缓存 1s）；macOS `defaults` / Linux `gsettings`（结果缓存 30s，避免频繁起进程） |
| `mqtt` | `MqttHandle`：rumqttc 0.25.1，3.1.1 与 5.0 双客户端；generation 失效旧线程；通道容量 32；Keep Alive / Clean / Timeout / Will；rustls 自定义 CA、客户端证书、跳过校验 |
| `logs` | 每连接 `VecDeque<LogRecord>`，默认上限 `MAX_LOGS = 200`（设置可调 50–2000）；只存原始字节 `Arc<[u8]>`，切换显示格式时重渲染 |
| `form` | `FormState`：编辑页与会话页所有字段的字符串/布尔草稿 + 预设行 + 规则行；`from_config` / `apply_to` / 行增删；预设置顶与显示排序 |
| `runtime` | `AppRuntime`：配置、表单、每连接 `ConnRuntime`、页面状态、偏好；动作入口与自动回复/定时发布状态机 |
| `export` | 日志导出到 exe 同目录 `logs/`（CSV / 文本，文件名带时间戳） |
| `cli` | `--bench --bench-scenario s1..s4 --bench-quit-after N --config <path>` 解析 |

### 表单与配置的一致性

配置 → 表单 → 配置的路径只有一条：

```text
mqttc.json ──load──▶ AppConfig ──from_config──▶ FormState ──UI 绑定──▶ 用户编辑
                                                        │
                                          apply_to（钳制/回填）
                                                        ▼
                                                   ConnectionConfig ──save──▶ mqttc.json
```

- 切换连接、执行动作（连接、订阅、发布、关闭）前统一 `apply_form()`，避免三份 UI 各自同步字段。
- 纯文本编辑走 `mark_dirty()` + 400ms 防抖 `maybe_persist()`；动作触发立即保存。
- 空 Client ID 在连接时自动生成 `mqttc-<millis hex>`；空名称回填 `host:port`。

## 3. 线程与事件模型

```text
UI 线程                                  工作线程（每连接 1 个 + 定时发布线程）
┌─────────────────────────┐              ┌──────────────────────────────┐
│ AppRuntime              │              │ rumqttc Connection::iter()   │
│  ├─ EventSink.drain ────┼── 队列 ◀─────┤  push(RuntimeEvent::Mqtt)   │
│  ├─ poll()              │              │  wake() ────────────────────┼─▶ request_repaint()
│  └─ auto-reply / 定时发送│              └──────────────────────────────┘
└─────────────────────────┘
```

- UI 线程独占 `AppRuntime`（egui/iced/gpui 各自用 `Rc<RefCell<AppRuntime>>`、`App` 状态或 `Entity<Root>` 包装）。
- 跨线程只有三样东西：`MqttHandle`、`Arc<EventSink>`、`Arc<AtomicU64>`（定时代数）。
- MQTT 事件循环是阻塞的 std 线程，栈 256KB；`generation` 计数保证重连/断开后旧线程自行退出。
- 工作者从不读取配置：定时发布只推 `TimedTick`，由 UI 线程在 `poll()` 中用当前配置发送。
- `poll() -> PollSummary { changed, status_changed, page_request }`，UI 只在 `changed` 时刷新对应区域；
  `page_request` 在握手完成时请求切到会话页。

### 自动回复与定时发布

- `poll()` 处理 `Recv` 时：先记 RECV（原始字节），再按已启用规则顺序匹配（主题通配符 + 关键字 + 负载），
  首条命中即发送并记录 `last_auto_reply`；自身回投按原始字节判定。
- 定时发布：首 tick 立即发送，之后按间隔；断开即停止；切换连接不会中断（与早期 Slint 版一致）。

## 4. 三套 UI 的接入差异

| | egui | Iced | GPUI |
|---|---|---|---|
| 渲染 | glow / OpenGL | tiny-skia 软件光栅 | GPU（Windows Direct3D 11 / macOS Metal / Linux Vulkan） |
| 唤醒 | `ctx.request_repaint()`（事件到达即重绘） | 100ms tick（忙时）/ 1s（跟随系统皮肤空闲）/ 否则不 tick | 自适应 ticker：100ms 忙时 / 1s 系统皮肤空闲 / 否则不 tick |
| 日志列表 | `ScrollArea` 渲染全部行 + `stick_to_bottom` | 最新 80 条窗口化布局 + 上滚冻结 / 显示更早 / 回到最新 | `uniform_list` 固定行高虚拟化 |
| 中文字体 | fontique 系统字体回退（不打包 CJK） | cosmic-text 系统回退，默认族按平台选 YaHei / PingFang | 系统字体回退 |
| 下拉/选择 | `ComboBox` | `pick_list` | 自绘点击循环选择器（无弹出层） |
| 输入 | `TextEdit`（密码 `password(true)`） | `text_input` / `text_editor` | 从官方 example 移植的 IME `TextInput`（支持密码遮罩） |
| 页面结构 | 侧栏全局页 + 会话内订阅卡 | 侧栏 4 页 + 会话标签（日志 / 订阅 / 自动回复 / 预设回复） | 侧栏全局页 |
| 标题栏 | 原生标题栏 | 无边框 + 自绘标题栏（含置顶图钉） | 自绘 `TitlebarOptions` |

GPUI 的取舍（规则/预设编辑页只读等）在 [PARITY.md](PARITY.md) 中逐条记录。

## 5. 体积与内存策略

- 只启用必要 feature：egui 关闭 wgpu/accesskit/persistence；Iced 关闭 wgpu/image/svg/web，启用 tiny-skia；
  网络栈仅 rumqttc + rustls。
- 不打包 CJK 字体（动辄 10–20MB），全部走系统字体回退。
- Release profile：`opt-level=3`、thin LTO、`codegen-units=1`、`strip="symbols"`、`panic="abort"`；
  交付对照另建 `release-size`（`opt-level="s"`、fat LTO）。GPUI 因内部 `catch_unwind` 使用
  `release-unwind`（仅保留 unwind，其余相同）。
- 不使用 `+crt-static`（反而更大，且与 TLS/插件生态冲突）；文档标注 VC 运行库前置条件。
- 运行时：日志环缓冲默认 200 条（设置中可调至 2000，另设 16 MB 字节预算）、只存字节、可见行虚拟化 / 窗口化、配置防抖保存、无固定帧率动画。

## 6. 测试

```text
cargo test -p mqttc-core          # 70 项：编解码、通配符、筛选、配置迁移/往返、皮肤/自定义配色、字号钳制、自动回复、表单钳制、200 上限
cargo test -p mqttc-core --test broker_e2e        # 3 项：双协议连接、订阅、发布、断开
cargo test -p mqttc-core --test broker_roundtrip  # 5 项 + 1 #[ignore] TLS：连接/订阅/发布/接收/自动回复/定时/重连/200 上限
cargo test -p mqttc-iced          # 15 项：页面构建、预设折叠、大字号、删除确认等
```

本地集成测试使用 `tools/test-broker` 的 `start_broker`（rumqttd 0.20 内嵌），不使用公网服务；
TLS 用例需要证书，默认 `#[ignore]`。最近一次运行：70 + 3 + 5 + 15 全部通过。

## 7. 便携性

- `config::config_path()` = 可执行文件所在目录 / `mqttc.json`；`log_dir()` = 同目录 / `logs/`。
- 不存在可执行目录写权限时：错误写 stderr，UI 继续可用（不闪退）。
- 发布包只含可执行文件 + 示例 `mqttc.json` + 简短说明；不写注册表、不写用户目录。
  Windows / Linux 解压到任意目录；macOS 的配置放在 `mqttc.app/Contents/MacOS/`，
  语义仍是“与可执行文件同目录”。
- 已实测：从含空格与中文的目录（`portable test 中文`）解压后直接运行、读取并写入同目录配置。
- Linux 产物在 Ubuntu 22.04 上构建（glibc 2.35+），macOS 产物为 arm64 + x86_64 universal，
  均未签名。
