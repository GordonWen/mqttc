# Requirements Document

## Introduction

调试时常要在几组固定报文之间来回切换。本需求为每条连接提供可增减的发布预设，放在独立页面编辑。会话页底部发布表单下方只保留发送按钮，点哪条就发哪组报文。底部发布表单与定时发布继续使用原字段。

## Glossary

- **System**: mqttc 桌面应用
- **Connection**: 一条已保存的 Broker 连接
- **Publish Preset Page**: 独立于配置页、会话页、自动回复页、设置页的预设编辑页
- **Publish Preset**: 一组可命名、可编辑的发布参数，包含名称、Topic、Payload、QoS、Retain、Payload 格式
- **Publish Form**: 会话页底部现有的 Topic / Payload / QoS / Retain / 格式输入区
- **Timed Publish**: 按间隔循环发送当前发布表单内容的功能
- **Preset Send Button**: 会话页发布区下方按预设列表展示的发送入口

## Requirements

### Requirement 1: 每条连接保存一份可增减的预设列表

**User Story:** AS 调试人员, I want 为当前连接维护多条发布预设, so that 常用报文可以按现场协议增减

#### Acceptance Criteria

1. WHEN System 创建或加载一条 Connection, the System SHALL 为该 Connection 提供一份 Publish Preset 列表，列表允许为空
2. WHEN 用户新增一条 Publish Preset, the System SHALL 追加一条空名称、空 Topic、空 Payload、QoS 0、Retain 关闭、格式为文本的预设
3. WHEN 用户删除一条 Publish Preset, the System SHALL 从当前 Connection 的列表中移除该预设，并保持其余预设的顺序与内容
4. WHEN 用户编辑某条 Publish Preset 的名称、Topic、Payload、QoS、Retain 或格式, the System SHALL 把该条完整写入当前 Connection 的配置
5. WHEN System 把当前 Connection 写入 `mqttc.json`, the System SHALL 一并写入该 Connection 的全部 Publish Preset
6. WHEN System 启动并读到不含发布预设字段的旧配置, the System SHALL 将该 Connection 的预设列表视为空列表，并继续加载其余字段
7. WHEN 用户切换到另一条 Connection, the System SHALL 在 Publish Preset Page 与会话页发送区展示该 Connection 自己的预设列表

### Requirement 2: 独立发布预设页

**User Story:** AS 调试人员, I want 在单独一页里增删和编辑预设, so that 会话页只负责发送，编辑有地方放

#### Acceptance Criteria

1. WHEN 用户从左侧栏打开发布预设, the System SHALL 显示 Publish Preset Page，页面列出当前 Connection 的全部 Publish Preset
2. WHILE Publish Preset Page 可见, the System SHALL 为每条预设展示名称、Topic、Payload、QoS、Retain、格式，以及删除该预设的入口
3. WHEN 用户在 Publish Preset Page 点击新增, the System SHALL 在列表末尾追加一条默认 Publish Preset 并展示该条
4. WHEN 用户新增或删除 Publish Preset，或改完预设字段后离开 Publish Preset Page、切连接、连接、断开、成功发布或关闭窗口, the System SHALL 把当前 Connection 的预设列表写入 `mqttc.json`
5. WHEN 界面语言为中文或英文, the System SHALL 用当前语言显示 Publish Preset Page 的标题、字段标签、按钮与空列表提示
6. WHILE 当前 Connection 未连接, the System SHALL 仍允许在 Publish Preset Page 增删和编辑预设

### Requirement 3: 会话页用发送按钮触发预设

**User Story:** AS 调试人员, I want 在发布表单下面点哪个预设就发哪组报文, so that 调试时不用先改底部发布栏再点发布

#### Acceptance Criteria

1. WHILE 会话页可见, the System SHALL 在 Publish Form 下方按当前 Connection 的预设列表展示 Preset Send Button
2. WHEN 某条 Publish Preset 的名称为空, the System SHALL 将该条发送按钮的可见标签显示为「预设」加序号，或对应英文「Preset」加序号
3. WHEN 用户在已连接状态下点击某个 Preset Send Button 且该预设 Topic 非空, the System SHALL 使用该预设的 Topic、Payload、QoS、Retain 与格式向 Broker 发布
4. WHEN 该次发布请求已发出, the System SHALL 在当前 Connection 的日志中追加一条 SEND 记录，内容为该预设的 Topic 与编码后的 Payload
5. IF 用户在未连接时点击某个 Preset Send Button, the System SHALL 拒绝发布，并在日志中提示需要先连接 Broker
6. IF 被点击的 Publish Preset 的 Topic 为空, the System SHALL 拒绝发布，并在日志中提示 Topic 为空
7. IF 被点击的 Publish Preset 的格式为 JSON / Base64 / Hex 且 Payload 无法按该格式编码, the System SHALL 拒绝发布，并在日志中写入编码错误原因
8. WHEN 用户通过 Publish Preset 发送成功, the System SHALL 保持底部 Publish Form 的现有内容不变
9. WHILE 当前 Connection 未连接, the System SHALL 禁用这些 Preset Send Button

### Requirement 4: 与现有发布和定时发布并存

**User Story:** AS 调试人员, I want 底部原来的发布栏和定时发布继续可用, so that 我既能随手发一条，也能按间隔重发当前表单

#### Acceptance Criteria

1. WHEN 用户点击底部 Publish Form 的「发布」按钮, the System SHALL 仍按该表单的 Topic、Payload、QoS、Retain 与格式发布
2. WHEN 用户启动 Timed Publish, the System SHALL 仍按底部 Publish Form 的当前内容循环发送
3. WHEN 用户通过 Publish Preset 发送, the System SHALL 保持正在运行的 Timed Publish 继续使用底部 Publish Form，不改用该预设
4. WHEN 用户清空或保存日志, the System SHALL 保持全部 Publish Preset 的内容不变
