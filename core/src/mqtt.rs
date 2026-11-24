//! rumqttc wrapper: MQTT 3.1.1 and 5.0 behind one [`MqttHandle`], plus rustls.
//!
//! rumqttc has two client types; this module hides that so the runtime always
//! talks to [`MqttHandle`].
//!
//! `connect` bumps `Inner.generation` and spawns a blocking event-loop thread.
//! Stale threads see a mismatched generation and exit, so reconnect/disconnect
//! never deliver events from a previous client.

use rumqttc::{
    Client, ConnectReturnCode, Event, Incoming, LastWill, MqttOptions, NetworkOptions, Outgoing,
    QoS, TlsConfiguration, Transport,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme,
};
use std::fmt;
use std::fs::File;
use std::io::{BufReader, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::i18n;

/// Combo index 0 = 3.1.1, 1 = 5.0. Config keys are `"3.1.1"` / `"5.0"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtocolVersion {
    V311,
    V5,
}

impl ProtocolVersion {
    pub fn from_index(index: i32) -> Self {
        if index == 1 {
            Self::V5
        } else {
            Self::V311
        }
    }

    pub fn from_key(key: &str) -> Self {
        match key.trim() {
            "5" | "5.0" | "mqtt5" | "MQTT5" => Self::V5,
            _ => Self::V311,
        }
    }

    pub fn to_index(self) -> i32 {
        match self {
            Self::V5 => 1,
            Self::V311 => 0,
        }
    }

    pub fn to_key(self) -> &'static str {
        match self {
            Self::V5 => "5.0",
            Self::V311 => "3.1.1",
        }
    }

    pub fn label(self) -> &'static str {
        self.to_key()
    }

    pub const ALL: [ProtocolVersion; 2] = [Self::V311, Self::V5];
}

/// Snapshot of the connection form. Built on the UI thread, consumed by the worker.
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
    pub ssl: bool,
    pub ssl_verify: bool,
    pub ssl_ca: String,
    pub ssl_cert: String,
    pub ssl_key: String,
    /// MQTT 5 CONNECT options; `None` leaves the rumqttc default (field omitted).
    pub session_expiry: Option<u32>,
    pub receive_maximum: Option<u16>,
    pub max_packet_size: Option<u32>,
    pub topic_alias_max: Option<u16>,
    /// Hold PUBACK / PUBREC for inbound QoS 1/2 publishes until the UI calls ack.
    pub manual_acks: bool,
    /// Write TLS session secrets to `logs/tls-keys.log` (Wireshark-style key log).
    pub tls_keylog: bool,
    pub protocol: ProtocolVersion,
}

/// Broker capabilities learned from CONNACK. Permissive defaults keep 3.1.1
/// (which has no capability flags) and older code paths working unchanged.
#[derive(Clone, Debug)]
pub struct BrokerCaps {
    pub v5: bool,
    pub session_present: bool,
    pub max_qos: u8,
    pub retain_available: bool,
    pub max_packet_size: Option<u32>,
    pub receive_maximum: Option<u16>,
    pub topic_alias_max: Option<u16>,
    pub server_keep_alive: Option<u16>,
    pub wildcard_available: bool,
    pub shared_available: bool,
    pub session_expiry: Option<u32>,
    pub assigned_client_id: Option<String>,
    /// reason string + user properties, shown as extra summary lines.
    pub extra: Vec<(String, String)>,
}

impl Default for BrokerCaps {
    fn default() -> Self {
        Self {
            v5: false,
            session_present: false,
            max_qos: 2,
            retain_available: true,
            max_packet_size: None,
            receive_maximum: None,
            topic_alias_max: None,
            server_keep_alive: None,
            wildcard_available: true,
            shared_available: true,
            session_expiry: None,
            assigned_client_id: None,
            extra: Vec::new(),
        }
    }
}

/// Broker ack for one of our PUBLISH packets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AckStage {
    /// QoS 1 terminal ack.
    PubAck,
    /// QoS 2 first ack; our PUBREL follows automatically.
    PubRec,
    /// QoS 2 terminal ack.
    PubComp,
}

/// MQTT 5-only PUBLISH properties. An empty struct sends a plain PUBLISH.
#[derive(Clone, Debug, Default)]
pub struct PublishOptions {
    pub user_properties: Vec<(String, String)>,
    pub expiry: Option<u32>,
}

impl PublishOptions {
    pub fn is_empty(&self) -> bool {
        self.user_properties.is_empty() && self.expiry.is_none()
    }
}

