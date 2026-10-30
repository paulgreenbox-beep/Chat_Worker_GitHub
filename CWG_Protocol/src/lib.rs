//! Versioned CWG data contracts. No transport or execution permissions.
#![forbid(unsafe_code)]
use serde::de::{self, MapAccess, Visitor, value::MapAccessDeserializer};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::marker::PhantomData;

pub const MAX_BODY_BYTES: usize = 65_536;
/// Upper bound checked before the raw JSON parser allocates decoded strings.
pub const MAX_JSON_BYTES: usize = 400 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Object<UncheckedTarget>")]
pub struct ChatTarget {
    pub provider: String,
    pub thread_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Object<UncheckedSource>")]
pub struct GithubSource {
    pub repository: String,
    pub issue_number: u64,
    pub comment_id: u64,
    pub author_id: u64,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Object<UncheckedMessage>")]
pub struct Message {
    pub schema: String,
    pub message_id: String,
    pub source: GithubSource,
    pub target: ChatTarget,
    pub goal_id: String,
    pub body: String,
}

impl fmt::Debug for Message {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Message")
            .field("schema", &self.schema)
            .field("message_id", &self.message_id)
            .field("source", &self.source)
            .field("target", &self.target)
            .field("goal_id", &self.goal_id)
            .field("body_bytes", &self.body.len())
            .finish()
    }
}

/// A safe-to-log classification of failure at the bounded JSON entry point.
/// The parser's source error is deliberately not retained because it may quote input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageParseError {
    TooLarge,
    InvalidJson { line: usize, column: usize },
}

impl fmt::Display for MessageParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => formatter.write_str("CWG JSON exceeds the raw byte limit"),
            Self::InvalidJson { line, column } => {
                write!(
                    formatter,
                    "invalid CWG JSON at line {line}, column {column}"
                )
            }
        }
    }
}
impl std::error::Error for MessageParseError {}

impl Message {
    /// Decode one complete JSON object after enforcing its raw byte budget.
    /// This is a protocol parser, not an authenticated receiver or an authorization check.
    pub fn from_json_slice(raw: &[u8]) -> Result<Self, MessageParseError> {
        if raw.len() > MAX_JSON_BYTES {
            return Err(MessageParseError::TooLarge);
        }
        serde_json::from_slice(raw).map_err(|error| MessageParseError::InvalidJson {
            line: error.line(),
            column: error.column(),
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeliveryStatus {
    Pending,
    Verified,
    Delivering,
    Delivered,
    Acknowledged,
    Completed,
    Blocked,
    Unknown,
}

impl<'de> Deserialize<'de> for DeliveryStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StatusVisitor;

        impl<'de> Visitor<'de> for StatusVisitor {
            type Value = DeliveryStatus;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an uppercase CWG delivery-status string")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                match value {
                    "PENDING" => Ok(DeliveryStatus::Pending),
                    "VERIFIED" => Ok(DeliveryStatus::Verified),
                    "DELIVERING" => Ok(DeliveryStatus::Delivering),
                    "DELIVERED" => Ok(DeliveryStatus::Delivered),
                    "ACKNOWLEDGED" => Ok(DeliveryStatus::Acknowledged),
                    "COMPLETED" => Ok(DeliveryStatus::Completed),
                    "BLOCKED" => Ok(DeliveryStatus::Blocked),
                    "UNKNOWN" => Ok(DeliveryStatus::Unknown),
                    _ => Err(E::custom("unknown CWG delivery status")),
                }
            }
        }

        // A unit enum's derived decoder also accepts {"UNKNOWN":null}. The
        // protocol uses string values only; do not request enum/map access.
        deserializer.deserialize_str(StatusVisitor)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InvalidMessage {
    Schema,
    Identity,
    Size,
    Format,
}

impl fmt::Display for InvalidMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Schema => "unsupported CWG schema",
            Self::Identity => "numeric identity must be nonzero",
            Self::Size => "CWG field size outside allowed bounds",
            Self::Format => "invalid CWG identity format",
        })
    }
}
impl std::error::Error for InvalidMessage {}

impl ChatTarget {
    pub fn validate(&self) -> Result<(), InvalidMessage> {
        if self.provider.is_empty()
            || self.provider.len() > 32
            || self.thread_id.is_empty()
            || self.thread_id.len() > 128
        {
            return Err(InvalidMessage::Size);
        }
        if !self.provider.as_bytes()[0].is_ascii_lowercase()
            || !self
                .provider
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_".contains(&b))
            || !self
                .thread_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_:.".contains(&b))
        {
            return Err(InvalidMessage::Format);
        }
        Ok(())
    }
}

impl GithubSource {
    pub fn validate(&self) -> Result<(), InvalidMessage> {
        if self.issue_number == 0 || self.comment_id == 0 || self.author_id == 0 {
            return Err(InvalidMessage::Identity);
        }
        validate_repository(&self.repository)
    }
}

fn validate_repository(repository: &str) -> Result<(), InvalidMessage> {
    let valid = repository.split_once('/').is_some_and(|(owner, name)| {
        !owner.is_empty()
            && owner.len() <= 39
            && !owner.starts_with('-')
            && !owner.ends_with('-')
            && !owner.contains("--")
            && owner
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && !name.is_empty()
            && name.len() <= 100
            && name != "."
            && name != ".."
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    });
    if valid {
        Ok(())
    } else {
        Err(InvalidMessage::Format)
    }
}

