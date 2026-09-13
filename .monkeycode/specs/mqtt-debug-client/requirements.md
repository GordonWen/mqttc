# Requirements Document

## Introduction

mqttc Windows 最小可用版本：用户通过桌面窗口连接 MQTT 3.1.1 Broker，完成订阅、发布，并在日志中查看消息。本文件描述 MVP 范围；TLS、用户名密码、MQTT 5.0、多会话为后续需求。

## Glossary

- **System**: mqttc 桌面应用
- **Broker**: MQTT 服务端
- **Client ID**: MQTT 连接标识
- **Topic**: MQTT 主题过滤器或发布主题
- **Payload**: 消息正文，MVP 按文本处理
- **Log**: 窗口内按时间排列的事件与消息列表

## Requirements

### Requirement 1: 连接 Broker

**User Story:** AS 调试人员, I want 填写主机、端口和 Client ID 后连接 Broker, so that 我能快速验证网络与协议是否通

#### Acceptance Criteria

1. WHEN 用户点击连接且主机非空、端口为 1 到 65535 的整数, the System SHALL 使用 MQTT 3.1.1、Clean Session、TCP 向该地址发起连接
2. WHEN Client ID 为空, the System SHALL 生成以 `mqttc-` 为前缀的 Client ID 再发起连接
3. WHEN Broker 返回 ConnAck, the System SHALL 将状态显示为已连接并允许订阅与发布
4. IF 端口非法或连接失败, the System SHALL 保持未连接，并在日志中写入错误原因
5. WHEN 用户在已连接状态下点击断开, the System SHALL 断开会话并将状态显示为未连接

### Requirement 2: 订阅 Topic

**User Story:** AS 调试人员, I want 订阅一个 Topic, so that 我能看到 Broker 推过来的消息

#### Acceptance Criteria

1. WHEN 用户在已连接状态下提交非空 Topic, the System SHALL 以 QoS 0 发送 SUBSCRIBE
2. WHEN 订阅请求已发出, the System SHALL 在日志中记录该 Topic
3. IF 用户在未连接时点击订阅, the System SHALL 在日志中提示需要先连接
4. IF Topic 为空, the System SHALL 拒绝发送订阅并在日志中提示 Topic 为空

### Requirement 3: 发布消息

**User Story:** AS 调试人员, I want 向指定 Topic 发布文本, so that 我能验证下游是否收到

#### Acceptance Criteria

1. WHEN 用户在已连接状态下提交非空 Topic 与 Payload, the System SHALL 以 QoS 0、retain=false 发布该文本
2. WHEN 发布请求已发出, the System SHALL 在日志中记录 Topic 与 Payload，种类为 SEND
3. IF 用户在未连接时点击发布, the System SHALL 在日志中提示需要先连接
4. IF Topic 为空, the System SHALL 拒绝发布并在日志中提示 Topic 为空

### Requirement 4: 消息日志

**User Story:** AS 调试人员, I want 按时间看到收发与系统事件, so that 我能对照一次调试过程

#### Acceptance Criteria

1. WHEN System 收到 PUBLISH, the System SHALL 追加一条 RECV 日志，包含时间、Topic 与 Payload（UTF-8 有损解码）
2. WHEN 发生连接、断开或错误, the System SHALL 追加 SYS 或 ERR 日志
3. WHEN 日志条数超过 500, the System SHALL 丢弃最早的条目并保留最近 500 条
4. WHEN 用户点击清空, the System SHALL 删除当前全部日志条目