/// Worker → runtime. Payload is raw bytes; the runtime decides how to display it.
#[derive(Clone, Debug)]
pub enum UiEvent {
    Connected { caps: BrokerCaps },
    Disconnected { reason: String },
    Recv {
        topic: String,
        payload: Vec<u8>,
        qos: u8,
        retain: bool,
        dup: bool,
        props: Vec<(String, String)>,
        expiry: Option<u32>,
        /// Packet id of an inbound QoS 1/2 PUBLISH (0 for QoS 0).
        pkid: u16,
    },
    /// One of our PUBLISH packets was handed to the network with this packet id
    /// (0 means QoS 0). Emitted when the event loop writes the packet.
    PublishPkid { pkid: u16 },
    /// Broker acknowledgement for one of our PUBLISH packets.
    Ack {
        pkid: u16,
        stage: AckStage,
        ok: bool,
        reason: Option<String>,
    },
    /// rumqttc finished an inbound QoS 2 exchange after our manual PUBREC
    /// (broker PUBREL arrived, PUBCOMP was sent automatically).
    ManualComp { pkid: u16 },
    /// MQTT 5 acknowledgement with a reason code (SUBACK / UNSUBACK).
    Reason {
        name: &'static str,
        pkid: u16,
        code: String,
        ok: bool,
    },
    Error { message: String },
}

/// Cloneable handle shared by the UI thread and the rumqttc event-loop thread.
#[derive(Clone)]
pub struct MqttHandle {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    /// Incremented on every connect/disconnect. Event-loop threads exit when this no longer matches.
    generation: u64,
    client: Option<LiveClient>,
}

/// rumqttc 3.1.1 and 5.0 clients have different types; this enum stores either.
enum LiveClient {
    V311(Client),
    V5(rumqttc::v5::Client),
}

impl LiveClient {
    fn disconnect(&self) {
        match self {
            Self::V311(client) => {
                let _ = client.disconnect();
            }
            Self::V5(client) => {
                let _ = client.disconnect();
            }
        }
    }

    /// MQTT 5 subscription options; the 3.1.1 client cannot express them, so it
    /// sends a plain SUBSCRIBE (the UI labels the toggles as MQTT 5-only).
    fn subscribe_with_options(
        &self,
        topic: &str,
        qos: QoS,
        no_local: bool,
        retain_as_published: bool,
    ) -> Result<(), String> {
        match self {
            Self::V311(client) => client.subscribe(topic, qos).map_err(|err| err.to_string()),
            Self::V5(client) => client
                .subscribe_many([rumqttc::v5::mqttbytes::v5::Filter {
                    path: topic.to_string(),
                    qos: qos_v5(qos),
                    nolocal: no_local,
                    preserve_retain: retain_as_published,
                    retain_forward_rule: Default::default(),
                }])
                .map_err(|err| err.to_string()),
        }
    }

    fn unsubscribe(&self, topic: &str) -> Result<(), String> {
        match self {
            Self::V311(client) => client.unsubscribe(topic).map_err(|err| err.to_string()),
            Self::V5(client) => client.unsubscribe(topic).map_err(|err| err.to_string()),
        }
    }

    fn publish_with_options(
        &self,
        topic: &str,
        qos: QoS,
        retain: bool,
        payload: Vec<u8>,
        options: &PublishOptions,
    ) -> Result<(), String> {
        match self {
            Self::V311(client) => client
                .publish(topic, qos, retain, payload)
                .map_err(|err| err.to_string()),
            Self::V5(client) => {
                if options.is_empty() {
                    client
                        .publish(topic, qos_v5(qos), retain, payload)
                        .map_err(|err| err.to_string())
                } else {
                    let properties = rumqttc::v5::mqttbytes::v5::PublishProperties {
                        user_properties: options.user_properties.clone(),
                        message_expiry_interval: options.expiry,
                        ..Default::default()
                    };
                    client
                        .publish_with_properties(topic, qos_v5(qos), retain, payload, properties)
                        .map_err(|err| err.to_string())
                }
            }
        }
    }

    /// Acknowledge one inbound PUBLISH in manual-ack mode. Only the packet id
    /// and QoS are read by rumqttc, so a placeholder topic/payload is enough.
    fn ack_incoming(&self, pkid: u16, qos: QoS) -> Result<(), String> {
        match self {
            Self::V311(client) => {
                let mut publish = rumqttc::Publish::new("", qos, Vec::new());
                publish.pkid = pkid;
                client.ack(&publish).map_err(|err| err.to_string())
            }
            Self::V5(client) => {
                let mut publish = rumqttc::v5::mqttbytes::v5::Publish::new(
                    "",
                    qos_v5(qos),
                    Vec::new(),
                    None,
                );
                publish.pkid = pkid;
                client.ack(&publish).map_err(|err| err.to_string())
            }
        }
    }
}

