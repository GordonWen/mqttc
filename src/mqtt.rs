//! rumqttc wrapper: MQTT 3.1.1 and 5.0 behind one [`MqttHandle`], plus rustls.
//!
//! `connect` bumps `Inner.generation` and spawns a blocking eventloop thread.
//! Stale threads see a mismatched generation and exit, so reconnect/disconnect
//! never deliver events from a previous client.

use rumqttc::{Client, ConnectReturnCode, Event, Incoming, LastWill, MqttOptions, NetworkOptions, QoS, Transport};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::{ClientConfig, RootCertStore, SignatureScheme};
use std::fs::File;
use std::io::BufReader;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

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
}

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

#[derive(Clone, Debug)]
pub enum UiEvent {
    Connected,
    Disconnected { reason: String },
    Recv { topic: String, payload: Vec<u8> },
    Error { message: String },
}

/// Cloneable handle shared by the UI thread and the rumqttc eventloop thread.
#[derive(Clone)]
pub struct MqttHandle {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    /// Incremented on every connect/disconnect. Eventloop threads exit when this no longer matches.
    generation: u64,
    client: Option<LiveClient>,
}

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

    fn subscribe(&self, topic: &str, qos: QoS) -> Result<(), String> {
        match self {
            Self::V311(client) => client.subscribe(topic, qos).map_err(|err| err.to_string()),
            Self::V5(client) => client
                .subscribe(topic, qos_v5(qos))
                .map_err(|err| err.to_string()),
        }
    }

    fn unsubscribe(&self, topic: &str) -> Result<(), String> {
        match self {
            Self::V311(client) => client.unsubscribe(topic).map_err(|err| err.to_string()),
            Self::V5(client) => client.unsubscribe(topic).map_err(|err| err.to_string()),
        }
    }

    fn publish(&self, topic: &str, qos: QoS, retain: bool, payload: Vec<u8>) -> Result<(), String> {
        match self {
            Self::V311(client) => client
                .publish(topic, qos, retain, payload)
                .map_err(|err| err.to_string()),
            Self::V5(client) => client
                .publish(topic, qos_v5(qos), retain, payload)
                .map_err(|err| err.to_string()),
        }
    }
}

fn connack_reason(code: ConnectReturnCode) -> String {
    match code {
        ConnectReturnCode::Success => crate::i18n::t("连接成功", "Connected"),
        ConnectReturnCode::RefusedProtocolVersion => {
            crate::i18n::t("协议版本不被接受", "Unacceptable protocol version")
        }
        ConnectReturnCode::BadClientId => crate::i18n::t("Client ID 不被接受", "Client ID rejected"),
        ConnectReturnCode::ServiceUnavailable => crate::i18n::t("服务不可用", "Service unavailable"),
        ConnectReturnCode::BadUserNamePassword => {
            crate::i18n::t("用户名或密码错误", "Bad username or password")
        }
        ConnectReturnCode::NotAuthorized => crate::i18n::t("未授权", "Not authorized"),
    }
}

