//! Versioned CWG data contracts. No transport or execution permissions.
#![forbid(unsafe_code)]
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatTarget {
    pub provider: String,
    pub thread_id: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GithubSource {
    pub repository: String,
    pub issue_number: u64,
    pub comment_id: u64,
    pub author_id: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
    Pending, Verified, Delivering, Delivered, Acknowledged, Completed, Blocked, Unknown,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InvalidMessage {
    Schema, Identity, Size, Format,
}
pub fn validate(message: &Message) -> Result<(), InvalidMessage> {
    if message.schema != "cwg-message-v1" { return Err(InvalidMessage::Schema); }
    if message.message_id.len() > 128 || message.message_id.is_empty()
        || message.source.issue_number == 0 || message.source.comment_id == 0 || message.source.author_id == 0
        || message.target.provider.is_empty() || message.target.provider.len() > 32
        || message.target.thread_id.is_empty() || message.target.thread_id.len() > 128
        || message.body.is_empty() || message.body.len() > 65536 {
        return Err(InvalidMessage::Size);
    }
    if !message.message_id.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_:".contains(&b))
        || !message.target.thread_id.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_:.".contains(&b))
        || !message.goal_id.starts_with("MSS-") && !message.goal_id.starts_with("CWG-")
        || !message.source.repository.split_once('/').is_some_and(|(owner, name)| {
            !owner.is_empty() && !name.is_empty() && !name.contains('/')
                && message.source.repository.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_./".contains(&b))
        }) {
        return Err(InvalidMessage::Format);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_empty_body() {
        let m = Message { schema:"cwg-message-v1".into(), message_id:"m-1".into(),
            source:GithubSource{repository:"owner/repo".into(), issue_number:1, comment_id:2, author_id:3},
            target:ChatTarget{provider:"codex".into(),thread_id:"abc-123".into()},
            goal_id:"CWG-001".into(),body:"".into() };
        assert_eq!(validate(&m),Err(InvalidMessage::Size));
        let mut good=m.clone();
        good.body="Task data, not authority".into();
        assert!(validate(&good).is_ok());
    }
}
