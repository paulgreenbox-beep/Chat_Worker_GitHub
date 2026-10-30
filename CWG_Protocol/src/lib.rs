//! Versioned CWG data contracts. No transport or execution permissions.
#![forbid(unsafe_code)]
use serde::{Deserialize, Serialize};
use std::fmt;

pub const MAX_BODY_BYTES: usize = 65_536;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "UncheckedTarget")]
pub struct ChatTarget {
    pub provider: String,
    pub thread_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "UncheckedSource")]
pub struct GithubSource {
    pub repository: String,
    pub issue_number: u64,
    pub comment_id: u64,
    pub author_id: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "UncheckedMessage")]
pub struct Message {
    pub schema: String,
    pub message_id: String,
    pub source: GithubSource,
    pub target: ChatTarget,
    pub goal_id: String,
    pub body: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

// Keep wire decoding strict and validate before exposing a public model.
// Parsing directly from raw JSON also preserves duplicate-field detection.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UncheckedTarget {
    provider: String,
    thread_id: String,
}
impl TryFrom<UncheckedTarget> for ChatTarget {
    type Error = InvalidMessage;
    fn try_from(raw: UncheckedTarget) -> Result<Self, Self::Error> {
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
    repository: String,
    issue_number: u64,
    comment_id: u64,
    author_id: u64,
}
impl TryFrom<UncheckedSource> for GithubSource {
    type Error = InvalidMessage;
    fn try_from(raw: UncheckedSource) -> Result<Self, Self::Error> {
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
    schema: String,
    message_id: String,
    source: GithubSource,
    target: ChatTarget,
    goal_id: String,
    body: String,
}
impl TryFrom<UncheckedMessage> for Message {
    type Error = InvalidMessage;
    fn try_from(raw: UncheckedMessage) -> Result<Self, Self::Error> {
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
