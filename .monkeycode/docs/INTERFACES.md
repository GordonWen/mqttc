# 接口定义

## UI 组件接口

`AppWindow` 是唯一窗口组件。左侧为连接列表，右侧按 `page` 切换：0 连接配置，1 会话，2 设置，3 自动回复，4 发布预设，5 关于。

### 通用组件

| 组件 | 说明 |
|------|------|
| Select | 紧凑下拉，属性 `options`、`current-index`、`interactive`，回调 `activated(int)` |
| ConnectionEntry | 左侧连接列表条目，字段 `id`、`name`、`status`、`status-color` |
| Subscription | 订阅条目，字段 `topic`、`qos`、`selected` |
| LogEntry | 日志条目，字段 `time`、`timestamp`、`kind`、`topic`、`payload` |
| PublishPresetRow | 发布预设行，字段 `name`、`topic`、`payload`、`qos`、`retain`、`format` |
| AutoReplyRuleRow | 自动回复规则行，字段 `enabled`、`match-topic`、`payload-contains`、`reply-topic`、`reply-payload`、`qos`、`retain`、`format` |

### 属性

| 属性 | 类型 | 含义 |
|------|------|------|
| page | int | 0 连接配置，1 会话，2 设置，3 自动回复，4 发布预设，5 关于 |
| app-version | string | 关于页展示的版本号，启动时写入 `CARGO_PKG_VERSION` |
| active-connection-id | string | 当前选中连接的 id |
| connections | [ConnectionEntry] | 左侧连接列表 |
| connection-name | string | 当前连接名称 |
| connection-summary | string | 会话页标题旁的 host:port |
| host / port / client-id | string | Broker 地址与 Client ID |
| username / password | string | 可选认证信息，密码用密文输入 |
| mqtt-protocol | int | 0 = MQTT 3.1.1，1 = MQTT 5.0 |
| keep-alive / connect-timeout | string | Keep Alive 与连接超时（秒） |
| clean-session | bool | MQTT Clean Session；MQTT 5.0 对应 Clean Start |
| will-topic / will-payload | string | Last Will Topic 与 Payload，Topic 为空则不发送遗嘱 |
| will-qos / will-retain | int / bool | 遗嘱 QoS 与 Retain |
| ssl | bool | 启用 MQTT over TLS（mqtts） |
| ssl-verify | bool | 是否校验服务器证书 |
| ssl-ca / ssl-cert / ssl-key | string | CA、客户端证书与私钥路径，证书可选 |
| status / status-color | string / color | 当前连接状态文案与颜色 |
| connected / connecting | bool | 控制按钮可用性 |
| sub-topic / sub-qos | string / int | 待订阅 Topic 与 QoS（0/1/2） |
| subscriptions | [Subscription] | 当前连接的订阅列表，含 `selected` 筛选高亮 |
| active-sub-topic | string | 会话页当前点击筛选的订阅 Topic |
| pub-topic / pub-payload | string | 待发布 Topic 与内容（payload 支持多行） |
| pub-qos / pub-retain | int / bool | 发布 QoS 与保留消息开关 |
| pub-format | int | 发送格式索引：0 文本、1 JSON、2 Base64、3 Hex |
| timed-interval | string | 定时发布间隔（秒），合法范围 1 到 3600 |
| timed-running | bool | 当前连接是否正在定时发布 |
| publish-presets | [PublishPresetRow] | 当前连接的发布预设，允许空 |
| auto-reply-rules | [AutoReplyRuleRow] | 当前连接的自动回复规则，允许空 |
| auto-reply-echo | int | 0 跳过自身回投，1 再匹配自身回投 |
| logs | [LogEntry] | 当前连接的日志列表（已按筛选过滤） |
| log-format | int | 接收显示格式索引，同上 |
| log-filter | string | 按 Topic / Payload / 类型关键字筛选 |
| log-kind-filter | int | 0 全部、1 收、2 发、3 系统、4 错误 |
| auto-scroll | bool | 日志自动滚动开关 |
| skin | int | 解析后的视觉皮肤：0 黑、1 白、2 蓝、3 绿、4 紫 |
| skin-choice | int | 设置页选项：0 跟随系统、1 黑、2 白、3 蓝、4 绿、5 紫 |
| language | int | 0 中文，1 English |