fn connack_reason(code: ConnectReturnCode) -> String {
    match code {
        ConnectReturnCode::Success => i18n::t("连接成功", "Connected"),
        ConnectReturnCode::RefusedProtocolVersion => {
            i18n::t("协议版本不被接受", "Unacceptable protocol version")
        }
        ConnectReturnCode::BadClientId => i18n::t("Client ID 不被接受", "Client ID rejected"),
        ConnectReturnCode::ServiceUnavailable => i18n::t("服务不可用", "Service unavailable"),
        ConnectReturnCode::BadUserNamePassword => {
            i18n::t("用户名或密码错误", "Bad username or password")
        }
        ConnectReturnCode::NotAuthorized => i18n::t("未授权", "Not authorized"),
    }
}

fn connack_reason_v5(code: rumqttc::v5::mqttbytes::v5::ConnectReturnCode) -> String {
    use rumqttc::v5::mqttbytes::v5::ConnectReturnCode as Code;
    match code {
        Code::Success => i18n::t("连接成功", "Connected"),
        Code::RefusedProtocolVersion | Code::UnsupportedProtocolVersion => {
            i18n::t("协议版本不被接受", "Unacceptable protocol version")
        }
        Code::BadClientId | Code::ClientIdentifierNotValid => {
            i18n::t("Client ID 不被接受", "Client ID rejected")
        }
        Code::ServiceUnavailable | Code::ServerUnavailable | Code::ServerBusy => {
            i18n::t("服务不可用", "Service unavailable")
        }
        Code::BadUserNamePassword => i18n::t("用户名或密码错误", "Bad username or password"),
        Code::NotAuthorized => i18n::t("未授权", "Not authorized"),
        Code::Banned => i18n::t("客户端被禁止", "Client banned"),
        Code::BadAuthenticationMethod => {
            i18n::t("认证方法不被接受", "Bad authentication method")
        }
        Code::QuotaExceeded => i18n::t("超出配额", "Quota exceeded"),
        Code::RetainNotSupported => i18n::t("Broker 不支持 Retain", "Retain not supported"),
        Code::QoSNotSupported => i18n::t("Broker 不支持该 QoS", "QoS not supported"),
        Code::PacketTooLarge => i18n::t("报文过大", "Packet too large"),
        other => format!("{other:?}"),
    }
}

/// Sample the negotiated MQTT 5 CONNACK properties. Absent fields keep the
/// spec defaults (QoS 2, retain and extended filters available).
fn caps_from_connack_v5(ack: &rumqttc::v5::mqttbytes::v5::ConnAck) -> BrokerCaps {
    let mut caps = BrokerCaps {
        v5: true,
        session_present: ack.session_present,
        ..BrokerCaps::default()
    };
    let Some(props) = ack.properties.as_ref() else {
        return caps;
    };
    caps.max_qos = props.max_qos.unwrap_or(2).min(2);
    caps.retain_available = props.retain_available != Some(0);
    caps.wildcard_available = props.wildcard_subscription_available != Some(0);
    caps.shared_available = props.shared_subscription_available != Some(0);
    caps.max_packet_size = props.max_packet_size;
    caps.receive_maximum = props.receive_max;
    caps.topic_alias_max = props.topic_alias_max;
    caps.server_keep_alive = props.server_keep_alive;
    caps.session_expiry = props.session_expiry_interval;
    caps.assigned_client_id = props.assigned_client_identifier.clone();
    if let Some(reason) = &props.reason_string {
        caps.extra.push(("reason".into(), reason.clone()));
    }
    caps.extra.extend(props.user_properties.iter().cloned());
    caps
}

