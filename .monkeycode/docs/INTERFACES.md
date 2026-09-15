# 接口定义

## UI 组件接口

`AppWindow` 是唯一窗口组件，属性与回调如下。

### 属性

| 属性 | 类型 | 含义 |
|------|------|------|
| host | string | Broker 主机，默认 `127.0.0.1` |
| port | string | Broker 端口，默认 `1883` |
| client-id | string | MQTT Client ID，空则自动生成 |
| username | string | MQTT 用户名，空则匿名连接 |
| password | string | MQTT 密码，仅在用户名非空时发送 |
| status | string | 连接状态文案 |
| status-color | color | 状态指示颜色 |
| connected | bool | 是否已连接，控制按钮可用性 |
| sub-topic | string | 待订阅 Topic |
| pub-topic | string | 待发布 Topic |
| pub-payload | string | 待发布文本，支持多行 |
| logs | [LogEntry] | 日志列表 |
| subscriptions | [Subscription] | 已订阅 Topic 列表 |
| auto-scroll | bool | 日志自动滚动开关，默认 true |
| log-scroll-y | length | 日志滚动偏移，与日志列表双向绑定 |

### 回调

| 回调 | 触发时机 |
|------|----------|
| connect-requested | 点击连接 |
| disconnect-requested | 点击断开 |
| subscribe-requested | 点击订阅 |
| unsubscribe-requested | 点击订阅列表中的取消按钮，参数为索引 |
| publish-requested | 点击发布 |
| save-logs-requested | 点击保存日志，导出为文件 |
| clear-logs-requested | 点击清空日志 |

### LogEntry

| 字段 | 类型 | 含义 |
|------|------|------|
| time | string | 本地时间 `HH:MM:SS` |
| timestamp | string | 完整时间戳 `YYYY-MM-DD HH:MM:SS`，用于导出 |
| kind | string | `SYS` / `RECV` / `SEND` / `ERR` |
| topic | string | 相关 Topic，系统事件可为空 |
| payload | string | 消息内容或说明 |

日志列表按条目高度自适应，Topic 与 Payload 均支持换行显示。

### Subscription

| 字段 | 类型 | 含义 |
|------|------|------|
| topic | string | 已订阅的 Topic |

订阅成功后写入列表并持久化；取消时移出列表。连接成功后自动重新订阅列表内全部 Topic（Clean Session 下重连需恢复订阅）。

## MQTT 会话命令

应用内部命令（UI 到会话）：

| 命令 | 参数 |
|------|------|
| Connect | host, port, client_id, username, password |
| Disconnect | 无 |
| Subscribe | topic |
| Unsubscribe | topic |
| Publish | topic, payload |

## 配置文件

路径：可执行文件所在目录下的 `mqttc.json`。

```json
{
  "host": "127.0.0.1",
  "port": "1883",
  "client_id": "",
  "username": "",
  "password": "",
  "sub_topic": "mqttc/mvp/#",
  "pub_topic": "mqttc/mvp/hello",
  "pub_payload": "hello from mqttc",
  "subscriptions": []
}
```

启动时读取；连接、订阅、取消订阅、发布成功以及退出时写入。

## 日志文件

路径：可执行文件所在目录下的 `logs/` 子目录（不存在时自动创建）。

文件名：`mqttc-YYYYMMDD-HHMMSS.log`，取保存时刻的本地时间。

每行格式：

```
[YYYY-MM-DD HH:MM:SS] [KIND] TOPIC PAYLOAD
```

系统事件无 Topic 时省略该字段。文本按 UTF-8 写入。

## Broker 约定（MVP）

- 传输：TCP
- 协议：MQTT 3.1.1
- Clean Session：true
- Keep Alive：30 秒
- 连接超时：10 秒
- 订阅与发布 QoS：0
- Payload：按 UTF-8 有损显示
