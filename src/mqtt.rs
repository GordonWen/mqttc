use rumqttc::{
    Client, ConnectReturnCode, Event, Incoming, LastWill, MqttOptions, NetworkOptions, QoS,
};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct ConnectParams {
    pub host: String,
    pub port: u16,
    pub client_id: String,
    pub username: String,
    pub password: String,
    pub keep_alive: u16,
    pub clean_session: bool,
    pub connect_timeout: u16,
    pub will_topic: String,
    pub will_payload: Vec<u8>,
    pub will_qos: u8,
    pub will_retain: bool,
}

#[derive(Clone, Debug)]
pub enum UiEvent {
    Connected,
    Disconnected { reason: String },
    Recv { topic: String, payload: Vec<u8> },
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

    pub fn connect<F>(&self, params: ConnectParams, on_event: F)
    where
        F: Fn(UiEvent) + Send + 'static,
    {
        let mut inner = self.inner.lock().expect("mqtt lock");
        if let Some(client) = inner.client.take() {
            let _ = client.disconnect();
        }
        inner.generation += 1;
        let generation = inner.generation;

        let mut opts = MqttOptions::new(params.client_id, params.host, params.port);
        opts.set_keep_alive(Duration::from_secs(params.keep_alive.max(1) as u64));
        opts.set_clean_session(params.clean_session);
        if !params.username.is_empty() {
            opts.set_credentials(params.username, params.password);
        }
        let will_topic = params.will_topic.trim();
        if !will_topic.is_empty() {
            opts.set_last_will(LastWill::new(
                will_topic,
                params.will_payload,
                qos_from_u8(params.will_qos),
                params.will_retain,
            ));
        }

        let mut network = NetworkOptions::new();
        network.set_connection_timeout(params.connect_timeout.max(1) as u64);

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
                        on_event(UiEvent::Recv {
                            topic: packet.topic,
                            payload: packet.payload.to_vec(),
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

    pub fn subscribe(&self, topic: &str, qos: QoS) -> Result<(), String> {
        let inner = self.inner.lock().expect("mqtt lock");
        let client = inner.client.as_ref().ok_or_else(|| "未连接".to_string())?;
        client
            .subscribe(topic, qos)
            .map_err(|err| err.to_string())
    }

    pub fn unsubscribe(&self, topic: &str) -> Result<(), String> {
        let inner = self.inner.lock().expect("mqtt lock");
        let client = inner.client.as_ref().ok_or_else(|| "未连接".to_string())?;
        client
            .unsubscribe(topic)
            .map_err(|err| err.to_string())
    }

    pub fn publish(
        &self,
        topic: &str,
        qos: QoS,
        retain: bool,
        payload: Vec<u8>,
    ) -> Result<(), String> {
        let inner = self.inner.lock().expect("mqtt lock");
        let client = inner.client.as_ref().ok_or_else(|| "未连接".to_string())?;
        client
            .publish(topic, qos, retain, payload)
            .map_err(|err| err.to_string())
    }
}

pub fn qos_from_u8(qos: u8) -> QoS {
    match qos {
        1 => QoS::AtLeastOnce,
        2 => QoS::ExactlyOnce,
        _ => QoS::AtMostOnce,
    }
}
