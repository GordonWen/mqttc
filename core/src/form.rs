//! Editable drafts for the active connection.
//!
//! The three shells bind widgets to these strings/bools and call
//! [`FormState::apply_to`] before switching connections or running an action.
//! This is the only place field clamping lives, so the ports cannot disagree.

use crate::config::{
    ensure_auto_reply_rules, ensure_publish_presets, normalize_auto_reply_echo, AutoReplyRule,
    ConnectionConfig, PublishPreset,
};
use crate::model::{parse_timed_interval, parse_u16_clamped, PayloadFormat};
use crate::mqtt::ProtocolVersion;

fn parse_interval_secs(text: &str) -> u32 {
    text.trim().parse::<u32>().unwrap_or(0).min(86_400)
}

/// Empty text box → 0 in the saved config ("omit this MQTT 5 property").
fn empty_if_zero(value: impl PartialEq + From<u8> + ToString) -> String {
    if value == 0u8.into() {
        String::new()
    } else {
        value.to_string()
    }
}

/// Lenient numeric field: empty / invalid / 0 → 0 ("omit this property"),
/// otherwise clamp to the field's protocol range.
fn parse_clamped_u32(text: &str, min: u32, max: u32) -> u32 {
    match text.trim().parse::<u64>() {
        Ok(0) | Err(_) => 0,
        Ok(value) => value.clamp(min as u64, max as u64) as u32,
    }
}

fn parse_clamped_u16(text: &str, min: u16, max: u16) -> u16 {
    match text.trim().parse::<u64>() {
        Ok(0) | Err(_) => 0,
        Ok(value) => value.clamp(min as u64, max as u64) as u16,
    }
}

/// One publish-preset row bound to the presets page.
#[derive(Clone, Debug)]
pub struct PresetRow {
    pub name: String,
    pub topic: String,
    pub payload: String,
    pub qos: i32,
    pub retain: bool,
    pub format: i32,
    /// MQTT 5 user properties (`k=v;k2=v2`) and expiry seconds.
    pub props: String,
    pub expiry: String,
}

impl Default for PresetRow {
    fn default() -> Self {
        Self {
            name: String::new(),
            topic: String::new(),
            payload: String::new(),
            qos: 0,
            retain: false,
            format: 0,
            props: String::new(),
            expiry: String::new(),
        }
    }
}

impl PresetRow {
    fn apply_to(&self, preset: &mut PublishPreset) {
        preset.name = self.name.clone();
        preset.topic = self.topic.clone();
        preset.payload = self.payload.clone();
        preset.qos = self.qos.clamp(0, 2) as u8;
        preset.retain = self.retain;
        preset.format = PayloadFormat::from_index(self.format).to_key().to_string();
        preset.props = self.props.trim().to_string();
        preset.expiry = parse_interval_secs(&self.expiry);
    }
}

/// One auto-reply rule row.
#[derive(Clone, Debug)]
pub struct RuleRow {
    pub enabled: bool,
    pub match_topic: String,
    pub payload_contains: String,
    pub reply_topic: String,
    pub reply_payload: String,
    pub qos: i32,
    pub retain: bool,
    pub format: i32,
    /// Minimum seconds between replies; empty / 0 replies to every match.
    pub min_interval_secs: String,
    /// MQTT 5 reply user properties and expiry seconds.
    pub props: String,
    pub expiry: String,
    /// Send the reply to the request's Response Topic when it has one.
    pub reply_to_response_topic: bool,
}

impl Default for RuleRow {
    fn default() -> Self {
        Self {
            enabled: false,
            match_topic: String::new(),
            payload_contains: String::new(),
            reply_topic: String::new(),
            reply_payload: String::new(),
            qos: 0,
            retain: false,
            format: 0,
            min_interval_secs: String::new(),
            props: String::new(),
            expiry: String::new(),
            reply_to_response_topic: false,
        }
    }
}