### 回调

| 回调 | 触发时机 |
|------|----------|
| new-connection-requested | 点击连接列表的 `+`，进入配置页 |
| select-connection-requested | 点击连接列表条目，参数为 id，进入会话页 |
| delete-connection-requested | 点击连接条目右侧 `×`，参数为 id |
| edit-connection-requested | 会话页点击编辑，进入配置页 |
| back-to-session-requested | 配置页 / 设置页 / 自动回复页 / 发布预设页 / 关于页点击返回会话 |
| connect-requested / disconnect-requested | 点击连接 / 断开 |
| subscribe-requested | 点击订阅 |
| unsubscribe-requested | 点击订阅条目右侧 `×`，参数为索引 |
| filter-subscription-requested | 点击订阅条目按 Topic 筛选消息，再点一次取消 |
| publish-requested | 点击底部发布 |
| preset-publish-requested | 会话页点击某个发布预设的发送按钮，参数为下标 |
| format-payload-requested | 点击美化，按 JSON 格式化 payload 编辑框 |
| generate-client-id-requested | 点击生成 Client ID |
| timed-toggle-requested | 启动或停止定时发布 |
| log-format-changed | 切换接收显示格式 |
| log-filter-changed | 日志搜索框或类型筛选变化 |
| save-logs-requested / clear-logs-requested | 保存 / 清空日志 |
| skin-changed | 切换皮肤 |
| language-changed | 切换界面语言 |
| settings-requested | 左侧栏打开设置页 |
| about-requested | 左侧栏打开关于页 |
| auto-reply-requested | 左侧栏打开自动回复页 |
| add-auto-reply-requested | 自动回复页点击新增规则 |
| delete-auto-reply-requested | 删除指定下标的自动回复规则，越界则忽略 |
| publish-presets-requested | 左侧栏打开发布预设页 |
| add-publish-preset-requested | 发布预设页点击新增预设 |
| delete-publish-preset-requested | 删除指定下标的发布预设，越界则忽略 |

## MQTT 会话命令

每个连接对应独立的 `MqttHandle`，应用内部命令：

| 命令 | 参数 |
|------|------|
| Connect | ConnectParams：host, port, client_id, username, password, keep_alive, clean_session, connect_timeout, Last Will, SSL, protocol |
| Disconnect | 无 |
| Subscribe | topic, qos |
| Unsubscribe | topic |
| Publish | topic, qos, retain, payload（字节） |

`QoS` 由 `mqtt::qos_from_u8` 从 0/1/2 映射为 `AtMostOnce` / `AtLeastOnce` / `ExactlyOnce`。`ProtocolVersion`：索引 0 为 `3.1.1`，1 为 `5.0`。

## 自动回复评估

入口：`handle_event` 收到 `UiEvent::Recv` 后先 `push_log_for(..., "RECV")`，再调用 `maybe_auto_reply`。

匹配选择纯函数：`select_auto_reply_rule(rules, topic, payload_text) -> Option<usize>`。规则 N 命中条件：`enabled`、匹配 Topic 去空白后非空、入站 Topic 满足 MQTT 过滤器、`payload_contains` 为空或 UTF-8 文本区分大小写包含该串。

发送成功后写入 `ConnRuntime.last_auto_reply = (reply_topic, 编码后字节)`。Echo Policy 为 skip 且入站 Topic 与字节等于该槽时，记 RECV 后本轮不再评估。手动发布、预设发送、定时发布不更新该槽。匹配成功但回复 Topic 为空、编码失败或 publish 失败都写 ERR 并结束本轮。

## 配置文件

路径：可执行文件所在目录下的 `mqttc.json`。保存多个连接与界面偏好。

