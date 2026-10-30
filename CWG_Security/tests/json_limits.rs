use cwg_security::{decode, parse_signature};
use serde::de::{Error as _, IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt;

#[derive(Debug, PartialEq)]
struct Envelope {
    ok: u64,
}

// Keep this isolated-crate test independent of Serde's optional derive feature.
impl<'de> Deserialize<'de> for Envelope {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EnvelopeVisitor;
        impl<'de> Visitor<'de> for EnvelopeVisitor {
            type Value = Envelope;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an object containing an unsigned ok field")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Envelope, M::Error> {
                let mut ok = None;
                while let Some(key) = map.next_key::<String>()? {
                    if key == "ok" {
                        if ok.is_some() {
                            return Err(M::Error::duplicate_field("ok"));
                        }
                        ok = Some(map.next_value::<u64>()?);
                    } else {
                        map.next_value::<IgnoredAny>()?;
                    }
                }
                ok.map(|ok| Envelope { ok })
                    .ok_or_else(|| M::Error::missing_field("ok"))
            }
        }
        deserializer.deserialize_map(EnvelopeVisitor)
    }
}

#[test]
fn security_size_guard_precedes_parser_and_exact_boundary_accepts() {
    assert_eq!(decode::<serde_json::Value>(&[0xff; 4], 3), Err("JSON_SIZE"));
    assert_eq!(decode::<serde_json::Value>(b"", 100), Err("JSON_SIZE"));
    assert_eq!(decode::<serde_json::Value>(b"{}", 0), Err("JSON_SIZE"));
    assert_eq!(
        decode::<serde_json::Value>(b"{}", 2).unwrap(),
        serde_json::json!({})
    );
    assert_eq!(decode::<serde_json::Value>(b"{} ", 2), Err("JSON_SIZE"));
    assert_eq!(
        decode::<serde_json::Value>(b"{} ", 3).unwrap(),
        serde_json::json!({})
    );
}

#[test]
fn security_duplicate_keys_rejected_at_every_extension_depth() {
    for raw in [
        br#"{"ok":1,"ok":1}"#.as_slice(),
        br#"{"ok":1,"extension":{"private_marker":1,"private_marker":2}}"#,
        br#"{"ok":1,"extension":[{"nested":{"x":1,"x":2}}]}"#,
        br#"{"ok":1,"extension":[[{"x":1,"x":1}]]}"#,
    ] {
        assert_eq!(
            decode::<Envelope>(raw, 1024),
            Err("JSON_INVALID_OR_DUPLICATE")
        );
    }
}

#[test]
fn security_duplicate_escaped_aliases_are_rejected() {
    for raw in [
        br#"{"ok":1,"\u006fk":1}"#.as_slice(),
        br#"{"ok":1,"extension":{"x":1,"\u0078":2}}"#,
        br#"{"ok":1,"extension":[{"thread_id":1,"thread_\u0069d":2}]}"#,
    ] {
        assert_eq!(
            decode::<Envelope>(raw, 1024),
            Err("JSON_INVALID_OR_DUPLICATE")
        );
    }
}

#[test]
fn security_independent_objects_and_unicode_key_spellings_are_not_global_duplicates() {
    let raw = "{\"ok\":1,\"extension\":[{\"x\":1},{\"x\":2},{\"é\":1,\"é\":2}]}";
    assert_eq!(
        decode::<Envelope>(raw.as_bytes(), 1024).unwrap(),
        Envelope { ok: 1 }
    );
    let raw = br#"{"ok":1,"extension":{"valid":"\ud83d\ude80"}}"#;
    assert_eq!(decode::<Envelope>(raw, 1024).unwrap(), Envelope { ok: 1 });
}

#[test]
fn security_nested_json_recursion_is_bounded() {
    let deep = format!("{}0{}", "[".repeat(512), "]".repeat(512));
    assert_eq!(
        decode::<serde_json::Value>(deep.as_bytes(), 2048),
        Err("JSON_INVALID_OR_DUPLICATE")
    );
    let supported = format!("{}0{}", "[".repeat(32), "]".repeat(32));
    assert!(decode::<serde_json::Value>(supported.as_bytes(), 2048).is_ok());
}

#[test]
fn security_malformed_unicode_and_trailing_documents_are_rejected() {
    for raw in [
        br#"{"ok":1,"extension":"\ud800"}"#.as_slice(),
        br#"{"ok":1,"extension":"\udc00"}"#,
        br#"{"ok":1,"extension":"\ud800\u0041"}"#,
        b"{\"ok\":1,\"extension\":\"\xff\"}",
        b"{\"ok\":1,\"extension\":\"\xc0\x80\"}",
        br#"{"ok":1}{}"#,
        b"{\"ok\":1}\0",
    ] {
        assert_eq!(
            decode::<Envelope>(raw, 1024),
            Err("JSON_INVALID_OR_DUPLICATE")
        );
    }
}

#[test]
fn security_typed_numeric_forms_are_not_coerced() {
    for raw in [
        br#"{"ok":-1}"#.as_slice(),
        br#"{"ok":1.0}"#,
        br#"{"ok":1e0}"#,
        br#"{"ok":"1"}"#,
        br#"{"ok":null}"#,
        br#"{"ok":18446744073709551616}"#,
    ] {
        assert!(decode::<Envelope>(raw, 1024).is_err());
    }
    assert_eq!(
        decode::<Envelope>(br#"{"ok":18446744073709551615}"#, 1024).unwrap(),
        Envelope { ok: u64::MAX }
    );
}

#[test]
fn security_errors_never_echo_untrusted_json() {
    let marker = "DO_NOT_LOG_UNTRUSTED_PRIVATE_BODY";
    for raw in [
        format!("{{\"ok\":\"{marker}\"}}"),
        format!("{{\"ok\":1,\"{marker}\":{{\"x\":1,\"x\":2}}}}"),
        marker.into(),
    ] {
        let error = decode::<Envelope>(raw.as_bytes(), 1024).unwrap_err();
        assert!(!error.contains(marker));
        assert!(matches!(error, "JSON_SCHEMA" | "JSON_INVALID_OR_DUPLICATE"));
    }
}

#[test]
fn security_signature_header_is_exact_lowercase_sha256() {
    let valid = format!("sha256={}", "0".repeat(64));
    assert_eq!(parse_signature(&valid), Ok([0; 32]));
    for header in [
        format!("sha256={}", "0".repeat(63)),
        format!("sha256={}", "0".repeat(65)),
        format!("sha256={}", "A".repeat(64)),
        format!("sha256={}", "g".repeat(64)),
        format!("SHA256={}", "0".repeat(64)),
        format!("{valid}\r\n"),
        format!(" {valid}"),
        format!("{valid} "),
    ] {
        assert_eq!(parse_signature(&header), Err("SIGNATURE_FORMAT"));
    }
}
