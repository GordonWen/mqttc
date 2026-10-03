# Requirements Document

## Introduction

会话页当前只有一套发布 Topic / Payload。调试时常要在几组固定报文之间来回切换。本需求为每个连接提供 3 个可编辑的发布预设；用户点对应按钮即用该预设发布，减少改表单的时间。

## Glossary

- **System**: mqttc 桌面应用
- **Connection**: 一条已保存的 Broker 连接
- **Publish Preset**: 一组可命名、可编辑的发布参数，包含名称、Topic、Payload、QoS、Retain、Payload 格式
- **Publish Form**: 会话页底部现有的 Topic / Payload / QoS / Retain / 格式输入区
- **Timed Publish**: 按间隔循环发送当前发布表单内容的功能

## Requirements

### Requirement 1: 每个连接保存 3 个发布预设

**User Story:** AS 调试人员, I want 为当前连接保存 3 组发布参数, so that 我能在几条常用报文之间快速切换

#### Acceptance Criteria

1. WHEN System 创建或加载一条 Connection, the System SHALL 为该 Connection 提供恰好 3 个 Publish Preset
2. WHEN 一条 Publish Preset 尚未由用户填写过, the System SHALL 使用空名称、空 Topic、空 Payload、QoS 0、Retain 关闭、格式为文本
3. WHEN 用户编辑某条 Publish Preset 的名称、Topic、Payload、QoS、Retain 或格式, the System SHALL 把该组参数写入当前 Connection 的配置
4. WHEN System 把当前 Connection 写入 `mqttc.json`, the System SHALL 一并写入 3 个 Publish Preset
5. WHEN System 启动并读到不含发布预设的旧配置, the System SHALL 为该 Connection 补齐 3 个空预设并继续加载其余字段
6. WHEN 用户切换到另一条 Connection, the System SHALL 在会话页展示该 Connection 自己的 3 个 Publish Preset

### Requirement 2: 点击预设即发布

**User Story:** AS 调试人员, I want 点哪个预设就发哪组报文, so that 调试时不用先改底部发布栏再点发布

#### Acceptance Criteria

1. WHEN 用户在已连接状态下点击某个 Publish Preset 的发送按钮且该预设 Topic 非空, the System SHALL 使用该预设的 Topic、Payload、QoS、Retain 与格式向 Broker 发布
2. WHEN 该次发布请求已发出, the System SHALL 在当前 Connection 的日志中追加一条 SEND 记录，内容为该预设的 Topic 与编码后的 Payload
3. IF 用户在未连接时点击某个 Publish Preset 的发送按钮, the System SHALL 拒绝发布，并在日志中提示需要先连接 Broker
4. IF 被点击的 Publish Preset 的 Topic 为空, the System SHALL 拒绝发布，并在日志中提示 Topic 为空
5. IF 被点击的 Publish Preset 的格式为 JSON / Base64 / Hex 且 Payload 无法按该格式编码, the System SHALL 拒绝发布，并在日志中写入编码错误原因
6. WHEN 用户通过 Publish Preset 发送成功, the System SHALL 保持底部 Publish Form 的现有内容不变

### Requirement 3: 在会话页编辑与识别预设

**User Story:** AS 调试人员, I want 在会话页直接看到并改这 3 个预设, so that 我能按现场协议随时改报文

#### Acceptance Criteria

1. WHILE 会话页可见, the System SHALL 在发布区域展示 3 个 Publish Preset，每个含名称、Topic、Payload、QoS、Retain、格式和一个发送按钮
2. WHEN 某条 Publish Preset 的名称为空, the System SHALL 将该条的可见标签显示为「预设 1 / 2 / 3」或对应英文「Preset 1 / 2 / 3」
3. WHEN 用户改完某条 Publish Preset 的字段, the System SHALL 在下一次成功发布、断开、切连接或关闭窗口时把 3 个预设写入 `mqttc.json`
4. WHILE 当前 Connection 未连接, the System SHALL 仍允许编辑 3 个 Publish Preset，并禁用这 3 个发送按钮
5. WHEN 界面语言为中文或英文, the System SHALL 用当前语言显示预设区标题、占位符与发送按钮文案

### Requirement 4: 与现有发布和定时发布并存

**User Story:** AS 调试人员, I want 底部原来的发布栏和定时发布继续可用, so that 我既能随手发一条，也能按间隔重发当前表单

#### Acceptance Criteria

1. WHEN 用户点击底部 Publish Form 的「发布」按钮, the System SHALL 仍按该表单的 Topic、Payload、QoS、Retain 与格式发布
2. WHEN 用户启动 Timed Publish, the System SHALL 仍按底部 Publish Form 的当前内容循环发送
3. WHEN 用户通过 Publish Preset 发送, the System SHALL 保持正在运行的 Timed Publish 继续使用底部 Publish Form，不改用该预设
4. WHEN 用户清空或保存日志, the System SHALL 保持 3 个 Publish Preset 的内容不变
