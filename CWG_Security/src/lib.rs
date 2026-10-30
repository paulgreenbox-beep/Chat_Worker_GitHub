//! Bounded, duplicate-preserving JSON validation and Windows CNG webhook HMAC.
//! HMAC authenticates bytes, not the GitHub author or a chat binding.
use serde::de::{DeserializeOwned, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::{collections::BTreeSet, fmt};

pub mod windows_acl;

pub type Result<T> = std::result::Result<T, &'static str>;

/// Validate every object (including ignored GitHub extension fields) before
/// decoding a typed model. Never round-trip through Value and lose duplicates.
pub fn decode<T: DeserializeOwned>(bytes: &[u8], limit: usize) -> Result<T> {
    if bytes.is_empty() || bytes.len() > limit {
        return Err("JSON_SIZE");
    }
    serde_json::from_slice::<Unique>(bytes).map_err(|_| "JSON_INVALID_OR_DUPLICATE")?;
    serde_json::from_slice(bytes).map_err(|_| "JSON_SCHEMA")
}

struct Unique;
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct Walk;
        impl<'de> Visitor<'de> for Walk {
            type Value = Unique;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("JSON with unique object keys")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> std::result::Result<Unique, M::Error> {
                let mut keys = BTreeSet::new();
                while let Some(key) = map.next_key::<String>()? {
                    if !keys.insert(key) {
                        return Err(serde::de::Error::custom("duplicate JSON key"));
                    }
                    map.next_value::<Unique>()?;
                }
                Ok(Unique)
            }
            fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> std::result::Result<Unique, S::Error> {
                while seq.next_element::<Unique>()?.is_some() {}
                Ok(Unique)
            }
            fn visit_bool<E: serde::de::Error>(self, _: bool) -> std::result::Result<Unique, E> { Ok(Unique) }
            fn visit_i64<E: serde::de::Error>(self, _: i64) -> std::result::Result<Unique, E> { Ok(Unique) }
            fn visit_u64<E: serde::de::Error>(self, _: u64) -> std::result::Result<Unique, E> { Ok(Unique) }
            fn visit_f64<E: serde::de::Error>(self, _: f64) -> std::result::Result<Unique, E> { Ok(Unique) }
            fn visit_str<E: serde::de::Error>(self, _: &str) -> std::result::Result<Unique, E> { Ok(Unique) }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Unique, E> { Ok(Unique) }
        }
        d.deserialize_any(Walk)
    }
}

pub fn parse_signature(header: &str) -> Result<[u8; 32]> {
    let hex = header.strip_prefix("sha256=").ok_or("SIGNATURE_FORMAT")?;
    if hex.len() != 64 { return Err("SIGNATURE_FORMAT"); }
    let mut out = [0; 32];
    for (i, pair) in hex.as_bytes().chunks_exact(2).enumerate() {
        let digit = |b| match b {
            b'0'..=b'9' => Ok(b - b'0'),
            b'a'..=b'f' => Ok(b - b'a' + 10),
            _ => Err("SIGNATURE_FORMAT"),
        };
        out[i] = (digit(pair[0])? << 4) | digit(pair[1])?;
    }
    Ok(out)
}

/// Fixed work for a fixed-length MAC: volatile loads prevent folding the loop
/// into an early-exit memcmp. This is not a general-purpose crypto library.
fn same_mac(left: &[u8; 32], right: &[u8; 32]) -> bool {
    let mut diff = 0u8;
    for i in 0..32 {
        // SAFETY: i is bounded by both arrays; reads do not outlive them.
        diff |= unsafe { std::ptr::read_volatile(&left[i]) ^ std::ptr::read_volatile(&right[i]) };
    }
    diff == 0
}

pub fn verify_signature(secret: &[u8], bytes: &[u8], header: &str) -> Result<()> {
    if secret.len() < 16 || secret.len() > 1024 { return Err("SECRET_SIZE"); }
    let supplied = parse_signature(header)?;
    let expected = hmac(secret, bytes)?;
    if same_mac(&supplied, &expected) { Ok(()) } else { Err("SIGNATURE_MISMATCH") }
}

#[cfg(not(windows))]
fn hmac(_: &[u8], _: &[u8]) -> Result<[u8; 32]> { Err("WINDOWS_CNG_REQUIRED") }

#[cfg(windows)]
mod windows_cng;

#[cfg(windows)]
fn hmac(secret: &[u8], bytes: &[u8]) -> Result<[u8; 32]> {
    windows_cng::hmac(secret, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn duplicates_in_ignored_nested_fields_are_rejected() {
        let raw = br#"{"ok":1,"extension":{"a":1,"a":2}}"#;
        assert!(decode::<serde_json::Value>(raw, 100).is_err());
    }
    #[test] fn bounds_and_trailing_json_are_rejected() {
        assert!(decode::<serde_json::Value>(b"{}", 1).is_err());
        assert!(decode::<serde_json::Value>(b"{}{}", 100).is_err());
        assert!(decode::<serde_json::Value>(br#"{"x":"\ud800"}"#, 100).is_err());
    }
    #[test] fn independent_objects_can_reuse_keys() {
        assert!(decode::<serde_json::Value>(br#"[{"x":1},{"x":2},null,true]"#, 100).is_ok());
    }
    #[test] fn malformed_mac_rejected() {
        for h in ["", "sha1=abc", "sha256=1", "sha256=zz"] { assert!(parse_signature(h).is_err()); }
    }
    #[test] fn comparison_checks_all_positions() {
        let a = [0; 32];
        assert!(same_mac(&a, &a));
        for i in 0..32 { let mut b = a; b[i] = 1; assert!(!same_mac(&a, &b)); }
    }
    #[cfg(windows)]
    #[test] fn github_official_hmac_vector_and_tamper() {
        let mac = "sha256=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17";
        assert!(verify_signature(b"It's a Secret to Everybody", b"Hello, World!", mac).is_ok());
        assert!(verify_signature(b"It's a Secret to Everybody", b"Hello, world!", mac).is_err());
    }
}
