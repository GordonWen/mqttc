//! UI-agnostic core for the mqttc debug client.
//!
//! The three UI shells (egui / Iced / GPUI) only paint and route input; all
//! connection, config, log, filter and auto-reply behaviour lives here so the
//! ports cannot drift apart.
//!
//! Threading model:
//! - The UI thread owns [`runtime::AppRuntime`].
//! - Each MQTT session runs a blocking `rumqttc` event loop on its own thread.
//! - Workers never touch the runtime; they push [`runtime::RuntimeEvent`] into
//!   an [`runtime::EventSink`] and optionally call a wake callback.

pub mod cli;
pub mod config;
pub mod export;
pub mod filter;
pub mod form;
pub mod i18n;
pub mod logs;
pub mod model;
pub mod mqtt;
pub mod payload;
pub mod platform;
pub mod runtime;
pub mod theme;

pub use i18n::{t, Lang};
pub use model::{ConnStatus, LogRecord, PayloadFormat, Skin, MAX_LOGS};