impl RuleRow {
    fn apply_to(&self, rule: &mut AutoReplyRule) {
        rule.enabled = self.enabled;
        rule.match_topic = self.match_topic.clone();
        rule.payload_contains = self.payload_contains.clone();
        rule.reply_topic = self.reply_topic.clone();
        rule.reply_payload = self.reply_payload.clone();
        rule.qos = self.qos.clamp(0, 2) as u8;
        rule.retain = self.retain;
        rule.format = PayloadFormat::from_index(self.format).to_key().to_string();
        rule.min_interval_secs = parse_interval_secs(&self.min_interval_secs);
        rule.props = self.props.trim().to_string();
        rule.expiry = parse_interval_secs(&self.expiry);
        rule.reply_to_response_topic = self.reply_to_response_topic;
    }
}

#[derive(Clone, Debug)]
pub struct FormState {
    pub name: String,
    pub host: String,
    pub port: String,
    pub client_id: String,
    pub username: String,
    pub password: String,
    pub protocol: i32,
    pub keep_alive: String,
    pub clean_session: bool,
    /// MQTT 5 CONNECT options (empty = omit). Hidden on the 3.1.1 form.
    pub session_expiry: String,
    pub receive_max: String,
    pub max_packet_size: String,
    pub topic_alias_max: String,
    pub manual_acks: bool,
    pub tls_keylog: bool,
    pub auto_reconnect: bool,
    pub connect_timeout: String,
    pub will_topic: String,
    pub will_payload: String,
    pub will_qos: i32,
    pub will_retain: bool,
    pub ssl: bool,
    pub ssl_verify: bool,
    pub ssl_ca: String,
    pub ssl_cert: String,
    pub ssl_key: String,
    pub sub_topic: String,
    pub sub_qos: i32,
    /// MQTT 5 subscription options for the next `subscribe()` call.
    pub sub_no_local: bool,
    pub sub_retain_as_published: bool,
    pub pub_topic: String,
    pub pub_payload: String,
    pub pub_qos: i32,
    pub pub_retain: bool,
    pub pub_format: i32,
    /// MQTT 5 publish user properties (`k=v;k2=v2`) and expiry seconds.
    pub pub_props: String,
    pub pub_expiry: String,
    pub timed_interval: String,
    pub presets: Vec<PresetRow>,
    pub rules: Vec<RuleRow>,
    pub auto_reply_echo: i32,
}

impl Default for FormState {
    fn default() -> Self {
        Self::from_config(&ConnectionConfig::new(String::new()))
    }
}

