# 协议行为说明

简体中文 | [English](PROTOCOL.en.md)

本文说明 mqttc 在 MQTT 3.1.1 / 5.0 上**自己实现**的协议相关行为：连接页字段差异、
CONNACK 能力预检、发送确认（PUBACK / PUBREC / PUBCOMP）状态、手动 ACK 与 TLS 密钥日志，
以及用于验证这些行为的故障注入测试。

MQTT 编解码、连接与重连、QoS 2 的 PUBREL / PUBCOMP 时序由 rumqttc 0.25.1 负责；
mqttc 只做调试器需要的展示、校验与记录，不实现 Broker 侧的存储、路由或会话恢复。

## 1. 连接页字段与协议版本

连接页只显示当前协议版本有意义的字段：

| 字段 | 3.1.1 | 5.0 | 说明 |
|---|---|---|---|
| Clean Session / Clean Start | ✓（Clean Session） | ✓（Clean Start） | 语义相同，标签随版本变化 |
| 会话过期（Session Expiry） | — | ✓ | CONNECT 属性，留空 / 0 表示不携带 |
| 接收上限（Receive Maximum） | — | ✓ | CONNECT 属性，0 表示不携带 |
| 最大报文（Maximum Packet Size） | — | ✓ | CONNECT 属性，0 表示不携带 |
| Topic Alias 上限 | — | ✓ | CONNECT 属性，0 表示不携带 |
| 手动确认（Manual ACK） | ✓ | ✓ | 客户端行为，两种协议都适用 |
| TLS 密钥日志 | ✓ | ✓ | 传输层开关，与协议版本无关 |

四个 CONNECT 属性只在 **5.0 + 值非 0** 时写入报文；切换回 3.1.1 后这些值仍保存在
`mqttc.json` 中，但不会发送。保存时由 `form` 统一钳制：Receive Maximum 1–65535、
Maximum Packet Size 1–268435455，0 统一视为“不携带”。

新连接默认值见 `ConnectionConfig::new`；旧配置文件缺少这些字段时按 `serde(default)` 读成 0 / false。

## 2. CONNACK 能力摘要与预检

连接成功后，运行时把 CONNACK 结果整理成一条 `SYS` 日志：

```text
CONNACK: 新会话 · 最大 QoS 2 · 最大报文 1048576 B · 接收上限 20
```

`reason string`、User Property 等额外信息会追加为后续 `SYS` 行。

`BrokerCaps`（`core/src/mqtt.rs`）来自 5.0 CONNACK 属性；3.1.1 没有这些标志，使用宽松默认值
（最大 QoS 2、支持 Retain / 通配符 / 共享订阅），因此 3.1.1 不会有额外拦截。可用字段：

`session_present`、`max_qos`、`retain_available`、`max_packet_size`、`receive_maximum`、
`topic_alias_max`、`server_keep_alive`、`wildcard_available`、`shared_available`、
`session_expiry`、`assigned_client_id`、`extra`。

发送和订阅之前按能力做本地预检，不满足时写入 `ERR` 行并取消本次操作，不把注定被拒的报文发出去：

- **发布**（`publish_check`）：`retain` 但 Broker 不支持 Retain；QoS 大于 `max_qos`；
  估算报文大小超过 `max_packet_size`。估算公式为
  `2 + topic 字节数 + payload 字节数 + 属性字节数 + (QoS>0 ? 2 : 0) + 5`，
  最后 5 字节按固定头最坏情况计。
- **订阅**（`subscribe_check`）：主题含 `+` / `#` 但不支持通配符；`$share/` 主题但不支持
  共享订阅；5.0 共享订阅同时启用 No Local（规范要求为 0）。
  `flip_sub_option` 在已连接时会重新订阅，因此同样走这套预检。

预检只依赖 Broker **主动声明**的上限；Broker 超发或违约时仍以 rumqttc 的错误为准。

## 3. 发送确认状态

每条 QoS 1/2 的 `SEND` 行带一个 `AckInfo`，按 broker 回包原地更新，不新增日志行：

