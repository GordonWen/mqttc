# 更新日志

本文件遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [Unreleased]

## [0.1.2] - 2026-11-03

### 新增

- GitHub Actions 在 macOS 上云编译 Apple Silicon + Intel 通用包
- 设置页可调字号（小 / 中 / 大 / 特大）与界面密度（紧凑 / 标准 / 宽松），立即生效并写入 `mqttc.json`
- 默认字号改为 12px，对齐 Windows 标题栏常用字号；窗口变窄时会话工具栏可横向滑动
- 单行输入框高度跟控件 Token，文字左对齐
- 全窗口对齐 MQTTX Desktop 聊天式骨架：左栏连接、会话顶栏、消息区左侧可隐藏订阅列表、中间气泡、底部发布
- 皮肤改为跟随系统 / 浅色 / 深色 / 夜间；无配置时默认跟随系统；浅色主题左栏与工作区同为浅色
- 订阅列表显隐写入 `mqttc.json` 的 `subs_visible`；隐藏后顶栏「断开」后可重新显示

## [0.1.1] - 2026-10-23

### 文档

- 补充项目发起缘由：MQTTX 很好用，mqttc 面向拷贝即可运行、更小的体积与内存占用

## [0.1.0] - 2026-10-22

### 新增

- 跨平台桌面 MQTT 调试客户端，支持 MQTT 3.1.1 与 MQTT 5.0
- 多连接并行：独立日志、订阅、发布预设与自动回复
- TCP / TLS（系统 CA、自定义 CA、客户端证书、可关闭校验）
- 订阅通配符、QoS 0/1/2，重连后自动恢复订阅
- 发布支持文本 / JSON / Base64 / Hex，以及 1–3600 秒定时发布
- 每连接可增减的发布预设与自动回复规则，自身回投可跳过
- 皮肤（跟随系统、浅色、深色、夜间）与中/英界面
- 关于页、MIT 许可证与 Made with Slint 署名
- macOS 本机打包脚本 `scripts/package-macos.sh`

### 变更

- Slint 升级至 1.18.1
- 发布预设与自动回复各自独立成页
- 图形后端固定为 `winit-software`，降低启动内存