impl FormState {
    /// Copy saved fields into editable widgets.
    pub fn load(&mut self, cfg: &ConnectionConfig) {
        self.name = cfg.name.clone();
        self.host = cfg.host.clone();
        self.port = cfg.port.clone();
        self.client_id = cfg.client_id.clone();
        self.username = cfg.username.clone();
        self.password = cfg.password.clone();
        self.protocol = ProtocolVersion::from_key(&cfg.protocol).to_index();
        self.keep_alive = cfg.keep_alive.to_string();
        self.clean_session = cfg.clean_session;
        self.session_expiry = empty_if_zero(cfg.session_expiry);
        self.receive_max = empty_if_zero(cfg.receive_max);
        self.max_packet_size = empty_if_zero(cfg.max_packet_size);
        self.topic_alias_max = empty_if_zero(cfg.topic_alias_max);
        self.manual_acks = cfg.manual_acks;
        self.tls_keylog = cfg.tls_keylog;
        self.auto_reconnect = cfg.auto_reconnect;
        self.connect_timeout = cfg.connect_timeout.to_string();
        self.will_topic = cfg.will_topic.clone();
        self.will_payload = cfg.will_payload.clone();
        self.will_qos = cfg.will_qos.min(2) as i32;
        self.will_retain = cfg.will_retain;
        self.ssl = cfg.ssl;
        self.ssl_verify = cfg.ssl_verify;
        self.ssl_ca = cfg.ssl_ca.clone();
        self.ssl_cert = cfg.ssl_cert.clone();
        self.ssl_key = cfg.ssl_key.clone();
        self.sub_topic = cfg.sub_topic.clone();
        self.sub_qos = cfg.sub_qos.min(2) as i32;
        self.sub_no_local = false;
        self.sub_retain_as_published = false;
        self.pub_topic = cfg.pub_topic.clone();
        self.pub_payload = cfg.pub_payload.clone();
        self.pub_qos = cfg.pub_qos.min(2) as i32;
        self.pub_retain = cfg.pub_retain;
        self.pub_format = PayloadFormat::from_key(&cfg.pub_format).to_index();
        self.pub_props = cfg.pub_props.clone();
        self.pub_expiry = if cfg.pub_expiry == 0 {
            String::new()
        } else {
            cfg.pub_expiry.to_string()
        };
        self.timed_interval = cfg.timed_interval.max(1).to_string();
        self.presets = cfg
            .publish_presets
            .iter()
            .map(|preset| PresetRow {
                name: preset.name.clone(),
                topic: preset.topic.clone(),
                payload: preset.payload.clone(),
                qos: preset.qos.min(2) as i32,
                retain: preset.retain,
                format: PayloadFormat::from_key(&preset.format).to_index(),
                props: preset.props.clone(),
                expiry: if preset.expiry == 0 {
                    String::new()
                } else {
                    preset.expiry.to_string()
                },
            })
            .collect();
        self.rules = cfg
            .auto_reply_rules
            .iter()
            .map(|rule| RuleRow {
                enabled: rule.enabled,
                match_topic: rule.match_topic.clone(),
                payload_contains: rule.payload_contains.clone(),
                reply_topic: rule.reply_topic.clone(),
                reply_payload: rule.reply_payload.clone(),
                qos: rule.qos.min(2) as i32,
                retain: rule.retain,
                format: PayloadFormat::from_key(&rule.format).to_index(),
                min_interval_secs: if rule.min_interval_secs == 0 {
                    String::new()
                } else {
                    rule.min_interval_secs.to_string()
                },
                props: rule.props.clone(),
                expiry: if rule.expiry == 0 {
                    String::new()
                } else {
                    rule.expiry.to_string()
                },
                reply_to_response_topic: rule.reply_to_response_topic,
            })
            .collect();
        self.auto_reply_echo = if normalize_auto_reply_echo(&cfg.auto_reply_echo) == "rematch" {
            1
        } else {
            0
        };
    }

    /// Build a fresh draft for `cfg`. Used by the runtime on first load.
    pub fn from_config(cfg: &ConnectionConfig) -> Self {
        let mut form = Self {
            name: String::new(),
            host: String::new(),
            port: String::new(),
            client_id: String::new(),
            username: String::new(),
            password: String::new(),
            protocol: 0,
            keep_alive: String::new(),
            clean_session: true,
            session_expiry: String::new(),
            receive_max: String::new(),
            max_packet_size: String::new(),
            topic_alias_max: String::new(),
            manual_acks: false,
            tls_keylog: false,
            auto_reconnect: true,
            connect_timeout: String::new(),
            will_topic: String::new(),
            will_payload: String::new(),
            will_qos: 0,
            will_retain: false,
            ssl: false,
            ssl_verify: true,
            ssl_ca: String::new(),
            ssl_cert: String::new(),
            ssl_key: String::new(),
            sub_topic: String::new(),
            sub_qos: 0,
            sub_no_local: false,
            sub_retain_as_published: false,
            pub_topic: String::new(),
            pub_payload: String::new(),
            pub_qos: 0,
            pub_retain: false,
            pub_format: 0,
            pub_props: String::new(),
            pub_expiry: String::new(),
            timed_interval: String::new(),
            presets: Vec::new(),
            rules: Vec::new(),
            auto_reply_echo: 0,
        };
        form.load(cfg);
        form
    }