fn connack_reason_v5(code: rumqttc::v5::mqttbytes::v5::ConnectReturnCode) -> String {
    use rumqttc::v5::mqttbytes::v5::ConnectReturnCode as Code;
    match code {
        Code::Success => crate::i18n::t("连接成功", "Connected"),
        Code::RefusedProtocolVersion | Code::UnsupportedProtocolVersion => {
            crate::i18n::t("协议版本不被接受", "Unacceptable protocol version")
        }
        Code::BadClientId | Code::ClientIdentifierNotValid => {
            crate::i18n::t("Client ID 不被接受", "Client ID rejected")
        }
        Code::ServiceUnavailable | Code::ServerUnavailable | Code::ServerBusy => {
            crate::i18n::t("服务不可用", "Service unavailable")
        }
        Code::BadUserNamePassword => {
            crate::i18n::t("用户名或密码错误", "Bad username or password")
        }
        Code::NotAuthorized => crate::i18n::t("未授权", "Not authorized"),
        Code::Banned => crate::i18n::t("客户端被禁止", "Client banned"),
        Code::BadAuthenticationMethod => {
            crate::i18n::t("认证方法不被接受", "Bad authentication method")
        }
        Code::QuotaExceeded => crate::i18n::t("超出配额", "Quota exceeded"),
        Code::RetainNotSupported => crate::i18n::t("Broker 不支持 Retain", "Retain not supported"),
        Code::QoSNotSupported => crate::i18n::t("Broker 不支持该 QoS", "QoS not supported"),
        Code::PacketTooLarge => crate::i18n::t("报文过大", "Packet too large"),
        other => format!("{other:?}"),
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
            client.disconnect();
        }
        // Invalidate the previous eventloop before spawning a new one.
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
        let inner = self.inner.lock().expect("mqtt lock");
        let client = inner
            .client
            .as_ref()
            .ok_or_else(|| crate::i18n::t("未连接", "Not connected"))?;
        client.subscribe(topic, qos)
    }

    pub fn unsubscribe(&self, topic: &str) -> Result<(), String> {
        let inner = self.inner.lock().expect("mqtt lock");
        let client = inner
            .client
            .as_ref()
            .ok_or_else(|| crate::i18n::t("未连接", "Not connected"))?;
        client.unsubscribe(topic)
    }

    pub fn publish(
        &self,
        topic: &str,
        qos: QoS,
        retain: bool,
        payload: Vec<u8>,
    ) -> Result<(), String> {
        let inner = self.inner.lock().expect("mqtt lock");
        let client = inner
            .client
            .as_ref()
            .ok_or_else(|| crate::i18n::t("未连接", "Not connected"))?;
        client.publish(topic, qos, retain, payload)
    }
}

/// Blocking rumqttc 3.1.1 eventloop. `generation` is captured so a later connect/disconnect stops this thread.
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
    // rumqttc v3.1.1 accepts 1s; v5 below floors at 5s (broker / crate minimum).
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
    drop(inner);

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
                            message: crate::i18n::t(
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

    thread::spawn(move || {
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
                            message: crate::i18n::t(
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
                    on_event(UiEvent::Recv {
                        topic,
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

/// Custom CA and/or client cert when set; otherwise the native store.
/// `ssl_verify = false` installs [`NoServerVerification`] (lab brokers with self-signed certs).
fn build_tls_transport(params: &ConnectParams) -> Result<Transport, String> {
    let mut roots = RootCertStore::empty();
    let ca_path = params.ssl_ca.trim();
    if !ca_path.is_empty() {
        let mut reader = BufReader::new(File::open(ca_path).map_err(|err| {
            crate::i18n::t(
                &format!("读取 CA 证书失败: {err}"),
                &format!("Failed to read CA certificate: {err}"),
            )
        })?);
        let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut reader)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| {
                crate::i18n::t(
                    &format!("解析 CA 证书失败: {err}"),
                    &format!("Failed to parse CA certificate: {err}"),
                )
            })?;
        if certs.is_empty() {
            return Err(crate::i18n::t(
                "CA 证书文件中没有有效证书",
                "No valid certificate in CA file",
            ));
        }
        for cert in certs {
            roots.add(cert).map_err(|err| {
                crate::i18n::t(
                    &format!("加载 CA 证书失败: {err}"),
                    &format!("Failed to load CA certificate: {err}"),
                )
            })?;
        }
    } else if params.ssl_verify {
        let native = rustls_native_certs::load_native_certs().map_err(|err| {
            crate::i18n::t(
                &format!("加载系统 CA 失败: {err}"),
                &format!("Failed to load system CA: {err}"),
            )
        })?;
        for cert in native {
            let _ = roots.add(cert);
        }
        if roots.is_empty() {
            return Err(crate::i18n::t(
                "系统 CA 证书为空，请指定 CA 文件",
                "System CA store is empty, specify a CA file",
            ));
        }
    }

    let client_certs = load_client_certs(params.ssl_cert.trim())?;
    let client_key = load_client_key(params.ssl_key.trim())?;
    if client_certs.is_some() != client_key.is_some() {
        return Err(crate::i18n::t(
            "客户端证书和私钥需要同时填写",
            "Client certificate and private key are both required",
        ));
    }

    let builder = ClientConfig::builder();
    let config = if params.ssl_verify {
        if let (Some(certs), Some(key)) = (client_certs, client_key) {
            builder
                .with_root_certificates(roots)
                .with_client_auth_cert(certs, key)
                .map_err(|err| {
                    crate::i18n::t(
                        &format!("加载客户端证书失败: {err}"),
                        &format!("Failed to load client certificate: {err}"),
                    )
                })?
        } else {
            builder
                .with_root_certificates(roots)
                .with_no_client_auth()
        }
    } else if let (Some(certs), Some(key)) = (client_certs, client_key) {
        ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoServerVerification))
            .with_client_auth_cert(certs, key)
            .map_err(|err| {
                crate::i18n::t(
                    &format!("加载客户端证书失败: {err}"),
                    &format!("Failed to load client certificate: {err}"),
                )
            })?
    } else {
        ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoServerVerification))
            .with_no_client_auth()
    };
    Ok(Transport::tls_with_config(rumqttc::TlsConfiguration::Rustls(
        Arc::new(config),
    )))
}

