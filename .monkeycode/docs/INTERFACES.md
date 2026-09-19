# 接口定义

## UI 组件接口

`AppWindow` 是唯一窗口组件。左侧为连接列表，右侧按 `page` 切换：0 连接配置页，1 会话页。

### 通用组件

| 组件 | 说明 |
|------|------|
| Segmented | 轻量分段选择器，属性 `options`、`current-index`、`interactive`、`item-width`，回调 `activated(int)` |
| ConnectionEntry | 左侧连接列表条目，字段 `id`、`name`、`status`、`status-color` |
| Subscription | 订阅条目，字段 `topic`、`qos`、`selected` |
| LogEntry | 日志条目，字段 `time`、`timestamp`、`kind`、`topic`、`payload` |

### 属性

| 属性 | 类型 | 含义 |
|------|------|------|
| page | int | 0 连接配置页，1 会话页 |
| active-connection-id | string | 当前选中连接的 id |
| connections | [ConnectionEntry] | 左侧连接列表 |
| connection-name | string | 当前连接名称 |
| connection-summary | string | 会话页标题旁的 host:port |
| host / port / client-id | string | Broker 地址与 Client ID |
| username / password | string | 可选认证信息，密码用密文输入 |
| keep-alive / connect-timeout | string | Keep Alive 与连接超时（秒） |
| clean-session | bool | MQTT Clean Session |
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
| timed-interval | string | 定时发布间隔（秒） |
| timed-running | bool | 当前连接是否正在定时发布 |
| logs | [LogEntry] | 当前连接的日志列表（已按筛选过滤） |
| log-format | int | 接收显示格式索引，同上 |
| log-filter | string | 按 Topic / Payload / 类型关键字筛选 |
| log-kind-filter | int | 0 全部、1 收、2 发、3 系统、4 错误 |
| auto-scroll | bool | 日志自动滚动开关 |
| log-scroll-y | length | 日志滚动偏移，与日志列表双向绑定 |

### 回调

| 回调 | 触发时机 |
|------|----------|
| new-connection-requested | 点击连接列表的 `+`，进入配置页 |
| select-connection-requested | 点击连接列表条目，参数为 id，进入会话页 |
| delete-connection-requested | 点击连接条目右侧 `×`，参数为 id |
| edit-connection-requested | 会话页点击编辑，进入配置页 |
| back-to-session-requested | 配置页点击返回会话 |
| connect-requested / disconnect-requested | 点击连接 / 断开 |
| subscribe-requested | 点击订阅 |
| unsubscribe-requested | 点击订阅条目右侧 `×`，参数为索引 |
| publish-requested | 点击发布 |
| format-payload-requested | 点击美化，按 JSON 格式化 payload 编辑框 |
| generate-client-id-requested | 点击生成 Client ID |
| filter-subscription-requested | 点击订阅条目按 Topic 筛选消息，再点一次取消 |
| timed-toggle-requested | 启动或停止定时发布 |
| log-format-changed | 切换接收显示格式 |
| log-filter-changed | 日志搜索框或类型筛选变化 |
| save-logs-requested / clear-logs-requested | 保存 / 清空日志 |

## MQTT 会话命令

每个连接对应独立的 `MqttHandle`，应用内部命令：

| 命令 | 参数 |
|------|------|
| Connect | ConnectParams：host, port, client_id, username, password, keep_alive, clean_session, connect_timeout, Last Will, SSL |
| Disconnect | 无 |
| Subscribe | topic, qos |
| Unsubscribe | topic |
| Publish | topic, qos, retain, payload（字节） |

`QoS` 由 `mqtt::qos_from_u8` 从 0/1/2 映射为 `AtMostOnce` / `AtLeastOnce` / `ExactlyOnce`。

## 配置文件

路径：可执行文件所在目录下的 `mqttc.json`。保存多个连接与界面偏好。

```json
{
  "active_id": "c18f2a0e0",
  "log_format": "text",
  "auto_scroll": true,
  "log_filter": "",
  "log_kind_filter": 0,
  "connections": [
    {
      "id": "c18f2a0e0",
      "name": "本地 Broker",
      "host": "127.0.0.1",
      "port": "1883",
      "client_id": "",
      "username": "",
      "password": "",
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
      ]
    }
  ]
}
```

旧版扁平结构（仅有 `host`、`port`、`pub_topic` 等字段）会在加载时自动迁移为一条连接，`subscriptions` 中的纯字符串 Topic 迁移为 `{topic, qos: 0}`。

写入时机：新建 / 删除 / 切换连接、连接、断开、订阅、取消订阅、发布、切换接收格式、日志筛选变化、启动/停止定时发布，以及退出时。

## 日志文件

路径：可执行文件所在目录下的 `logs/` 子目录（不存在时自动创建）。

文件名：`mqttc-YYYYMMDD-HHMMSS.log`，取保存时刻的本地时间。

每行格式：

```
[YYYY-MM-DD HH:MM:SS] [KIND] TOPIC PAYLOAD
```

系统事件无 Topic 时省略该字段。Payload 按当前接收显示格式渲染（JSON 会美化，Base64/Hex 会编码），文本按 UTF-8 写入。

## Broker 约定

- 传输：TCP
- 协议：MQTT 3.1.1
- Clean Session：可配置，默认 true
- Keep Alive：可配置，默认 60 秒
- 连接超时：可配置，默认 10 秒
- Last Will：Topic 非空时随 CONNECT 发送
- 订阅与发布 QoS：可选 0/1/2，默认 0
- 发布支持 Retain 保留消息与定时循环发送
- 重连后按订阅列表自动重新订阅
