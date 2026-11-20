# mqttc v0.2.3

一个简单轻便易用的 MQTT 调试客户端。免安装，解压即用。

## 下载

| 平台 | 文件 |
|---|---|
| Windows 10/11 x64 | `mqttc-v0.2.3-windows-x64.zip` |
| Linux x64（glibc 2.35+） | `mqttc-v0.2.3-linux-x64.tar.gz` |
| macOS 11+（Apple Silicon / Intel） | `mqttc-v0.2.3-macos-universal.zip` |

解压后运行 `mqttc`（macOS 打开 `mqttc.app`）。配置与导出的日志写在可执行文件同目录，
不写注册表、不写用户目录。

## 本版改动

### 复制

- 每条日志常驻「复制 / 详情」入口；详情面板可分别复制内容或整行，
  `Ctrl+Shift+C` 直接复制当前行
- 复制出的文本与导出的日志行格式一致，包含 QoS / Retain / DUP / 过期时间与用户属性

### 自动重连

- 断线后按 1 / 2 / 4 / 8 / 16 / 30 秒退避重试，最多 8 次；
  标题栏显示 `重连 n/8 · Ns 后`
- 重连成功后自动恢复订阅；手动断开不再重连
- 所有连接都会重试，后台会话断开后也能自行恢复

### TLS 与配置安全

- 把 `.pem` / `.crt` / `.key` 文件拖进窗口，即可填入空槽并自动启用 SSL
- 连接前检查证书文件是否存在，路径失效时给出明确提示
- 删除连接改为确认弹窗，确认后才会写回 `mqttc.json`

### 日志元数据

- 时间精确到毫秒（`HH:MM:SS.mmm`）
- 每条记录追加 QoS、Retain、DUP、消息过期与用户属性；无属性时不再出现多余空格，
  导出与复制格式同步

### 自动回复

- 按顺序匹配，首条可回复的规则生效
- 每条规则可设置最小间隔秒数，显示命中次数并可一键重置
- `↑` / `↓` 调整规则优先级

### MQTT 5

- 发布支持用户属性（`k=v;k2=v2`）与消息过期
- 订阅支持 No Local 与 Retain As Published（仅 MQTT 5 生效）
- PUBACK / SUBACK / UNSUBACK 原因码写入日志

### 快捷键

- `Enter` 或 `Ctrl+Enter` 发布，`Ctrl+F` 聚焦搜索，`Ctrl+L` 清空日志
- `↑` 取回上一条发送内容，`Esc` 关闭删除确认框

### 界面

- 最小窗口降到 980×640，日志工具栏拆成两行以适配窄窗口

## 功能

- 多连接管理，每条连接独立会话
- MQTT 3.1.1 / 5.0，QoS 0/1/2、Clean Session、Keep Alive、Last Will
- TLS：系统根证书、自定义 CA、客户端证书 / 私钥、跳过校验
- 订阅通配符、按 QoS 订阅、No Local / Retain As Published、点击订阅项筛选日志
- 文本 / JSON / Base64 / Hex 发布与互转，JSON 美化
- 定时发布、发布预设、按主题/关键字/负载自动回复
- 聊天式日志：搜索、筛选、格式切换、自动滚动、暂停、复制、导出；默认 200 条，可调至 2000
- 中英文界面，6 套皮肤
- 便携：`mqttc.json` 与 `logs/` 均在 exe 同目录

## 轻量

Windows x64 release 实测（3 次独立进程中位数）：

| 场景 | 工作集 |
|---|---|
| 启动空闲 | 28.8 MB |
| 连接 + 200 条日志 | 30.8 MB |
| 10 msg/s 流式接收 | 30.8 MB |
| 1 Hz 定时发布 | 31.2 MB |

空闲 CPU 接近 0%。对比数据见仓库 `docs/BENCHMARK.md`。

## 说明

- 构建未签名。macOS 首次打开请右键 → 打开，或执行
  `xattr -dr com.apple.quarantine mqttc.app`。
- Linux 需要 `libxkbcommon`、Wayland 或 X11、`fontconfig`；详见包内 READ-ME。
- 校验：每个资产附 `.sha256`，Release 中另有 `SHA256SUMS.txt`（含三平台）。

源码、问题反馈：https://github.com/GordonWen/mqttc · MIT License