    /// Copy widgets back into a saved connection, with the reference's clamping rules.
    pub fn apply_to(&self, cfg: &mut ConnectionConfig) {
        cfg.name = self.name.clone();
        cfg.host = self.host.clone();
        cfg.port = self.port.clone();
        cfg.client_id = self.client_id.clone();
        cfg.username = self.username.clone();
        cfg.password = self.password.clone();
        cfg.protocol = ProtocolVersion::from_index(self.protocol).to_key().to_string();
        cfg.keep_alive = parse_u16_clamped(&self.keep_alive, 1, 65535, 60);
        cfg.clean_session = self.clean_session;
        cfg.session_expiry = parse_clamped_u32(&self.session_expiry, 0, u32::MAX);
        cfg.receive_max = parse_clamped_u16(&self.receive_max, 1, 65535);
        // MQTT packet size is a variable-byte integer: 1 … 268 435 455 bytes.
        cfg.max_packet_size = parse_clamped_u32(&self.max_packet_size, 1, 268_435_455);
        cfg.topic_alias_max = parse_clamped_u16(&self.topic_alias_max, 1, 65535);
        cfg.manual_acks = self.manual_acks;
        cfg.tls_keylog = self.tls_keylog;
        cfg.auto_reconnect = self.auto_reconnect;
        cfg.connect_timeout = parse_u16_clamped(&self.connect_timeout, 1, 300, 10);
        cfg.will_topic = self.will_topic.clone();
        cfg.will_payload = self.will_payload.clone();
        cfg.will_qos = self.will_qos.clamp(0, 2) as u8;
        cfg.will_retain = self.will_retain;
        cfg.ssl = self.ssl;
        cfg.ssl_verify = self.ssl_verify;
        cfg.ssl_ca = self.ssl_ca.clone();
        cfg.ssl_cert = self.ssl_cert.clone();
        cfg.ssl_key = self.ssl_key.clone();
        if let Ok(interval) = parse_timed_interval(&self.timed_interval) {
            cfg.timed_interval = interval;
        }
        cfg.sub_topic = self.sub_topic.clone();
        cfg.sub_qos = self.sub_qos.clamp(0, 2) as u8;
        cfg.pub_topic = self.pub_topic.clone();
        cfg.pub_payload = self.pub_payload.clone();
        cfg.pub_qos = self.pub_qos.clamp(0, 2) as u8;
        cfg.pub_retain = self.pub_retain;
        cfg.pub_format = PayloadFormat::from_index(self.pub_format).to_key().to_string();
        cfg.pub_props = self.pub_props.trim().to_string();
        cfg.pub_expiry = parse_interval_secs(&self.pub_expiry);

        cfg.publish_presets = self
            .presets
            .iter()
            .map(|row| {
                let mut preset = PublishPreset::default();
                row.apply_to(&mut preset);
                preset
            })
            .collect();
        ensure_publish_presets(&mut cfg.publish_presets);

        cfg.auto_reply_rules = self
            .rules
            .iter()
            .map(|row| {
                let mut rule = AutoReplyRule::default();
                row.apply_to(&mut rule);
                rule
            })
            .collect();
        ensure_auto_reply_rules(&mut cfg.auto_reply_rules);
        cfg.auto_reply_echo = if self.auto_reply_echo == 1 {
            "rematch".to_string()
        } else {
            "skip".to_string()
        };

        if cfg.name.trim().is_empty() {
            cfg.name = format!("{}:{}", cfg.host.trim(), cfg.port.trim());
        }
    }

    pub fn add_preset(&mut self) -> usize {
        self.presets.push(PresetRow::default());
        self.presets.len() - 1
    }

    pub fn remove_preset(&mut self, index: usize) {
        if index < self.presets.len() {
            self.presets.remove(index);
        }
    }

    /// Move a preset one slot up (-1) or down (+1) in list order.
    pub fn move_preset(&mut self, index: usize, delta: isize) -> bool {
        let len = self.presets.len();
        let target = index as isize + delta;
        if index >= len || target < 0 || target >= len as isize {
            return false;
        }
        self.presets.swap(index, target as usize);
        true
    }

    /// Move a preset to an absolute slot in list order (top = 0).
    pub fn move_preset_to(&mut self, index: usize, target: usize) -> bool {
        let len = self.presets.len();
        if index >= len || target >= len || index == target {
            return false;
        }
        let preset = self.presets.remove(index);
        self.presets.insert(target, preset);
        true
    }

