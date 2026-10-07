# Requirements Document

## Introduction

调试时常要在收到指定 Topic / 内容后立刻回一条固定报文。本需求为每条连接提供可增减的自动回复规则，放在独立页面编辑。每条规则自带匹配条件与回复内容：规则 N 只按规则 N 的条件匹配，并只发送规则 N 配置的固定报文。同一条入站消息按列表顺序只触发第一条命中的已启用规则。

## Glossary

- **System**: mqttc 桌面应用
- **Connection**: 一条已保存的 Broker 连接
- **Auto-Reply Page**: 独立于配置页、会话页、设置页的规则编辑页
- **Auto-Reply Rule**: 一条自包含规则，含启用开关、匹配 Topic 过滤器、Payload 包含文本、以及固定回复的 Topic / Payload / QoS / Retain / 格式
- **Inbound Publish**: Broker 推送给当前 Connection 的 MQTT PUBLISH
- **Fixed Reply**: 规则里预先填好的发布参数，匹配成功后原样发出
- **Own Echo**: 当前 Connection 刚发出的 Fixed Reply 被 Broker 回投到本连接订阅的 Inbound Publish
- **Echo Policy**: 连接级选项，决定 Own Echo 是否再次进入规则匹配；取值「跳过」或「再匹配」

## Requirements

### Requirement 1: 每条连接保存一组独立规则

**User Story:** AS 调试人员, I want 为当前连接维护多条互不混用的自动回复规则, so that 不同报文走各自的匹配和回复

#### Acceptance Criteria

1. WHEN System 创建或加载一条 Connection, the System SHALL 为该 Connection 提供一份 Auto-Reply Rule 列表，列表允许为空
2. WHEN 用户新增一条 Auto-Reply Rule, the System SHALL 追加一条默认关闭、匹配 Topic 为空、Payload 包含为空、回复 Topic 为空、回复 Payload 为空、QoS 0、Retain 关闭、格式为文本的规则
3. WHEN 用户删除一条 Auto-Reply Rule, the System SHALL 从当前 Connection 的列表中移除该规则，并保持其余规则的顺序与内容
4. WHEN 用户编辑某条 Auto-Reply Rule 的启用勾选、匹配 Topic、Payload 包含、回复 Topic、回复 Payload、QoS、Retain 或格式, the System SHALL 把该条完整写入当前 Connection 的配置
5. WHEN System 把当前 Connection 写入 `mqttc.json`, the System SHALL 一并写入该 Connection 的全部 Auto-Reply Rule 与 Echo Policy
6. WHEN System 启动并读到不含自动回复字段的旧配置, the System SHALL 将该 Connection 的规则列表视为空列表、Echo Policy 视为「跳过」，并继续加载其余字段
7. WHEN 用户切换到另一条 Connection, the System SHALL 在 Auto-Reply Page 展示该 Connection 自己的规则列表与 Echo Policy

### Requirement 2: Topic 加 Payload 包含匹配后发送固定回复

**User Story:** AS 调试人员, I want 收到匹配的消息后自动发出我写好的固定内容, so that 设备握手或状态查询能马上有回应

#### Acceptance Criteria

1. WHEN 当前 Connection 已连接且收到一条需要评估的 Inbound Publish, the System SHALL 按列表顺序检查该 Connection 中已启用的 Auto-Reply Rule，并在第一条匹配成功的规则处理完成后结束本轮评估
2. WHEN 一条已启用规则的匹配 Topic 非空，且 Inbound Publish 的 Topic 满足该 MQTT 过滤器（含 `+` / `#`），且 Payload 包含文本为空或入站 Payload 的 UTF-8 文本按区分大小写方式包含该字符串, the System SHALL 判定该规则匹配成功
3. WHEN 一条 Auto-Reply Rule 匹配成功且该规则的回复 Topic 非空, the System SHALL 使用该规则自己的回复 Topic、回复 Payload、QoS、Retain 与格式向 Broker 发布 Fixed Reply
4. WHEN 该次 Fixed Reply 已发出, the System SHALL 在当前 Connection 的日志中追加一条 SEND 记录，内容为该规则的回复 Topic 与编码后的 Payload
5. WHEN 一条 Auto-Reply Rule 处于关闭状态, the System SHALL 跳过该规则并继续检查列表中的下一条
6. IF 一条已启用规则的匹配 Topic 为空, the System SHALL 跳过该规则并继续检查列表中的下一条
7. IF 一条已启用规则匹配成功但回复 Topic 为空, the System SHALL 跳过该规则的发送，在日志中提示回复 Topic 为空，并结束本轮评估
8. IF 一条已启用规则匹配成功且格式为 JSON / Base64 / Hex 且回复 Payload 无法按该格式编码, the System SHALL 跳过该规则的发送，在日志中写入编码错误原因，并结束本轮评估
9. IF 当前 Connection 未连接, the System SHALL 保持全部 Auto-Reply Rule 可编辑，并保持不发送 Fixed Reply