| 状态 | 含义 |
|---|---|
| 排队中 `Queued` | 已交给 rumqttc，尚未写出（QoS 0 不进入此状态） |
| 已发出 `Sent` | QoS 0 已写出；QoS 1/2 通常很快进入 `AwaitingAck` |
| 等待确认 `AwaitingAck` | QoS 1 等 PUBACK，QoS 2 等 PUBREC |
| 等待 PUBCOMP `AwaitingComp` | QoS 2 已收到 PUBREC，rumqttc 自动发出 PUBREL |
| 已确认 `Acked` | PUBACK / PUBCOMP 成功；记录从发出到确认的毫秒数 |
| 确认失败 `Rejected` | Broker 返回失败原因（5.0 reason code 或连接错误） |
| 结果未知 `Timeout` | 15 秒内没有确认，或链路在确认前断开；mqttc 不自动重发 |

实现要点：

- SEND 行与 packet id 的对应来自 rumqttc 的 `Outgoing::Publish(pkid)`（报文真正写出时触发），
  用先进先出队列匹配，避免把并发发布的确认挂错行。
- 5.0 的 PUBACK / PUBREC 只有 `Success` 与 `NoMatchingSubscribers` 计为成功；
  其余 reason code 显示为 `Rejected` 并附原因。3.1.1 没有失败 reason code。
- QoS 2 的 PUBREL / PUBCOMP 由 rumqttc 自动完成，mqttc 只观察时序。
- 超时由 `poll()` 内的 `ack_tick` 统一检查，每轮最多遍历尚未确认的 packet id；
  断开或重连前调用 `mark_acks_lost`，把所有未确认行标为 `Timeout`。
- 超时只改显示，不触发重发；是否为 QoS 1/2 重传由 rumqttc 的会话状态决定。

## 4. 手动 ACK

开启“手动确认”（连接页，或 `mqttc.json` 的 `manual_acks`）后，rumqttc 的 `manual_acks`
只拦截**入站** QoS 1/2 PUBLISH 的确认：

1. 入站消息照常写入 `RECV` 行，状态为 `PendingAck`，并在日志气泡和详情页显示“确认”按钮。
2. 点击确认调用 `AppRuntime::manual_ack(seq)`：QoS 1 发送 PUBACK，QoS 2 发送 PUBREC。
3. 状态变为 `AckSent`；QoS 2 之后由 rumqttc 自动发送 PUBREL / PUBCOMP，
   收到 `Outgoing::PubComp` 后该行变为 `Acked`。
4. 出站 SEND 行的确认逻辑不受影响。

注意：手动确认只用于调试“不确认时 Broker 会怎样”。挂起期间 Broker 可能按自身策略重发
（DUP=1），mqttc 会按普通入站消息记录。

## 5. TLS 密钥日志

开启“TLS 密钥日志”后，rustls 的 `KeyLog` 把每个连接的会话密钥追加写入配置目录下的
`logs/tls-keys.log`，格式与 `SSLKEYLOGFILE` 相同（`CLIENT_RANDOM` 等标准标签），
可直接在 Wireshark 中解密 TLS 流量。

该文件等同于明文会话密钥：只在本机调试时开启，不要附在 issue、日志或交付物中。
设置只影响新建连接；关闭后不再写入，但不会删除已有文件。

## 6. 故障注入测试

`tools/test-broker/src/fault.rs` 是一个刻意不守规矩的最小 MQTT 3.1.1 服务端，
与 rumqttd 并存，只用于 `core/tests/fault_injection.rs`：

| `Fault` | 行为 | 覆盖点 |
|---|---|---|
| `Happy` | 立即 PUBACK；QoS 2 走 PUBREC → PUBREL → PUBCOMP | QoS 2 完整时序 |
| `DelayAck(d)` | 延迟 `d` 后回 PUBACK | `AwaitingAck` → `Acked`，确认耗时 |
| `DropAck` | 永不回 PUBACK，保持 TCP | 挂起状态；断开后标为 `Timeout` |
| `Duplicate` | 每条 PUBLISH 回两次（第二份 DUP=1） | 重复投递 + 手动确认两行 |
| `OutOfOrder` | 攒够两条 PUBACK 后倒序发送 | packet id 对应关系不串行 |

运行方式：

```text
cargo test -p mqttc-core --test fault_injection
```

15 秒真实超时不放在集成测试里等待，由 `core/src/runtime.rs` 的单元测试直接拨快
`ack_at` 验证；集成测试覆盖链路断开（`disconnect` → `Timeout`）的真实路径。
