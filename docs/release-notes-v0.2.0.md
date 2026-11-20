# mqttc v0.2.0

一个简单轻便易用的 MQTT 调试客户端。免安装，解压即用。

## 下载

| 平台 | 文件 |
|---|---|
| Windows 10/11 x64 | `mqttc-v0.2.0-windows-x64.zip` |
| Linux x64（glibc 2.35+） | `mqttc-v0.2.0-linux-x64.tar.gz` |
| macOS 11+（Apple Silicon / Intel） | `mqttc-v0.2.0-macos-universal.zip` |

解压后运行 `mqttc`（macOS 打开 `mqttc.app`）。配置与导出的日志写在可执行文件同目录，
不写注册表、不写用户目录。

## 功能

- 多连接管理，每条连接独立会话
- MQTT 3.1.1 / 5.0，QoS 0/1/2、Clean Session、Keep Alive、Last Will
- TLS：系统根证书、自定义 CA、客户端证书 / 私钥、跳过校验
- 订阅通配符、按 QoS 订阅、点击订阅项筛选日志
- 文本 / JSON / Base64 / Hex 发布与互转，JSON 美化
- 定时发布、发布预设、按主题/关键字/负载自动回复
- 聊天式日志：搜索、筛选、格式切换、自动滚动、导出；每连接 200 条
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

exe 5.83 MB，portable zip 2.91 MB；空闲 CPU 接近 0%。对比数据见仓库
`docs/BENCHMARK.md`。

## 说明

- 构建未签名。macOS 首次打开请右键 → 打开，或执行
  `xattr -dr com.apple.quarantine mqttc.app`。
- Linux 需要 `libxkbcommon`、Wayland 或 X11、`fontconfig`；详见包内 READ-ME。
- 校验：每个资产附 `.sha256`，Release 中另有 `SHA256SUMS.txt`。

源码、问题反馈：https://github.com/GordonWen/mqttc · MIT License
