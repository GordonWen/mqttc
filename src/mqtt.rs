use rumqttc::{Client, ConnectReturnCode, Event, Incoming, MqttOptions, NetworkOptions, QoS};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Clone, Debug)]
pub enum UiEvent {
    Connected,
    Disconnected { reason: String },
    Recv { topic: String, payload: String },
    Error { message: String },
}

#[derive(Clone)]
pub struct MqttHandle {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    generation: u64,
    client: Option<Client>,
}

fn connack_reason(code: ConnectReturnCode) -> &'static str {
    match code {
        ConnectReturnCode::Success => "连接成功",
        ConnectReturnCode::RefusedProtocolVersion => "协议版本不被接受",
        ConnectReturnCode::BadClientId => "Client ID 不被接受",
        ConnectReturnCode::ServiceUnavailable => "服务不可用",
        ConnectReturnCode::BadUserNamePassword => "用户名或密码错误",
        ConnectReturnCode::NotAuthorized => "未授权",
    }
}

impl MqttHandle {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                generation: 0,
                client: None,
            })),
        }
    }

    pub fn connect<F>(
        &self,
        host: String,
        port: u16,
        client_id: String,
        username: String,
        password: String,
        on_event: F,
    ) where
        F: Fn(UiEvent) + Send + 'static,
    {
        let mut inner = self.inner.lock().expect("mqtt lock");
        if let Some(client) = inner.client.take() {
            let _ = client.disconnect();
        }
        inner.generation += 1;
        let generation = inner.generation;

        let mut opts = MqttOptions::new(client_id, host, port);
        opts.set_keep_alive(Duration::from_secs(30));
        opts.set_clean_session(true);
        if !username.is_empty() {
            opts.set_credentials(username, password);
        }

        let mut network = NetworkOptions::new();
        network.set_connection_timeout(10);

        let (client, mut connection) = Client::new(opts, 32);
        connection.eventloop.set_network_options(network);
        inner.client = Some(client);
        drop(inner);

        let handle = self.inner.clone();
        thread::spawn(move || {
            for notification in connection.iter() {
                {
                    let inner = handle.lock().expect("mqtt lock");
                    if inner.generation != generation {
                        break;
                    }
                }
                match notification {
                    Ok(Event::Incoming(Incoming::ConnAck(ack))) => {
                        if ack.code == ConnectReturnCode::Success {
                            on_event(UiEvent::Connected);
                        } else {
                            on_event(UiEvent::Error {
                                message: format!("连接被拒绝: {}", connack_reason(ack.code)),
                            });
                            on_event(UiEvent::Disconnected {
                                reason: format!("{:?}", ack.code),
                            });
                        }
                    }
                    Ok(Event::Incoming(Incoming::Publish(packet))) => {
                        let payload = String::from_utf8_lossy(&packet.payload).into_owned();
                        on_event(UiEvent::Recv {
                            topic: packet.topic,
                            payload,
                        });
                    }
                    Err(err) => {
                        on_event(UiEvent::Error {
                            message: err.to_string(),
                        });
                        on_event(UiEvent::Disconnected {
                            reason: err.to_string(),
                        });
                        break;
                    }
                    _ => {}
                }
            }
            let mut inner = handle.lock().expect("mqtt lock");
            if inner.generation == generation {
                inner.client = None;
            }
        });
    }

    pub fn disconnect(&self) {
        let mut inner = self.inner.lock().expect("mqtt lock");
        inner.generation += 1;
        if let Some(client) = inner.client.take() {
            let _ = client.disconnect();
        }
    }

    pub fn subscribe(&self, topic: &str) -> Result<(), String> {
        let inner = self.inner.lock().expect("mqtt lock");
        let client = inner.client.as_ref().ok_or_else(|| "未连接".to_string())?;
        client
            .subscribe(topic, QoS::AtMostOnce)
            .map_err(|err| err.to_string())
    }

    pub fn unsubscribe(&self, topic: &str) -> Result<(), String> {
        let inner = self.inner.lock().expect("mqtt lock");
        let client = inner.client.as_ref().ok_or_else(|| "未连接".to_string())?;
        client
            .unsubscribe(topic)
            .map_err(|err| err.to_string())
    }

    pub fn publish(&self, topic: &str, payload: &str) -> Result<(), String> {
        let inner = self.inner.lock().expect("mqtt lock");
        let client = inner.client.as_ref().ok_or_else(|| "未连接".to_string())?;
        client
            .publish(topic, QoS::AtMostOnce, false, payload.as_bytes())
            .map_err(|err| err.to_string())
    }
}