/// Structural validation only; this does not authenticate or approve a binding.
pub fn validate_binding(
    repository: &str,
    goal_id: &str,
    target: &ChatTarget,
) -> Result<(), InvalidMessage> {
    validate_repository(repository)?;
    let suffix = goal_id
        .strip_prefix("CWG-")
        .or_else(|| goal_id.strip_prefix("MSS-"))
        .ok_or(InvalidMessage::Format)?;
    if suffix.is_empty()
        || suffix.len() > 32
        || !suffix.bytes().all(|b| b.is_ascii_digit())
        || !suffix.bytes().any(|b| b != b'0')
    {
        return Err(InvalidMessage::Format);
    }
    target.validate()
}

pub fn validate(message: &Message) -> Result<(), InvalidMessage> {
    if message.schema != "cwg-message-v1" {
        return Err(InvalidMessage::Schema);
    }
    if message.message_id.len() > 128
        || message.message_id.is_empty()
        || message.body.is_empty()
        || message.body.len() > MAX_BODY_BYTES
    {
        return Err(InvalidMessage::Size);
    }
    if !message
        .message_id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"-_:".contains(&b))
    {
        return Err(InvalidMessage::Format);
    }
    message.source.validate()?;
    validate_binding(
        &message.source.repository,
        &message.goal_id,
        &message.target,
    )
}

// Serde's default struct decoder also accepts positional sequences. CWG wire
// models require named objects, so only map access reaches the derived strict
// field visitors. MapAccessDeserializer preserves duplicate-field detection.
struct Object<T>(T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Object<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor<T>(PhantomData<T>);

        impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
            type Value = Object<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a named CWG JSON object")
            }

            fn visit_map<A: MapAccess<'de>>(self, access: A) -> Result<Self::Value, A::Error> {
                T::deserialize(MapAccessDeserializer::new(access)).map(Object)
            }
        }

        deserializer.deserialize_map(ObjectVisitor(PhantomData))
    }
}

// Check decoded byte lengths before this visitor copies borrowed/scratch strings.
// A generic parser may allocate escaped-string scratch before visiting; only
// from_json_slice enforces the overall raw byte budget before parsing begins.
fn bounded_string<'de, const MAX: usize, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<String, D::Error> {
    struct BoundedStringVisitor<const MAX: usize>;

    impl<'de, const MAX: usize> Visitor<'de> for BoundedStringVisitor<MAX> {
        type Value = String;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "a string of at most {MAX} UTF-8 bytes")
        }

        fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
            if value.len() > MAX {
                return Err(E::custom("CWG string exceeds its decoded byte limit"));
            }
            Ok(value.to_owned())
        }

        fn visit_borrowed_str<E: de::Error>(self, value: &'de str) -> Result<Self::Value, E> {
            self.visit_str(value)
        }

        fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
            if value.len() > MAX {
                return Err(E::custom("CWG string exceeds its decoded byte limit"));
            }
            Ok(value)
        }
    }

    deserializer.deserialize_str(BoundedStringVisitor::<MAX>)
}

// Keep wire decoding strict and validate before exposing a public model.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UncheckedTarget {
    #[serde(deserialize_with = "bounded_string::<32, _>")]
    provider: String,
    #[serde(deserialize_with = "bounded_string::<128, _>")]
    thread_id: String,
}
impl TryFrom<Object<UncheckedTarget>> for ChatTarget {
    type Error = InvalidMessage;
    fn try_from(Object(raw): Object<UncheckedTarget>) -> Result<Self, Self::Error> {
        let target = Self {
            provider: raw.provider,
            thread_id: raw.thread_id,
        };
        target.validate()?;
        Ok(target)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UncheckedSource {
    #[serde(deserialize_with = "bounded_string::<140, _>")]
    repository: String,
    issue_number: u64,
    comment_id: u64,
    author_id: u64,
}
impl TryFrom<Object<UncheckedSource>> for GithubSource {
    type Error = InvalidMessage;
    fn try_from(Object(raw): Object<UncheckedSource>) -> Result<Self, Self::Error> {
        let source = Self {
            repository: raw.repository,
            issue_number: raw.issue_number,
            comment_id: raw.comment_id,
            author_id: raw.author_id,
        };
        source.validate()?;
        Ok(source)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UncheckedMessage {
    #[serde(deserialize_with = "bounded_string::<14, _>")]
    schema: String,
    #[serde(deserialize_with = "bounded_string::<128, _>")]
    message_id: String,
    source: GithubSource,
    target: ChatTarget,
    #[serde(deserialize_with = "bounded_string::<36, _>")]
    goal_id: String,
    #[serde(deserialize_with = "bounded_string::<MAX_BODY_BYTES, _>")]
    body: String,
}
impl TryFrom<Object<UncheckedMessage>> for Message {
    type Error = InvalidMessage;
    fn try_from(Object(raw): Object<UncheckedMessage>) -> Result<Self, Self::Error> {
        let message = Self {
            schema: raw.schema,
            message_id: raw.message_id,
            source: raw.source,
            target: raw.target,
            goal_id: raw.goal_id,
            body: raw.body,
        };
        validate(&message)?;
        Ok(message)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_empty_body() {
        let m = Message {
            schema: "cwg-message-v1".into(),
            message_id: "m-1".into(),
            source: GithubSource {
                repository: "owner/repo".into(),
                issue_number: 1,
                comment_id: 2,
                author_id: 3,
            },
            target: ChatTarget {
                provider: "codex".into(),
                thread_id: "abc-123".into(),
            },
            goal_id: "CWG-001".into(),
            body: "".into(),
        };
        assert_eq!(validate(&m), Err(InvalidMessage::Size));
        let mut good = m.clone();
        good.body = "Task data, not authority".into();
        assert!(validate(&good).is_ok());
    }
}
