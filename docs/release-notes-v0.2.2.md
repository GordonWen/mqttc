# mqttc v0.2.2

一个简单轻便易用的 MQTT 调试客户端。免安装，解压即用。

## 下载

| 平台 | 文件 |
|---|---|
| Windows 10/11 x64 | `mqttc-v0.2.2-windows-x64.zip` |
| Linux x64（glibc 2.35+） | `mqttc-v0.2.2-linux-x64.tar.gz` |
| macOS 11+（Apple Silicon / Intel） | `mqttc-v0.2.2-macos-universal.zip` |

解压后运行 `mqttc`（macOS 打开 `mqttc.app`）。配置与导出的日志写在可执行文件同目录，
不写注册表、不写用户目录。

## 本版改动

### 日志与大负载

- 大负载预览按格式截断（文本 / JSON 1024 字符，Hex 512，Base64 768），
  显示 `展开` 可查看完整内容，详情单条上限 16 KB
- 渲染结果按日志记录缓存，滚动和切换格式不再重复解析
- 工具栏可暂停接收：暂停时只丢弃收发消息，系统与错误事件照常记录，
  恢复时提示期间丢弃条数
- 设置中新增历史上限 200 / 500 / 1000 / 2000，默认 200；
  每条连接另有 16 MB 字节预算

### 导出

- `存` 保存全部记录，`存筛选` 只保存当前搜索 / 类型 / 主题筛选结果
- 文件名带连接名与毫秒时间戳：`mqttc-<连接名>-YYYYMMDD-HHMMSS-mmm.log`，
  连接名中的非法字符会自动替换，重名自动加序号

### 界面

- 连接编辑页补上整页卡片，与会话 / 设置 / 关于风格一致
- 设置页与关于页铺满右侧区域
- 复选框、输入框、小按钮字号统一
- 语言切换后所有下拉框（皮肤、日志筛选、显示格式等）立即跟随
- 订阅 / 自动回复 / 预设页面边距与日志页对齐

## 功能

- 多连接管理，每条连接独立会话
- MQTT 3.1.1 / 5.0，QoS 0/1/2、Clean Session、Keep Alive、Last Will
- TLS：系统根证书、自定义 CA、客户端证书 / 私钥、跳过校验
- 订阅通配符、按 QoS 订阅、点击订阅项筛选日志
- 文本 / JSON / Base64 / Hex 发布与互转，JSON 美化
- 定时发布、发布预设、按主题/关键字/负载自动回复
- 聊天式日志：搜索、筛选、格式切换、自动滚动、暂停、导出；默认 200 条，可调至 2000
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
- 校验：每个资产附 `.sha256`，Release 中另有 `SHA256SUMS.txt`。

源码、问题反馈：https://github.com/GordonWen/mqttc · MIT License