fn load_client_certs(path: &str) -> Result<Option<Vec<CertificateDer<'static>>>, String> {
    if path.is_empty() {
        return Ok(None);
    }
    let mut reader = BufReader::new(File::open(path).map_err(|err| {
        crate::i18n::t(
            &format!("读取客户端证书失败: {err}"),
            &format!("Failed to read client certificate: {err}"),
        )
    })?);
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| {
            crate::i18n::t(
                &format!("解析客户端证书失败: {err}"),
                &format!("Failed to parse client certificate: {err}"),
            )
        })?;
    if certs.is_empty() {
        return Err(crate::i18n::t(
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
        crate::i18n::t(
            &format!("读取客户端私钥失败: {err}"),
            &format!("Failed to read client private key: {err}"),
        )
    })?);
    rustls_pemfile::private_key(&mut reader)
        .map_err(|err| {
            crate::i18n::t(
                &format!("解析客户端私钥失败: {err}"),
                &format!("Failed to parse client private key: {err}"),
            )
        })?
        .ok_or_else(|| {
            crate::i18n::t(
                "客户端私钥文件中没有有效私钥",
                "No valid private key in client key file",
            )
        })
        .map(Some)
}

/// Accepts any server certificate. Only used when the user turns SSL verify off.
#[derive(Debug)]
struct NoServerVerification;

impl rustls::client::danger::ServerCertVerifier for NoServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_params() -> ConnectParams {
        ConnectParams {
            host: "localhost".into(),
            port: 8883,
            client_id: "t".into(),
            username: String::new(),
            password: String::new(),
            keep_alive: 60,
            clean_session: true,
            connect_timeout: 10,
            will_topic: String::new(),
            will_payload: vec![],
            will_qos: 0,
            will_retain: false,
            ssl: true,
            ssl_verify: false,
            ssl_ca: String::new(),
            ssl_cert: String::new(),
            ssl_key: String::new(),
            protocol: ProtocolVersion::V311,
        }
    }

    #[test]
    fn tls_without_verify_builds() {
        assert!(build_tls_transport(&dummy_params()).is_ok());
    }

    #[test]
    fn tls_missing_ca_file_errors() {
        let mut params = dummy_params();
        params.ssl_verify = true;
        params.ssl_ca = "/no/such/ca.pem".into();
        match build_tls_transport(&params) {
            Err(err) => assert!(err.contains("CA")),
            Ok(_) => panic!("expected missing CA file to fail"),
        }
    }

    #[test]
    fn protocol_index_and_key_round_trip() {
        assert_eq!(ProtocolVersion::from_index(0), ProtocolVersion::V311);
        assert_eq!(ProtocolVersion::from_index(1), ProtocolVersion::V5);
        assert_eq!(ProtocolVersion::from_key("3.1.1"), ProtocolVersion::V311);
        assert_eq!(ProtocolVersion::from_key("5.0"), ProtocolVersion::V5);
        assert_eq!(ProtocolVersion::from_key("5"), ProtocolVersion::V5);
        assert_eq!(ProtocolVersion::from_key("unknown"), ProtocolVersion::V311);
        assert_eq!(ProtocolVersion::V311.to_key(), "3.1.1");
        assert_eq!(ProtocolVersion::V5.to_key(), "5.0");
        assert_eq!(ProtocolVersion::V5.to_index(), 1);
    }
}
