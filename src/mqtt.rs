use rumqttc::{
    Client, ConnectReturnCode, Event, Incoming, LastWill, MqttOptions, NetworkOptions, QoS,
    Transport,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::{ClientConfig, RootCertStore, SignatureScheme};
use std::fs::File;
use std::io::BufReader;
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
    pub ssl: bool,
    pub ssl_verify: bool,
    pub ssl_ca: String,
    pub ssl_cert: String,
    pub ssl_key: String,
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

        let mut opts = MqttOptions::new(&params.client_id, &params.host, params.port);
        opts.set_keep_alive(Duration::from_secs(params.keep_alive.max(1) as u64));
        opts.set_clean_session(params.clean_session);
        if !params.username.is_empty() {
            opts.set_credentials(&params.username, &params.password);
        }
        if params.ssl {
            match build_tls_transport(&params) {
                Ok(transport) => {
                    opts.set_transport(transport);
                }
                Err(err) => {
                    drop(inner);
                    on_event(UiEvent::Error { message: err.clone() });
                    on_event(UiEvent::Disconnected { reason: err });
                    return;
                }
            }
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

    pub fn disconnect(&self) {
        let mut inner = self.inner.lock().expect("mqtt lock");
        inner.generation += 1;
        if let Some(client) = inner.client.take() {
            let _ = client.disconnect();
        }
    }

    pub fn subscribe(&self, topic: &str, qos: QoS) -> Result<(), String> {
        let inner = self.inner.lock().expect("mqtt lock");
        let client = inner
            .client
            .as_ref()
            .ok_or_else(|| crate::i18n::t("未连接", "Not connected"))?;
        client
            .subscribe(topic, qos)
            .map_err(|err| err.to_string())
    }

    pub fn unsubscribe(&self, topic: &str) -> Result<(), String> {
        let inner = self.inner.lock().expect("mqtt lock");
        let client = inner
            .client
            .as_ref()
            .ok_or_else(|| crate::i18n::t("未连接", "Not connected"))?;
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
        let client = inner
            .client
            .as_ref()
            .ok_or_else(|| crate::i18n::t("未连接", "Not connected"))?;
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

fn build_tls_transport(params: &ConnectParams) -> Result<Transport, String> {
    let mut roots = RootCertStore::empty();
    let ca_path = params.ssl_ca.trim();
    if !ca_path.is_empty() {
        let mut reader = BufReader::new(
            File::open(ca_path).map_err(|err| {
                crate::i18n::t(
                    &format!("读取 CA 证书失败: {err}"),
                    &format!("Failed to read CA certificate: {err}"),
                )
            })?,
        );
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
    let mut reader =
        BufReader::new(File::open(path).map_err(|err| {
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
    let mut reader =
        BufReader::new(File::open(path).map_err(|err| {
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
}