### Requirement 3: 独立自动回复页

**User Story:** AS 调试人员, I want 在单独一页里增删和编辑规则, so that 会话页保持订阅、日志和发布，规则管理有地方放

#### Acceptance Criteria

1. WHEN 用户从左侧栏打开自动回复, the System SHALL 显示 Auto-Reply Page，页面列出当前 Connection 的全部 Auto-Reply Rule
2. WHILE Auto-Reply Page 可见, the System SHALL 为每条规则展示独立的启用复选框、匹配 Topic、Payload 包含、回复 Topic、回复 Payload、QoS、Retain、格式，以及删除该规则的入口
3. WHILE Auto-Reply Page 可见, the System SHALL 展示当前 Connection 的 Echo Policy 选项：「跳过自身回投」与「再匹配自身回投」
4. WHEN 用户在 Auto-Reply Page 点击新增, the System SHALL 在列表末尾追加一条默认 Auto-Reply Rule 并展示该条
5. WHEN 用户改完规则字段或 Echo Policy, the System SHALL 在下一次成功发布、断开、切连接、离开 Auto-Reply Page 或关闭窗口时把规则列表与 Echo Policy 写入 `mqttc.json`
6. WHEN 界面语言为中文或英文, the System SHALL 用当前语言显示 Auto-Reply Page 的标题、字段标签、按钮、Echo Policy 文案与空列表提示
7. WHILE 当前 Connection 未连接, the System SHALL 仍允许在 Auto-Reply Page 增删和编辑规则，并允许修改 Echo Policy

### Requirement 4: 与现有收发路径并存

**User Story:** AS 调试人员, I want 自动回复叠在现有收发之上, so that 手动发布、预设发送和定时发布继续按原方式工作

#### Acceptance Criteria

1. WHEN System 收到 Inbound Publish, the System SHALL 先按现有规则追加 RECV 日志，再按 Echo Policy 决定是否评估 Auto-Reply Rule
2. WHEN 用户通过底部发布表单、发布预设或定时发布发送, the System SHALL 保持这些路径使用各自原有字段，不改用 Auto-Reply Rule
3. WHEN System 因 Auto-Reply Rule 发出 Fixed Reply, the System SHALL 保持底部发布表单与 3 个发布预设的内容不变
4. WHEN Echo Policy 为「跳过」且 Inbound Publish 的 Topic 与 Payload 字节等于本 Connection 最近一次 Fixed Reply, the System SHALL 记录 RECV 日志并跳过本轮 Auto-Reply Rule 评估
5. WHEN Echo Policy 为「再匹配」, the System SHALL 对每一条 Inbound Publish（含 Own Echo）按 Requirement 2 评估 Auto-Reply Rule

### Requirement 5: 规则之间保持自包含

**User Story:** AS 调试人员, I want 规则 1 只按规则 1 匹配并只发规则 1 的内容, so that 多条规则不会串用对方的条件或报文

#### Acceptance Criteria

1. WHEN 规则 N 匹配成功, the System SHALL 仅使用规则 N 自己的回复 Topic、回复 Payload、QoS、Retain 与格式发送
2. WHEN 规则 N 的匹配失败, the System SHALL 保持规则 N 不发送，并继续检查列表中的下一条规则
3. WHEN 用户编辑规则 N, the System SHALL 保持其余规则的字段不被该次编辑改写

### Requirement 6: 每条规则独立勾选生效

**User Story:** AS 调试人员, I want 用每条规则自己的复选框决定这条是否参与匹配, so that 我能保留多条规则只让当前需要的几条生效

#### Acceptance Criteria

1. WHILE Auto-Reply Page 可见, the System SHALL 在每条 Auto-Reply Rule 上提供独立的启用复选框，勾选表示该条参与匹配，取消勾选表示该条本轮被跳过
2. WHEN 用户勾选或取消勾选规则 N 的启用复选框, the System SHALL 只改写规则 N 的启用状态，并保持其余规则的启用状态与字段不变
3. WHEN 一条 Auto-Reply Rule 的启用复选框未勾选, the System SHALL 在评估 Inbound Publish 时跳过该规则，并继续检查列表中的下一条
4. WHEN 用户新增一条 Auto-Reply Rule, the System SHALL 将该条的启用复选框置于未勾选
5. WHEN System 把当前 Connection 写入 `mqttc.json`, the System SHALL 为每条 Auto-Reply Rule 写入各自的启用状态