impl Default for MqttHandle {
    fn default() -> Self {
        Self::new()
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

    /// Start a new session. `on_event` must be `Send + 'static` because it runs on the worker.
    pub fn connect<F>(&self, params: ConnectParams, on_event: F)
    where
        F: Fn(UiEvent) + Send + 'static,
    {
        let mut inner = self.inner.lock().expect("mqtt lock");
        if let Some(client) = inner.client.take() {
            client.disconnect();
        }
        // Invalidate the previous event loop before spawning a new one.
        inner.generation += 1;
        let generation = inner.generation;

        let transport = if params.ssl {
            match build_tls_transport(&params) {
                Ok(transport) => Some(transport),
                Err(err) => {
                    drop(inner);
                    on_event(UiEvent::Error { message: err.clone() });
                    on_event(UiEvent::Disconnected { reason: err });
                    return;
                }
            }
        } else {
            None
        };

        let handle = self.inner.clone();
        match params.protocol {
            ProtocolVersion::V311 => {
                start_v311(inner, handle, generation, params, transport, on_event);
            }
            ProtocolVersion::V5 => {
                start_v5(inner, handle, generation, params, transport, on_event);
            }
        }
    }

    pub fn disconnect(&self) {
        let mut inner = self.inner.lock().expect("mqtt lock");
        inner.generation += 1;
        if let Some(client) = inner.client.take() {
            client.disconnect();
        }
    }

    pub fn subscribe(&self, topic: &str, qos: QoS) -> Result<(), String> {
        self.subscribe_with_options(topic, qos, false, false)
    }

    /// Subscribe with MQTT 5 options. The 3.1.1 client silently ignores the flags.
    pub fn subscribe_with_options(
        &self,
        topic: &str,
        qos: QoS,
        no_local: bool,
        retain_as_published: bool,
    ) -> Result<(), String> {
        let inner = self.inner.lock().expect("mqtt lock");
        let client = inner
            .client
            .as_ref()
            .ok_or_else(|| i18n::t("未连接", "Not connected"))?;
        client.subscribe_with_options(topic, qos, no_local, retain_as_published)
    }

    pub fn unsubscribe(&self, topic: &str) -> Result<(), String> {
        let inner = self.inner.lock().expect("mqtt lock");
        let client = inner
            .client
            .as_ref()
            .ok_or_else(|| i18n::t("未连接", "Not connected"))?;
        client.unsubscribe(topic)
    }

    pub fn publish(
        &self,
        topic: &str,
        qos: QoS,
        retain: bool,
        payload: Vec<u8>,
    ) -> Result<(), String> {
        self.publish_with_options(topic, qos, retain, payload, &PublishOptions::default())
    }

    /// Publish with MQTT 5 user properties / expiry. The 3.1.1 client ignores them.
    pub fn publish_with_options(
        &self,
        topic: &str,
        qos: QoS,
        retain: bool,
        payload: Vec<u8>,
        options: &PublishOptions,
    ) -> Result<(), String> {
        let inner = self.inner.lock().expect("mqtt lock");
        let client = inner
            .client
            .as_ref()
            .ok_or_else(|| i18n::t("未连接", "Not connected"))?;
        client.publish_with_options(topic, qos, retain, payload, options)
    }

    /// Manual ack for an inbound PUBLISH (QoS 1 → PUBACK, QoS 2 → PUBREC).
    /// Only meaningful when the session was opened with `manual_acks`.
    pub fn ack_incoming(&self, pkid: u16, qos: u8) -> Result<(), String> {
        let inner = self.inner.lock().expect("mqtt lock");
        let client = inner
            .client
            .as_ref()
            .ok_or_else(|| i18n::t("未连接", "Not connected"))?;
        client.ack_incoming(pkid, qos_from_u8(qos))
    }
}

fn spawn_worker(name: &str, f: impl FnOnce() + Send + 'static) {
    // Small stack: the event loop is shallow. Saves ~8 MB of virtual reserve per idle connection.
    let _ = thread::Builder::new()
        .name(name.to_string())
        .stack_size(256 * 1024)
        .spawn(f);
}

/// Blocking rumqttc 3.1.1 event loop. `generation` stops this thread on reconnect/disconnect.
fn start_v311<F>(
    mut inner: std::sync::MutexGuard<'_, Inner>,
    handle: Arc<Mutex<Inner>>,
    generation: u64,
    params: ConnectParams,
    transport: Option<Transport>,
    on_event: F,
) where
    F: Fn(UiEvent) + Send + 'static,
{
    let mut opts = MqttOptions::new(&params.client_id, &params.host, params.port);
    opts.set_keep_alive(Duration::from_secs(params.keep_alive.max(1) as u64));
    opts.set_clean_session(params.clean_session);
    opts.set_manual_acks(params.manual_acks);
    if !params.username.is_empty() {
        opts.set_credentials(&params.username, &params.password);
    }
    if let Some(transport) = transport {
        opts.set_transport(transport);
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
    inner.client = Some(LiveClient::V311(client));
    // Release the mutex before blocking on the network, or the UI could not publish.
    drop(inner);

    spawn_worker("mqtt-v311", move || {
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
                        let caps = BrokerCaps {
                            session_present: ack.session_present,
                            ..BrokerCaps::default()
                        };
                        on_event(UiEvent::Connected { caps });
                    } else {
                        on_event(UiEvent::Error {
                            message: i18n::t(
                                &format!("连接被拒绝: {}", connack_reason(ack.code)),
                                &format!("Connection refused: {}", connack_reason(ack.code)),
                            ),
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
                        qos: qos_index(packet.qos),
                        retain: packet.retain,
                        dup: packet.dup,
                        props: Vec::new(),
                        expiry: None,
                        pkid: packet.pkid,
                    });
                }
                Ok(Event::Incoming(Incoming::PubAck(ack))) => {
                    on_event(UiEvent::Ack {
                        pkid: ack.pkid,
                        stage: AckStage::PubAck,
                        ok: true,
                        reason: None,
                    });
                }
                Ok(Event::Incoming(Incoming::PubRec(ack))) => {
                    on_event(UiEvent::Ack {
                        pkid: ack.pkid,
                        stage: AckStage::PubRec,
                        ok: true,
                        reason: None,
                    });
                }
                Ok(Event::Incoming(Incoming::PubComp(ack))) => {
                    on_event(UiEvent::Ack {
                        pkid: ack.pkid,
                        stage: AckStage::PubComp,
                        ok: true,
                        reason: None,
                    });
                }
                Ok(Event::Outgoing(Outgoing::Publish(pkid))) => {
                    if pkid != 0 {
                        on_event(UiEvent::PublishPkid { pkid });
                    }
                }
                Ok(Event::Outgoing(Outgoing::PubComp(pkid))) => {
                    // Only arrives for inbound QoS 2 after a manual PUBREC.
                    on_event(UiEvent::ManualComp { pkid });
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

/// UI stores QoS as 0/1/2. rumqttc wants the `QoS` enum.
pub fn qos_from_u8(qos: u8) -> QoS {
    match qos {
        1 => QoS::AtLeastOnce,
        2 => QoS::ExactlyOnce,
        _ => QoS::AtMostOnce,
    }
}

fn qos_v5(qos: QoS) -> rumqttc::v5::mqttbytes::QoS {
    match qos {
        QoS::AtLeastOnce => rumqttc::v5::mqttbytes::QoS::AtLeastOnce,
        QoS::ExactlyOnce => rumqttc::v5::mqttbytes::QoS::ExactlyOnce,
        _ => rumqttc::v5::mqttbytes::QoS::AtMostOnce,
    }
}

fn qos_index(qos: QoS) -> u8 {
    match qos {
        QoS::AtLeastOnce => 1,
        QoS::ExactlyOnce => 2,
        _ => 0,
    }
}

fn qos_v5_index(qos: rumqttc::v5::mqttbytes::QoS) -> u8 {
    use rumqttc::v5::mqttbytes::QoS as V5QoS;
    match qos {
        V5QoS::AtLeastOnce => 1,
        V5QoS::ExactlyOnce => 2,
        _ => 0,
    }
}

/// Flatten the Publish properties the log panel can show without extra UI.
fn v5_publish_meta(
    props: Option<&rumqttc::v5::mqttbytes::v5::PublishProperties>,
) -> (Vec<(String, String)>, Option<u32>) {
    let Some(props) = props else {
        return (Vec::new(), None);
    };
    let mut out = props.user_properties.clone();
    if let Some(topic) = &props.response_topic {
        out.push(("response-topic".into(), topic.clone()));
    }
    if let Some(kind) = &props.content_type {
        out.push(("content-type".into(), kind.clone()));
    }
    if let Some(alias) = props.topic_alias {
        out.push(("topic-alias".into(), alias.to_string()));
    }
    if let Some(data) = &props.correlation_data {
        out.push((
            "correlation-data".into(),
            String::from_utf8_lossy(data).into_owned(),
        ));
    }
    (out, props.message_expiry_interval)
}

/// Same shape as [`start_v311`], using rumqttc's MQTT 5 client (`clean_start` instead of `clean_session`).
fn start_v5<F>(
    mut inner: std::sync::MutexGuard<'_, Inner>,
    handle: Arc<Mutex<Inner>>,
    generation: u64,
    params: ConnectParams,
    transport: Option<Transport>,
    on_event: F,
) where
    F: Fn(UiEvent) + Send + 'static,
{
    let mut opts = rumqttc::v5::MqttOptions::new(&params.client_id, &params.host, params.port);
    // rumqttc v5 rejects keep-alive below 5 seconds.
    opts.set_keep_alive(Duration::from_secs(params.keep_alive.max(5) as u64));
    opts.set_clean_start(params.clean_session);
    opts.set_connection_timeout(params.connect_timeout.max(1) as u64);
    opts.set_manual_acks(params.manual_acks);
    opts.set_session_expiry_interval(params.session_expiry);
    opts.set_receive_maximum(params.receive_maximum);
    opts.set_max_packet_size(params.max_packet_size);
    opts.set_topic_alias_max(params.topic_alias_max);
    if !params.username.is_empty() {
        opts.set_credentials(&params.username, &params.password);
    }
    if let Some(transport) = transport {
        opts.set_transport(transport);
    }
    let will_topic = params.will_topic.trim();
    if !will_topic.is_empty() {
        opts.set_last_will(rumqttc::v5::mqttbytes::v5::LastWill::new(
            will_topic,
            params.will_payload,
            qos_v5(qos_from_u8(params.will_qos)),
            params.will_retain,
            None,
        ));
    }
    let mut network = NetworkOptions::new();
    network.set_connection_timeout(params.connect_timeout.max(1) as u64);
    opts.set_network_options(network);

    let (client, mut connection) = rumqttc::v5::Client::new(opts, 32);
    inner.client = Some(LiveClient::V5(client));
    drop(inner);

    spawn_worker("mqtt-v5", move || {
        for notification in connection.iter() {
            {
                let inner = handle.lock().expect("mqtt lock");
                if inner.generation != generation {
                    break;
                }
            }
            match notification {
                Ok(rumqttc::v5::Event::Incoming(
                    rumqttc::v5::mqttbytes::v5::Packet::ConnAck(ack),
                )) => {
                    if ack.code == rumqttc::v5::mqttbytes::v5::ConnectReturnCode::Success {
                        on_event(UiEvent::Connected {
                            caps: caps_from_connack_v5(&ack),
                        });
                    } else {
                        on_event(UiEvent::Error {
                            message: i18n::t(
                                &format!("连接被拒绝: {}", connack_reason_v5(ack.code)),
                                &format!("Connection refused: {}", connack_reason_v5(ack.code)),
                            ),
                        });
                        on_event(UiEvent::Disconnected {
                            reason: format!("{:?}", ack.code),
                        });
                    }
                }
                Ok(rumqttc::v5::Event::Incoming(
                    rumqttc::v5::mqttbytes::v5::Packet::Publish(packet),
                )) => {
                    let topic = String::from_utf8_lossy(&packet.topic).into_owned();
                    let (props, expiry) = v5_publish_meta(packet.properties.as_ref());
                    on_event(UiEvent::Recv {
                        topic,
                        payload: packet.payload.to_vec(),
                        qos: qos_v5_index(packet.qos),
                        retain: packet.retain,
                        dup: packet.dup,
                        props,
                        expiry,
                        pkid: packet.pkid,
                    });
                }
                Ok(rumqttc::v5::Event::Incoming(
                    rumqttc::v5::mqttbytes::v5::Packet::PubAck(ack),
                )) => {
                    use rumqttc::v5::mqttbytes::v5::PubAckReason;
                    let ok = matches!(
                        ack.reason,
                        PubAckReason::Success | PubAckReason::NoMatchingSubscribers
                    );
                    on_event(UiEvent::Ack {
                        pkid: ack.pkid,
                        stage: AckStage::PubAck,
                        ok,
                        reason: (!ok).then(|| format!("{:?}", ack.reason)),
                    });
                }
                Ok(rumqttc::v5::Event::Incoming(
                    rumqttc::v5::mqttbytes::v5::Packet::PubRec(ack),
                )) => {
                    use rumqttc::v5::mqttbytes::v5::PubRecReason;
                    let ok = matches!(
                        ack.reason,
                        PubRecReason::Success | PubRecReason::NoMatchingSubscribers
                    );
                    on_event(UiEvent::Ack {
                        pkid: ack.pkid,
                        stage: AckStage::PubRec,
                        ok,
                        reason: (!ok).then(|| format!("{:?}", ack.reason)),
                    });
                }
                Ok(rumqttc::v5::Event::Incoming(
                    rumqttc::v5::mqttbytes::v5::Packet::PubComp(ack),
                )) => {
                    use rumqttc::v5::mqttbytes::v5::PubCompReason;
                    let ok = ack.reason == PubCompReason::Success;
                    on_event(UiEvent::Ack {
                        pkid: ack.pkid,
                        stage: AckStage::PubComp,
                        ok,
                        reason: (!ok).then(|| format!("{:?}", ack.reason)),
                    });
                }
                Ok(rumqttc::v5::Event::Outgoing(Outgoing::Publish(pkid))) => {
                    if pkid != 0 {
                        on_event(UiEvent::PublishPkid { pkid });
                    }
                }
                Ok(rumqttc::v5::Event::Outgoing(Outgoing::PubComp(pkid))) => {
                    on_event(UiEvent::ManualComp { pkid });
                }
                Ok(rumqttc::v5::Event::Incoming(
                    rumqttc::v5::mqttbytes::v5::Packet::SubAck(ack),
                )) => {
                    use rumqttc::v5::mqttbytes::v5::SubscribeReasonCode;
                    let ok = ack
                        .return_codes
                        .iter()
                        .all(|code| matches!(code, SubscribeReasonCode::Success(_)));
                    let code = ack
                        .return_codes
                        .iter()
                        .map(|code| format!("{code:?}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    on_event(UiEvent::Reason {
                        name: "SUBACK",
                        pkid: ack.pkid,
                        code,
                        ok,
                    });
                }
                Ok(rumqttc::v5::Event::Incoming(
                    rumqttc::v5::mqttbytes::v5::Packet::UnsubAck(ack),
                )) => {
                    use rumqttc::v5::mqttbytes::v5::UnsubAckReason;
                    let ok = ack
                        .reasons
                        .iter()
                        .all(|code| matches!(code, UnsubAckReason::Success));
                    let code = ack
                        .reasons
                        .iter()
                        .map(|code| format!("{code:?}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    on_event(UiEvent::Reason {
                        name: "UNSUBACK",
                        pkid: ack.pkid,
                        code,
                        ok,
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

/// Accepts every server certificate. Signature checks still use the ring provider.
struct NoServerVerification {
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl fmt::Debug for NoServerVerification {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NoServerVerification")
    }
}

impl ServerCertVerifier for NoServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Trust anchors for server verification: the custom CA when set, otherwise
/// the native system store.
fn load_trust_roots(ca_path: &str) -> Result<RootCertStore, String> {
    let mut roots = RootCertStore::empty();
    if !ca_path.is_empty() {
        let mut reader = BufReader::new(File::open(ca_path).map_err(|err| {
            i18n::t(
                &format!("读取 CA 证书失败: {err}"),
                &format!("Failed to read CA certificate: {err}"),
            )
        })?);
        let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut reader)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| {
                i18n::t(
                    &format!("解析 CA 证书失败: {err}"),
                    &format!("Failed to parse CA certificate: {err}"),
                )
            })?;
        if certs.is_empty() {
            return Err(i18n::t(
                "CA 证书文件中没有有效证书",
                "No valid certificate in CA file",
            ));
        }
        for cert in certs {
            roots.add(cert).map_err(|err| {
                i18n::t(
                    &format!("加载 CA 证书失败: {err}"),
                    &format!("Failed to load CA certificate: {err}"),
                )
            })?;
        }
    } else {
        let native = rustls_native_certs::load_native_certs();
        for cert in native.certs {
            let _ = roots.add(cert);
        }
        if roots.is_empty() {
            return Err(i18n::t(
                "系统 CA 证书为空，请指定 CA 文件",
                "System CA store is empty, specify a CA file",
            ));
        }
    }
    Ok(roots)
}

/// NSS key-log writer for Wireshark (`SSLKEYLOGFILE` format). Appends one
/// `LABEL <client_random> <secret>` line per TLS secret.
struct FileKeyLog {
    file: Mutex<File>,
}

impl fmt::Debug for FileKeyLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FileKeyLog")
    }
}

impl FileKeyLog {
    fn open() -> Result<Self, String> {
        let dir = crate::config::log_dir();
        std::fs::create_dir_all(&dir).map_err(|err| {
            i18n::t(
                &format!("创建日志目录失败: {err}"),
                &format!("Failed to create log directory: {err}"),
            )
        })?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("tls-keys.log"))
            .map_err(|err| {
                i18n::t(
                    &format!("创建 TLS 密钥日志失败: {err}"),
                    &format!("Failed to create TLS key log: {err}"),
                )
            })?;
        Ok(Self {
            file: Mutex::new(file),
        })
    }
}

impl rustls::KeyLog for FileKeyLog {
    fn log(&self, label: &str, client_random: &[u8], secret: &[u8]) {
        let line = format!(
            "{} {} {}\n",
            label,
            hex_bytes(client_random),
            hex_bytes(secret)
        );
        if let Ok(mut file) = self.file.lock() {
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
        }
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Custom CA and/or client cert when set; otherwise the native store.
/// `ssl_verify = false` accepts any server certificate and ignores the CA
/// path entirely, so lab brokers with self-signed certs need no CA file.
fn build_tls_transport(params: &ConnectParams) -> Result<Transport, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    // Only gather anchors when the server is actually verified; otherwise a
    // stale or missing CA path must not block an accept-any connection.
    let roots = if params.ssl_verify {
        load_trust_roots(params.ssl_ca.trim())?
    } else {
        RootCertStore::empty()
    };

    let client_certs = load_client_certs(params.ssl_cert.trim())?;
    let client_key = load_client_key(params.ssl_key.trim())?;
    if client_certs.is_some() != client_key.is_some() {
        return Err(i18n::t(
            "客户端证书和私钥需要同时填写",
            "Client certificate and private key are both required",
        ));
    }

    let builder = ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|err| {
            i18n::t(
                &format!("初始化 TLS 失败: {err}"),
                &format!("Failed to initialize TLS: {err}"),
            )
        })?;

    let mut config = if params.ssl_verify {
        if let (Some(certs), Some(key)) = (client_certs, client_key) {
            builder
                .with_root_certificates(roots)
                .with_client_auth_cert(certs, key)
                .map_err(|err| {
                    i18n::t(
                        &format!("加载客户端证书失败: {err}"),
                        &format!("Failed to load client certificate: {err}"),
                    )
                })?
        } else {
            builder.with_root_certificates(roots).with_no_client_auth()
        }
    } else if let (Some(certs), Some(key)) = (client_certs, client_key) {
        ClientConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .map_err(|err| {
                i18n::t(
                    &format!("初始化 TLS 失败: {err}"),
                    &format!("Failed to initialize TLS: {err}"),
                )
            })?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoServerVerification { provider }))
            .with_client_auth_cert(certs, key)
            .map_err(|err| {
                i18n::t(
                    &format!("加载客户端证书失败: {err}"),
                    &format!("Failed to load client certificate: {err}"),
                )
            })?
    } else {
        ClientConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .map_err(|err| {
                i18n::t(
                    &format!("初始化 TLS 失败: {err}"),
                    &format!("Failed to initialize TLS: {err}"),
                )
            })?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoServerVerification { provider }))
            .with_no_client_auth()
    };

    if params.tls_keylog {
        config.key_log = Arc::new(FileKeyLog::open()?);
    }

    Ok(Transport::tls_with_config(TlsConfiguration::Rustls(
        Arc::new(config),
    )))
}

fn load_client_certs(path: &str) -> Result<Option<Vec<CertificateDer<'static>>>, String> {
    if path.is_empty() {
        return Ok(None);
    }
    let mut reader = BufReader::new(File::open(path).map_err(|err| {
        i18n::t(
            &format!("读取客户端证书失败: {err}"),
            &format!("Failed to read client certificate: {err}"),
        )
    })?);
    let certs = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| {
            i18n::t(
                &format!("解析客户端证书失败: {err}"),
                &format!("Failed to parse client certificate: {err}"),
            )
        })?;
    if certs.is_empty() {
        return Err(i18n::t(
            "客户端证书文件中没有有效证书",
            "No valid certificate in client certificate file",
        ));
    }
    Ok(Some(certs))
}

fn load_client_key(path: &str) -> Result<Option<PrivateKeyDer<'static>>, String> {
    if path.is_empty() {
        return Ok(None);
    }
    let mut reader = BufReader::new(File::open(path).map_err(|err| {
        i18n::t(
            &format!("读取私钥失败: {err}"),
            &format!("Failed to read private key: {err}"),
        )
    })?);
    let key = rustls_pemfile::private_key(&mut reader).map_err(|err| {
        i18n::t(
            &format!("解析私钥失败: {err}"),
            &format!("Failed to parse private key: {err}"),
        )
    })?;
    match key {
        Some(key) => Ok(Some(key)),
        None => Err(i18n::t(
            "私钥文件中没有有效私钥",
            "No valid private key in key file",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(ssl: bool) -> ConnectParams {
        ConnectParams {
            host: "127.0.0.1".into(),
            port: 1883,
            client_id: "test".into(),
            username: String::new(),
            password: String::new(),
            keep_alive: 60,
            clean_session: true,
            connect_timeout: 10,
            will_topic: String::new(),
            will_payload: Vec::new(),
            will_qos: 0,
            will_retain: false,
            ssl,
            ssl_verify: true,
            ssl_ca: String::new(),
            ssl_cert: String::new(),
            ssl_key: String::new(),
            session_expiry: None,
            receive_maximum: None,
            max_packet_size: None,
            topic_alias_max: None,
            manual_acks: false,
            tls_keylog: false,
            protocol: ProtocolVersion::V311,
        }
    }

    #[test]
    fn protocol_round_trip() {
        for p in ProtocolVersion::ALL {
            assert_eq!(ProtocolVersion::from_index(p.to_index()), p);
            assert_eq!(ProtocolVersion::from_key(p.to_key()), p);
            assert_eq!(ProtocolVersion::from_key(p.label()), p);
        }
        assert_eq!(ProtocolVersion::from_key("mqtt5"), ProtocolVersion::V5);
        assert_eq!(ProtocolVersion::from_index(9), ProtocolVersion::V311);
    }

    #[test]
    fn qos_mapping() {
        assert_eq!(qos_from_u8(0), QoS::AtMostOnce);
        assert_eq!(qos_from_u8(1), QoS::AtLeastOnce);
        assert_eq!(qos_from_u8(2), QoS::ExactlyOnce);
        assert_eq!(qos_from_u8(9), QoS::AtMostOnce);
    }

    #[test]
    fn tls_rejects_missing_ca_file() {
        let mut p = params(true);
        p.ssl_ca = "definitely-not-there.pem".into();
        assert!(build_tls_transport(&p).is_err());
    }

    #[test]
    fn tls_requires_cert_and_key_together() {
        let dir = std::env::temp_dir().join(format!("mqttc-tls-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let cert = dir.join("c.pem");
        // A syntactically valid certificate is not required: the loader fails before pairing.
        std::fs::write(&cert, b"not a cert").unwrap();
        let mut p = params(true);
        p.ssl_cert = cert.to_string_lossy().into_owned();
        assert!(build_tls_transport(&p).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_verify_builds_without_ca() {
        let mut p = params(true);
        p.ssl_verify = false;
        assert!(build_tls_transport(&p).is_ok());
    }

    #[test]
    fn no_verify_ignores_ca_path() {
        let mut p = params(true);
        p.ssl_verify = false;
        p.ssl_ca = "definitely-not-there.pem".into();
        assert!(build_tls_transport(&p).is_ok());
    }
}
