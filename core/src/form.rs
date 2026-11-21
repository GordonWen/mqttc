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

/// One publish-preset row bound to the presets page.
#[derive(Clone, Debug)]
pub struct PresetRow {
    pub name: String,
    pub topic: String,
    pub payload: String,
    pub qos: i32,
    pub retain: bool,
    pub format: i32,
    pub pinned: bool,
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
            pinned: false,
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
        preset.pinned = self.pinned;
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
                pinned: preset.pinned,
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

    /// Publish order: pinned presets first, each group in edit-list order.
    ///
    /// Returns original indices so callers can still publish by index while
    /// the presets page keeps its stable editing order.
    pub fn preset_display_order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.presets.len()).collect();
        order.sort_by_key(|&index| !self.presets[index].pinned);
        order
    }

    /// Flip the pin flag of one preset. Returns false for a stale index.
    pub fn toggle_preset_pin(&mut self, index: usize) -> bool {
        match self.presets.get_mut(index) {
            Some(preset) => {
                preset.pinned = !preset.pinned;
                true
            }
            None => false,
        }
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
            pinned: true,
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
        assert_eq!(back.publish_presets.len(), 1);
        assert_eq!(back.publish_presets[0].format, "json");
        assert!(back.publish_presets[0].retain);
        assert!(back.publish_presets[0].pinned);
        assert_eq!(back.auto_reply_rules.len(), 1);
        assert_eq!(back.auto_reply_rules[0].match_topic, "in/#");
        assert_eq!(back.auto_reply_rules[0].min_interval_secs, 5);
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
        assert_eq!(cfg.name, "broker:1883");
    }

    #[test]
    fn pinned_presets_sort_first() {
        let mut form = FormState::default();
        form.add_preset();
        form.add_preset();
        form.add_preset();
        form.presets[1].pinned = true;
        form.presets[2].pinned = true;
        assert_eq!(form.preset_display_order(), vec![1, 2, 0]);

        assert!(form.toggle_preset_pin(1));
        assert_eq!(form.preset_display_order(), vec![2, 0, 1]);
        assert!(!form.toggle_preset_pin(9));
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
