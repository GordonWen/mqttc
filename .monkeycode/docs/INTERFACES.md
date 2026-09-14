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
| pub-payload | string | 待发布文本 |
| logs | [LogEntry] | 日志列表 |

### 回调

| 回调 | 触发时机 |
|------|----------|
| connect-requested | 点击连接 |
| disconnect-requested | 点击断开 |
| subscribe-requested | 点击订阅 |
| publish-requested | 点击发布 |
| clear-logs-requested | 点击清空日志 |

### LogEntry

| 字段 | 类型 | 含义 |
|------|------|------|
| time | string | 本地时间 `HH:MM:SS` |
| kind | string | `SYS` / `RECV` / `SEND` / `ERR` |
| topic | string | 相关 Topic，系统事件可为空 |
| payload | string | 消息内容或说明 |

## MQTT 会话命令

应用内部命令（UI 到会话）：

| 命令 | 参数 |
|------|------|
| Connect | host, port, client_id, username, password |
| Disconnect | 无 |
| Subscribe | topic |
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
  "pub_payload": "hello from mqttc"
}
```

启动时读取；连接、订阅、发布成功以及退出时写入。

## Broker 约定（MVP）

- 传输：TCP
- 协议：MQTT 3.1.1
- Clean Session：true
- Keep Alive：30 秒
- 连接超时：10 秒
- 订阅与发布 QoS：0
- Payload：按 UTF-8 有损显示
