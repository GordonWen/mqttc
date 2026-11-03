# Requirements Document

Status: ready

## Introduction

mqttc 已具备多连接、订阅、聊天日志、发布、预设与自动回复。当前会话页把订阅做成顶栏花签、工具栏横排，窄窗口要靠横滑才能看到右侧按钮；左侧栏把自动回复、发布预设、设置、关于做成四个整宽按钮，和 MQTTX 的「左栏连接 + 会话中订阅列表 + 中间聊天气泡 + 底部发布」骨架不一致。皮肤目前是跟随系统、黑、白、蓝、绿、紫五套色板，与 MQTTX Desktop 的 Light / Dark / Night 不一致。

本需求重构全窗口壳层与会话页信息架构，使主界面视觉与操作路径对齐 [MQTTX Desktop](https://github.com/emqx/MQTTX) 的聊天式布局。覆盖左栏、会话页、连接配置、设置、自动回复、发布预设与关于页。不引入 WebSocket、MQTTX Agent、主题树、脚本、CBOR/MsgPack/Protobuf 等 MQTTX 专有能力。既有 MQTT 3.1.1 / 5.0、TLS、定时发布、发布预设、自动回复、字号、密度与中英界面继续可用。

## Glossary

- **System**: mqttc 桌面应用
- **MQTTX Shell**: 对齐 MQTTX Desktop 的窗口分区：左栏连接、会话顶栏、订阅列表、消息区、发布区
- **Nav Rail**: 窗口最左侧的窄栏，放置连接列表与次级入口
- **Session Workspace**: 选中连接后的主工作区（当前 `page` 为 1）
- **Subscription List**: 会话工作区内、Message Pane 左侧的已订阅 Topic 纵向列表，可隐藏
- **Message Pane**: 聊天气泡日志区
- **Publish Dock**: 消息区下方的发布表单与预设发送行
- **Secondary Page**: 连接配置、自动回复、发布预设、设置、关于等非会话页
- **Theme Variant**: 界面配色。选项为跟随系统、Light、Dark、Night。缺省为跟随系统。跟随系统时按操作系统深浅色在 Light 与 Dark 之间切换；Night 只在用户显式选择时生效

## Requirements

### Requirement 1: MQTTX 式窗口分区

**User Story:** AS 调试人员, I want 打开窗口就能看到左栏连接、中间消息、底部发布, so that 日常收发路径与 MQTTX Desktop 一致

#### Acceptance Criteria

1. WHILE 窗口可见, the System SHALL 在最左侧展示 Nav Rail，在右侧展示当前 Secondary Page 或 Session Workspace
2. WHILE Session Workspace 可见, the System SHALL 自上而下排列连接顶栏、消息工具栏与主体、Publish Dock
3. WHILE Session Workspace 可见且 Subscription List 未隐藏, the System SHALL 在 Message Pane 左侧展示 Subscription List
4. WHEN 用户将窗口宽度收到当前 Theme 的最小窗口宽度, the System SHALL 保持 Nav Rail、Message Pane 与 Publish Dock 同时可见
5. WHEN Theme 的字号或密度变化, the System SHALL 按同一组 Theme Token 更新 Nav Rail、Subscription List、顶栏、Secondary Page 与 Publish Dock 的间距与控件高度
6. WHILE 连接配置、设置、自动回复、发布预设或关于页可见, the System SHALL 使用与 Session Workspace 相同的 Nav Rail、面板圆角、内边距与 Theme Token

### Requirement 2: 左栏连接与次级入口

**User Story:** AS 调试人员, I want 左栏只承担连接切换和次级入口, so that 主工作区留给当前连接的消息

#### Acceptance Criteria

1. WHILE Nav Rail 可见, the System SHALL 在顶部展示连接列表与新建入口
2. WHEN 用户点击新建入口, the System SHALL 打开连接配置页
3. WHEN 用户点击一条已有连接, the System SHALL 把该连接设为当前连接并打开 Session Workspace
4. WHILE 一条连接已连接或正在连接, the System SHALL 在该连接条目上显示状态色点
5. WHILE Nav Rail 可见, the System SHALL 在连接列表下方提供自动回复、发布预设、设置、关于四个入口
6. WHEN 用户点击上述四个入口之一, the System SHALL 打开对应 Secondary Page，并提供返回 Session Workspace 的控件

### Requirement 3: 会话顶栏

**User Story:** AS 调试人员, I want 顶栏一眼看到当前连接名称、地址和连通状态, so that 不用离开消息区就能连接或断开

#### Acceptance Criteria

1. WHILE Session Workspace 可见, the System SHALL 在顶栏展示当前连接名称、主机与端口摘要、连通状态文案
2. WHILE Session Workspace 可见, the System SHALL 在顶栏提供编辑、连接、断开三个操作
3. WHEN 当前连接未连接且未在连接中, the System SHALL 允许用户从顶栏发起连接
4. WHEN 当前连接已连接或正在连接, the System SHALL 允许用户从顶栏断开
5. WHEN 窗口宽度小于顶栏首选宽度, the System SHALL 在顶栏内横向滚动，并保持编辑、连接、断开三个操作完整可见

### Requirement 4: 订阅列表

**User Story:** AS 调试人员, I want 已订阅 Topic 固定列在消息区左侧, so that 筛选和取消订阅的路径与 MQTTX 一致

#### Acceptance Criteria

1. WHILE Session Workspace 可见且 Subscription List 未隐藏, the System SHALL 在 Message Pane 左侧以纵向列表展示当前连接的每条订阅，每行包含 Topic 与 QoS
2. WHEN 用户在已连接状态下打开新建订阅入口, the System SHALL 展示 Topic 输入、QoS 选择与确认控件
3. WHEN 订阅请求成功（含该 Topic 已在列表中）, the System SHALL 收起新建订阅表单并刷新 Subscription List
4. WHEN 用户点击一条订阅, the System SHALL 按该 Topic 筛选 Message Pane；若该条已是当前筛选, the System SHALL 取消筛选
5. WHILE 某条订阅为当前筛选, the System SHALL 高亮该行
6. WHEN 用户点击一条订阅的取消控件, the System SHALL 取消该条订阅
7. WHEN 用户隐藏 Subscription List, the System SHALL 把 Message Pane 扩展到原列表占用的宽度
8. WHEN 用户再次显示 Subscription List, the System SHALL 把列表恢复到 Message Pane 左侧
9. WHEN 当前连接没有任何订阅, the System SHALL 仍展示 Subscription List 容器与新建订阅入口
10. WHEN 用户切换当前连接, the System SHALL 按该连接的订阅刷新 Subscription List，并保持列表的显示或隐藏状态

### Requirement 5: 聊天气泡消息区

**User Story:** AS 调试人员, I want 收到的消息靠左、发出的消息靠右, so that 对照收发时和 MQTTX 一样扫一眼就能分清方向

#### Acceptance Criteria

1. WHEN Message Pane 展示种类为 RECV 的条目, the System SHALL 将该气泡靠左对齐
2. WHEN Message Pane 展示种类为 SEND 的条目, the System SHALL 将该气泡靠右对齐
3. WHEN Message Pane 展示种类为 SYS 或 ERR 的条目, the System SHALL 将该条居中或通栏展示，并与收发气泡区分配色
4. WHILE Message Pane 可见, the System SHALL 提供搜索、种类筛选、接收格式、自动滚动、保存与清空
5. WHEN 窗口宽度小于消息工具栏首选宽度, the System SHALL 在工具栏内横向滚动，并保持保存与清空完整可见

### Requirement 6: 底部发布区

**User Story:** AS 调试人员, I want 发布表单固定在消息区下方, so that 订阅和看日志时仍能直接改 Topic 并发送

#### Acceptance Criteria

1. WHILE Session Workspace 可见, the System SHALL 在 Message Pane 下方展示 Publish Dock
2. WHILE Publish Dock 可见, the System SHALL 提供 Topic、QoS、Retain、格式、定时间隔、发布、美化与定时开关
3. WHILE 当前连接存在至少一条发布预设, the System SHALL 在 Publish Dock 内以单行展示预设发送按钮；按钮超出一行时横向滚动
4. WHEN 用户点击发布, the System SHALL 使用 Publish Dock 当前字段发送
5. WHEN 用户点击一条预设发送按钮, the System SHALL 使用该预设字段发送，并保持 Publish Dock 当前字段不变

### Requirement 7: Light / Dark / Night 主题

**User Story:** AS 调试人员, I want 在设置页选 Light、Dark 或 Night, so that 配色与 MQTTX Desktop 三套主题对齐

#### Acceptance Criteria

1. WHEN 用户打开设置页, the System SHALL 将皮肤选项展示为跟随系统、Light、Dark、Night
2. WHEN 用户选择 Light, the System SHALL 立即把全窗口配色切换为浅色主题
3. WHEN 用户选择 Dark, the System SHALL 立即把全窗口配色切换为深色主题
4. WHEN 用户选择 Night, the System SHALL 立即把全窗口配色切换为夜间主题
5. WHEN 用户选择跟随系统且操作系统为深色外观, the System SHALL 使用 Dark
6. WHEN 用户选择跟随系统且操作系统为浅色外观, the System SHALL 使用 Light
7. WHEN 用户更改皮肤选项, the System SHALL 把该选择写入 `mqttc.json`，键值为 `system`、`light`、`dark` 或 `night`
8. WHEN System 启动且配置文件不存在, the System SHALL 使用跟随系统
9. WHEN System 加载 `mqttc.json` 且皮肤键为 `white`, the System SHALL 按 Light 处理
10. WHEN System 加载 `mqttc.json` 且皮肤键为 `black`, the System SHALL 按 Dark 处理
11. WHEN System 加载 `mqttc.json` 且皮肤键为 `blue`、`green` 或 `purple`, the System SHALL 按 Night 处理
12. WHILE 界面语言为中文, the System SHALL 将上述选项显示为「跟随系统」「浅色」「深色」「夜间」
13. WHILE 界面语言为英文, the System SHALL 将上述选项显示为 System、Light、Dark、Night

### Requirement 8: 既有能力保持可用

**User Story:** AS 调试人员, I want 换壳之后原来的调试能力都还在, so that 我不用为了对齐 MQTTX 丢掉现有功能

#### Acceptance Criteria

1. WHILE System 运行, the System SHALL 继续支持多连接并行在线、MQTT 3.1.1 与 5.0、TCP 与 TLS、Last Will、定时发布
2. WHILE System 运行, the System SHALL 继续支持每连接独立的发布预设与自动回复
3. WHILE 设置页可见, the System SHALL 继续提供字号与界面密度
4. WHEN 用户完成一次连接、订阅、发布或关闭窗口, the System SHALL 把配置写入可执行文件同目录的 `mqttc.json`
5. WHEN 界面展示任何面向用户的文案, the System SHALL 通过 I18n 同时提供中文与英文
6. WHEN 用户隐藏或显示 Subscription List, the System SHALL 把该状态写入 `mqttc.json`，并在下次启动时恢复