    pub fn add_rule(&mut self) -> usize {
        self.rules.push(RuleRow::default());
        self.rules.len() - 1
    }

    pub fn remove_rule(&mut self, index: usize) {
        if index < self.rules.len() {
            self.rules.remove(index);
        }
    }

    /// Move a rule one slot up (-1) or down (+1). First match wins, so order matters.
    pub fn move_rule(&mut self, index: usize, delta: isize) -> bool {
        let len = self.rules.len();
        let target = index as isize + delta;
        if index >= len || target < 0 || target >= len as isize {
            return false;
        }
        self.rules.swap(index, target as usize);
        true
    }

    /// Move a rule to an absolute slot in list order (top = 0).
    pub fn move_rule_to(&mut self, index: usize, target: usize) -> bool {
        let len = self.rules.len();
        if index >= len || target >= len || index == target {
            return false;
        }
        let rule = self.rules.remove(index);
        self.rules.insert(target, rule);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_and_apply_round_trip() {
        let mut cfg = ConnectionConfig::new("local".into());
        cfg.host = "10.0.0.1".into();
        cfg.port = "1884".into();
        cfg.keep_alive = 30;
        cfg.connect_timeout = 5;
        cfg.pub_format = "hex".into();
        cfg.pub_qos = 1;
        cfg.protocol = "5.0".into();
        cfg.timed_interval = 9;
        cfg.session_expiry = 3600;
        cfg.receive_max = 50;
        cfg.max_packet_size = 1_048_576;
        cfg.topic_alias_max = 10;
        cfg.manual_acks = true;
        cfg.tls_keylog = true;
        cfg.subscriptions.push(crate::config::SubscriptionConfig {
            topic: "a/#".into(),
            qos: 2,
            ..Default::default()
        });
        cfg.publish_presets.push(PublishPreset {
            name: "p".into(),
            topic: "t".into(),
            payload: "x".into(),
            qos: 1,
            retain: true,
            format: "json".into(),
            props: "source=test".into(),
            expiry: 30,
        });
        cfg.auto_reply_rules.push(AutoReplyRule {
            enabled: true,
            match_topic: "in/#".into(),
            payload_contains: "ping".into(),
            reply_topic: "out".into(),
            reply_payload: "pong".into(),
            qos: 2,
            retain: false,
            format: "text".into(),
            min_interval_secs: 5,
            props: "role=echo".into(),
            expiry: 15,
            reply_to_response_topic: true,
        });
        cfg.auto_reply_echo = "rematch".into();

        let form = FormState::from_config(&cfg);
        let mut back = cfg.clone();
        form.apply_to(&mut back);

        assert_eq!(back.host, "10.0.0.1");
        assert_eq!(back.keep_alive, 30);
        assert_eq!(back.connect_timeout, 5);
        assert_eq!(back.pub_format, "hex");
        assert_eq!(back.pub_qos, 1);
        assert_eq!(back.protocol, "5.0");
        assert_eq!(back.timed_interval, 9);
        assert_eq!(back.session_expiry, 3600);
        assert_eq!(back.receive_max, 50);
        assert_eq!(back.max_packet_size, 1_048_576);
        assert_eq!(back.topic_alias_max, 10);
        assert!(back.manual_acks);
        assert!(back.tls_keylog);
        assert_eq!(back.publish_presets.len(), 1);
        assert_eq!(back.publish_presets[0].format, "json");
        assert!(back.publish_presets[0].retain);
        assert_eq!(back.publish_presets[0].props, "source=test");
        assert_eq!(back.publish_presets[0].expiry, 30);
        assert_eq!(back.auto_reply_rules.len(), 1);
        assert_eq!(back.auto_reply_rules[0].match_topic, "in/#");
        assert_eq!(back.auto_reply_rules[0].min_interval_secs, 5);
        assert_eq!(back.auto_reply_rules[0].props, "role=echo");
        assert_eq!(back.auto_reply_rules[0].expiry, 15);
        assert!(back.auto_reply_rules[0].reply_to_response_topic);
        assert_eq!(back.auto_reply_echo, "rematch");
        assert_eq!(back.subscriptions.len(), 1);
        assert!(!back.subscriptions[0].no_local);
    }

    #[test]
    fn apply_clamps_invalid_fields() {
        let mut cfg = ConnectionConfig::new(String::new());
        cfg.timed_interval = 7;
        let mut form = FormState::from_config(&cfg);
        form.keep_alive = "0".into();
        form.connect_timeout = "999".into();
        form.timed_interval = "abc".into();
        form.pub_qos = 9;
        form.sub_qos = -1;
        form.will_qos = 3;
        form.receive_max = "0".into();
        form.max_packet_size = "99999999999".into();
        form.topic_alias_max = "abc".into();
        form.session_expiry = "".into();
        form.name = "  ".into();
        form.host = "broker".into();
        form.port = "1883".into();

        form.apply_to(&mut cfg);
        assert_eq!(cfg.keep_alive, 60);
        assert_eq!(cfg.connect_timeout, 10);
        assert_eq!(cfg.timed_interval, 7);
        assert_eq!(cfg.pub_qos, 2);
        assert_eq!(cfg.sub_qos, 0);
        assert_eq!(cfg.will_qos, 2);
        assert_eq!(cfg.receive_max, 0);
        assert_eq!(cfg.max_packet_size, 268_435_455);
        assert_eq!(cfg.topic_alias_max, 0);
        assert_eq!(cfg.session_expiry, 0);
        assert_eq!(cfg.name, "broker:1883");
    }

    #[test]
    fn presets_reorder_in_list_order() {
        let mut form = FormState::default();
        for name in ["a", "b", "c", "d"] {
            let index = form.add_preset();
            form.presets[index].name = name.into();
        }
        let names = |form: &FormState| -> Vec<String> {
            form.presets.iter().map(|p| p.name.clone()).collect()
        };

        assert!(form.move_preset(1, 1));
        assert_eq!(names(&form), ["a", "c", "b", "d"]);
        assert!(!form.move_preset(0, -1));
        assert!(!form.move_preset(3, 1));
        assert!(!form.move_preset(9, 1));

        assert!(form.move_preset_to(2, 0));
        assert_eq!(names(&form), ["b", "a", "c", "d"]);
        assert!(form.move_preset_to(1, 3));
        assert_eq!(names(&form), ["b", "c", "d", "a"]);
        assert!(!form.move_preset_to(0, 0));
        assert!(!form.move_preset_to(0, 9));
        assert!(!form.move_preset_to(9, 0));
    }

    #[test]
    fn rules_reorder_in_list_order() {
        let mut form = FormState::default();
        for name in ["a", "b", "c", "d"] {
            let index = form.add_rule();
            form.rules[index].match_topic = name.into();
        }
        let names = |form: &FormState| -> Vec<String> {
            form.rules.iter().map(|r| r.match_topic.clone()).collect()
        };

        assert!(form.move_rule(1, 1));
        assert_eq!(names(&form), ["a", "c", "b", "d"]);
        assert!(!form.move_rule(0, -1));
        assert!(!form.move_rule(3, 1));
        assert!(!form.move_rule(9, 1));

        assert!(form.move_rule_to(2, 0));
        assert_eq!(names(&form), ["b", "a", "c", "d"]);
        assert!(form.move_rule_to(1, 3));
        assert_eq!(names(&form), ["b", "c", "d", "a"]);
        assert!(!form.move_rule_to(0, 0));
        assert!(!form.move_rule_to(0, 9));
        assert!(!form.move_rule_to(9, 0));
    }

    #[test]
    fn row_add_and_remove() {
        let mut form = FormState::default();
        let i = form.add_preset();
        assert_eq!(i, 0);
        form.add_rule();
        assert_eq!(form.rules.len(), 1);
        form.remove_preset(0);
        assert!(form.presets.is_empty());
        form.remove_rule(0);
        assert!(form.rules.is_empty());
        form.remove_rule(99);
    }
}