```json
{
  "active_id": "c18f2a0e0",
  "log_format": "text",
  "auto_scroll": true,
  "log_filter": "",
  "log_kind_filter": 0,
  "skin": "auto",
  "language": "zh",
  "connections": [
    {
      "id": "c18f2a0e0",
      "name": "本地 Broker",
      "host": "127.0.0.1",
      "port": "1883",
      "client_id": "",
      "username": "",
      "password": "",
      "protocol": "3.1.1",
      "keep_alive": 60,
      "clean_session": true,
      "connect_timeout": 10,
      "will_topic": "",
      "will_payload": "",
      "will_qos": 0,
      "will_retain": false,
      "ssl": false,
      "ssl_verify": true,
      "ssl_ca": "",
      "ssl_cert": "",
      "ssl_key": "",
      "timed_interval": 5,
      "sub_topic": "mqttc/mvp/#",
      "sub_qos": 0,
      "pub_topic": "mqttc/mvp/hello",
      "pub_payload": "hello from mqttc",
      "pub_qos": 0,
      "pub_retain": false,
      "pub_format": "text",
      "subscriptions": [
        { "topic": "mqttc/mvp/#", "qos": 1 }
      ],
      "publish_presets": [
        { "name": "heartbeat", "topic": "dev/hb", "payload": "ping", "qos": 0, "retain": false, "format": "text" }
      ],
      "auto_reply_rules": [
        {
          "enabled": false,
          "match_topic": "dev/+/status",
          "payload_contains": "ping",
          "reply_topic": "dev/out",
          "reply_payload": "pong",
          "qos": 0,
          "retain": false,
          "format": "text"
        }
      ],
      "auto_reply_echo": "skip"
    }
  ]
}
```

`skin` 取值：`auto` / `black` / `white` / `blue` / `green` / `purple`。`language` 取值：`zh` / `en`。`protocol` 取值：`3.1.1` / `5.0`。`auto_reply_echo` 取值：`skip` / `rematch`，未知值按 `skip`。

旧版扁平结构（仅有 `host`、`port`、`pub_topic` 等字段）会在加载时自动迁移为一条连接，`subscriptions` 中的纯字符串 Topic 迁移为 `{topic, qos: 0}`。缺 `publish_presets` 时预设列表为空。缺 `auto_reply_rules` / `auto_reply_echo` 时规则为空列表、Echo Policy 为 skip。已有文件中的预设条数保持原样，加载后不截断、不补齐。

写入时机：新建 / 删除 / 切换连接、连接、断开、订阅、取消订阅、发布、预设发送、切换接收格式、皮肤与语言变化、启动/停止定时发布、进入设置、关于、自动回复或发布预设页、新增或删除规则与预设，以及退出窗口时。

## 日志文件

路径：可执行文件所在目录下的 `logs/` 子目录（不存在时自动创建）。

文件名：`mqttc-YYYYMMDD-HHMMSS.log`，取保存时刻的本地时间。

每行格式：

```
[YYYY-MM-DD HH:MM:SS] [KIND] TOPIC PAYLOAD
```

系统事件无 Topic 时省略该字段。Payload 按当前接收显示格式渲染（JSON 会美化，Base64/Hex 会编码），文本按 UTF-8 写入。界面筛选不影响导出范围，始终写出当前连接内存中的全部记录。

## Broker 约定

- 传输：TCP，可选 TLS（rustls）
- 协议：MQTT 3.1.1 或 MQTT 5.0
- Clean Session：可配置，默认 true；MQTT 5.0 对应 Clean Start
- Keep Alive：可配置，默认 60 秒
- 连接超时：可配置，默认 10 秒
- Last Will：Topic 非空时随 CONNECT 发送
- 订阅与发布 QoS：可选 0/1/2，默认 0
- 发布支持 Retain 保留消息、定时循环发送、每连接可增减的发布预设，以及自动回复 Fixed Reply
- 重连后按订阅列表自动重新订阅
