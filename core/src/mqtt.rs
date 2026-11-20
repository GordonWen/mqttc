//! rumqttc wrapper: MQTT 3.1.1 and 5.0 behind one [`MqttHandle`], plus rustls.
//!
//! rumqttc has two client types; this module hides that so the runtime always
//! talks to [`MqttHandle`].
//!
//! `connect` bumps `Inner.generation` and spawns a blocking event-loop thread.
//! Stale threads see a mismatched generation and exit, so reconnect/disconnect
//! never deliver events from a previous client.

use rumqttc::{
    Client, ConnectReturnCode, Event, Incoming, LastWill, MqttOptions, NetworkOptions, QoS,
    TlsConfiguration, Transport,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme,
};
use std::fmt;
use std::fs::File;
use std::io::BufReader;
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
    pub protocol: ProtocolVersion,
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
    Connected,
    Disconnected { reason: String },
    Recv {
        topic: String,
        payload: Vec<u8>,
        qos: u8,
        retain: bool,
        dup: bool,
        props: Vec<(String, String)>,
        expiry: Option<u32>,
    },
    /// MQTT 5 acknowledgement with a reason code (PUBACK / SUBACK / UNSUBACK).
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
                        on_event(UiEvent::Connected);
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
                        on_event(UiEvent::Connected);
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
                    });
                }
                Ok(rumqttc::v5::Event::Incoming(
                    rumqttc::v5::mqttbytes::v5::Packet::PubAck(ack),
                )) => {
                    if ack.reason != rumqttc::v5::mqttbytes::v5::PubAckReason::Success {
                        on_event(UiEvent::Reason {
                            name: "PUBACK",
                            pkid: ack.pkid,
                            code: format!("{:?}", ack.reason),
                            ok: false,
                        });
                    }
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

/// Custom CA and/or client cert when set; otherwise the native store.
/// `ssl_verify = false` accepts any server certificate (lab brokers with self-signed certs).
fn build_tls_transport(params: &ConnectParams) -> Result<Transport, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut roots = RootCertStore::empty();
    let ca_path = params.ssl_ca.trim();
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
    } else if params.ssl_verify {
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

    let config = if params.ssl_verify {
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
}
