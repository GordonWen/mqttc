//! Payload codecs shared by publish, log rendering and auto-reply.

use base64::Engine;

use crate::i18n;
use crate::model::PayloadFormat;

/// Encode the form text into wire bytes. JSON is validated and sent verbatim.
pub fn encode_payload(text: &str, format: PayloadFormat) -> Result<Vec<u8>, String> {
    match format {
        PayloadFormat::Text => Ok(text.as_bytes().to_vec()),
        PayloadFormat::Json => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Ok(Vec::new());
            }
            serde_json::from_str::<serde_json::Value>(trimmed).map_err(|err| {
                i18n::t(
                    &format!("JSON 格式无效: {err}"),
                    &format!("Invalid JSON: {err}"),
                )
            })?;
            Ok(text.as_bytes().to_vec())
        }
        PayloadFormat::Base64 => {
            let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
            base64::engine::general_purpose::STANDARD
                .decode(cleaned)
                .map_err(|err| {
                    i18n::t(
                        &format!("Base64 解码失败: {err}"),
                        &format!("Base64 decode failed: {err}"),
                    )
                })
        }
        PayloadFormat::Hex => decode_hex(text),
    }
}

/// Raw bytes to display text. RECV/SEND use this; SYS/ERR are already UTF-8.
pub fn render_payload(bytes: &[u8], format: PayloadFormat) -> String {
    match format {
        PayloadFormat::Text => String::from_utf8_lossy(bytes).into_owned(),
        PayloadFormat::Json => match serde_json::from_slice::<serde_json::Value>(bytes) {
            Ok(value) => serde_json::to_string_pretty(&value)
                .unwrap_or_else(|_| String::from_utf8_lossy(bytes).into_owned()),
            Err(_) => String::from_utf8_lossy(bytes).into_owned(),
        },
        PayloadFormat::Base64 => base64::engine::general_purpose::STANDARD.encode(bytes),
        PayloadFormat::Hex => encode_hex(bytes),
    }
}

/// Pretty-print JSON in the payload box. Does not send anything.
pub fn beautify_json(text: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(text.trim()).map_err(|err| {
        i18n::t(
            &format!("JSON 格式无效: {err}"),
            &format!("Invalid JSON: {err}"),
        )
    })?;
    serde_json::to_string_pretty(&value).map_err(|err| {
        i18n::t(
            &format!("JSON 格式化失败: {err}"),
            &format!("JSON format failed: {err}"),
        )
    })
}

pub fn encode_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02X}"));
    }
    out
}

/// Accept `00 1A FF`, `00:1A:FF`, and `00-1A-FF` as the same payload.
pub fn decode_hex(text: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',' && *c != ':' && *c != '-')
        .collect();
    if cleaned.is_empty() {
        return Ok(Vec::new());
    }
    let chars: Vec<char> = cleaned.chars().collect();
    if chars.len() % 2 != 0 {
        return Err(i18n::t("Hex 长度必须为偶数", "Hex length must be even"));
    }
    let mut out = Vec::with_capacity(chars.len() / 2);
    for pair in chars.chunks(2) {
        let token: String = pair.iter().collect();
        let byte = u8::from_str_radix(&token, 16).map_err(|_| {
            i18n::t(
                &format!("Hex 字符无效: {token}"),
                &format!("Invalid hex character: {token}"),
            )
        })?;
        out.push(byte);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_payload_is_verbatim() {
        let payload = encode_payload("line1\nline2", PayloadFormat::Text).unwrap();
        assert_eq!(payload, b"line1\nline2");
    }

    #[test]
    fn json_payload_is_validated() {
        assert!(encode_payload("{\"a\":1}", PayloadFormat::Json).is_ok());
        assert!(encode_payload("{oops", PayloadFormat::Json).is_err());
        assert!(encode_payload("   ", PayloadFormat::Json).unwrap().is_empty());
    }

    #[test]
    fn json_can_be_beautified() {
        let pretty = beautify_json("{\"a\":[1,2]}").unwrap();
        assert!(pretty.contains('\n'));
        assert!(pretty.contains("\"a\""));
        assert!(beautify_json("nope").is_err());
    }

    #[test]
    fn base64_payload_round_trip() {
        let bytes = encode_payload("aGVsbG8=", PayloadFormat::Base64).unwrap();
        assert_eq!(bytes, b"hello");
        assert_eq!(render_payload(&bytes, PayloadFormat::Base64), "aGVsbG8=");
        assert!(encode_payload("!!!", PayloadFormat::Base64).is_err());
    }

    #[test]
    fn hex_payload_round_trip() {
        let bytes = vec![0x00, 0x1a, 0xff];
        assert_eq!(render_payload(&bytes, PayloadFormat::Hex), "001AFF");
        assert_eq!(decode_hex("001AFF").unwrap(), bytes);
        assert_eq!(decode_hex("00 1A FF").unwrap(), bytes);
        assert_eq!(decode_hex("00:1A:FF").unwrap(), bytes);
        assert_eq!(decode_hex("00-1a-ff").unwrap(), bytes);
        assert!(decode_hex("  ").unwrap().is_empty());
    }

    #[test]
    fn hex_rejects_invalid_input() {
        assert!(decode_hex("ABC").is_err());
        assert!(decode_hex("ZZ").is_err());
    }

    #[test]
    fn json_render_falls_back_on_plain_text() {
        assert_eq!(render_payload(b"not json", PayloadFormat::Json), "not json");
        assert_eq!(
            render_payload(b"{\"a\":1}", PayloadFormat::Json),
            "{\n  \"a\": 1\n}"
        );
    }
}
