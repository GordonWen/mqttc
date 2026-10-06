//! Process-wide zh/en switch. MQTT worker threads call [`t`] for ConnAck errors,
//! so the current language is an `AtomicU8` instead of UI state.

use std::sync::atomic::{AtomicU8, Ordering};

static LANG: AtomicU8 = AtomicU8::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    pub fn from_index(index: i32) -> Self {
        if index == 1 {
            Self::En
        } else {
            Self::Zh
        }
    }

    pub fn from_key(key: &str) -> Self {
        if key.eq_ignore_ascii_case("en") {
            Self::En
        } else {
            Self::Zh
        }
    }

    pub fn to_index(self) -> i32 {
        match self {
            Self::En => 1,
            Self::Zh => 0,
        }
    }

    pub fn to_key(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Zh => "zh",
        }
    }
}

pub fn set_lang(lang: Lang) {
    LANG.store(lang.to_index() as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    Lang::from_index(LANG.load(Ordering::Relaxed) as i32)
}

pub fn is_en() -> bool {
    current() == Lang::En
}

/// Pick zh or en. Callers pass both literals so MQTT worker threads need no UI handle.
pub fn t(zh: &str, en: &str) -> String {
    if is_en() {
        en.to_string()
    } else {
        zh.to_string()
    }
}

pub fn local_broker_name() -> String {
    t("本地 Broker", "Local Broker")
}

pub fn new_connection_name(n: usize) -> String {
    if is_en() {
        format!("Connection {n}")
    } else {
        format!("连接 {n}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_index_and_key_round_trip() {
        assert_eq!(Lang::from_index(Lang::Zh.to_index()), Lang::Zh);
        assert_eq!(Lang::from_index(Lang::En.to_index()), Lang::En);
        assert_eq!(Lang::from_key(Lang::Zh.to_key()), Lang::Zh);
        assert_eq!(Lang::from_key(Lang::En.to_key()), Lang::En);
        assert_eq!(Lang::from_key("EN"), Lang::En);
        assert_eq!(Lang::from_key("unknown"), Lang::Zh);
        assert_eq!(Lang::from_index(99), Lang::Zh);
    }
}
