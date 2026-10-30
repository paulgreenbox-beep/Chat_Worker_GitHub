use cwg_protocol::{
    ChatTarget, GithubSource, MAX_BODY_BYTES, MAX_JSON_BYTES, Message, MessageParseError,
};
use serde_json::json;
use std::error::Error;

fn message() -> Message {
    Message {
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
            thread_id: "thread-1".into(),
        },
        goal_id: "CWG-001".into(),
        body: "Task data".into(),
    }
}

#[test]
fn raw_budget_is_checked_before_json_parsing() {
    let raw = vec![0xff; MAX_JSON_BYTES + 1];
    assert_eq!(
        Message::from_json_slice(&raw),
        Err(MessageParseError::TooLarge)
    );
}

#[test]
fn exact_raw_budget_accepts_valid_json_and_one_extra_byte_is_refused() {
    let m = message();
    let raw = serde_json::to_vec(&m).unwrap();
    let mut padded = vec![b' '; MAX_JSON_BYTES - raw.len()];
    padded.extend_from_slice(&raw);
    assert_eq!(padded.len(), MAX_JSON_BYTES);
    assert_eq!(Message::from_json_slice(&padded).unwrap(), m);
    padded.push(b' ');
    assert_eq!(
        Message::from_json_slice(&padded),
        Err(MessageParseError::TooLarge)
    );
}

#[test]
fn escaped_maximum_body_fits_raw_budget_and_round_trips() {
    let mut m = message();
    m.body = "\u{1}".repeat(MAX_BODY_BYTES);
    m.message_id = "m".repeat(128);
    m.source.repository = format!("{}/{}", "o".repeat(39), "r".repeat(100));
    m.source.issue_number = u64::MAX;
    m.source.comment_id = u64::MAX;
    m.source.author_id = u64::MAX;
    m.target.provider = "p".repeat(32);
    m.target.thread_id = "t".repeat(128);
    m.goal_id = format!("CWG-{}", "1".repeat(32));
    let raw = serde_json::to_vec(&m).unwrap();
    assert!(raw.len() > MAX_BODY_BYTES * 6);
    assert!(raw.len() <= MAX_JSON_BYTES);
    assert_eq!(Message::from_json_slice(&raw).unwrap(), m);
}

#[test]
fn escaped_decoded_body_limit_is_enforced_within_raw_budget() {
    let raw = serde_json::to_string(&message()).unwrap();
    let raw = raw.replace("Task data", &r"\u0001".repeat(MAX_BODY_BYTES + 1));
    assert!(raw.len() <= MAX_JSON_BYTES);
    assert!(matches!(
        Message::from_json_slice(raw.as_bytes()),
        Err(MessageParseError::InvalidJson { .. })
    ));
    assert!(serde_json::from_str::<Message>(&raw).is_err());
}

#[test]
fn every_wire_string_has_a_decoded_byte_limit() {
    let valid = serde_json::to_value(message()).unwrap();
    for (field, maximum) in [
        ("schema", 14),
        ("message_id", 128),
        ("goal_id", 36),
        ("body", MAX_BODY_BYTES),
    ] {
        let mut value = valid.clone();
        value[field] = json!("x".repeat(maximum + 1));
        let raw = serde_json::to_vec(&value).unwrap();
        assert!(serde_json::from_slice::<Message>(&raw).is_err(), "{field}");
        assert!(
            matches!(
                Message::from_json_slice(&raw),
                Err(MessageParseError::InvalidJson { .. })
            ),
            "{field}"
        );
    }
    for (object, field, maximum) in [
        ("source", "repository", 140),
        ("target", "provider", 32),
        ("target", "thread_id", 128),
    ] {
        let mut value = valid.clone();
        value[object][field] = json!("x".repeat(maximum + 1));
        let raw = serde_json::to_vec(&value).unwrap();
        assert!(
            serde_json::from_slice::<Message>(&raw).is_err(),
            "{object}.{field}"
        );
        assert!(
            matches!(
                Message::from_json_slice(&raw),
                Err(MessageParseError::InvalidJson { .. })
            ),
            "{object}.{field}"
        );
    }
    let mut value = valid;
    value["body"] = json!("🚀".repeat(MAX_BODY_BYTES / 4 + 1));
    assert!(Message::from_json_slice(&serde_json::to_vec(&value).unwrap()).is_err());
}

#[test]
fn raw_parser_errors_report_location_without_echoing_input() {
    let marker = "DO_NOT_LOG_PRIVATE_MESSAGE_CONTENT";
    let mut unknown = serde_json::to_value(message()).unwrap();
    unknown[marker] = json!(marker);
    for raw in [
        serde_json::to_vec(&unknown).unwrap(),
        serde_json::to_vec(marker).unwrap(),
        b"{\n\"body\":\"DO_NOT_LOG_PRIVATE_MESSAGE_CONTENT".to_vec(),
    ] {
        let error = Message::from_json_slice(&raw).unwrap_err();
        let MessageParseError::InvalidJson { line, column } = error else {
            panic!("unexpected size classification")
        };
        assert!(line >= 1);
        assert!(column >= 1);
        assert!(!format!("{error:?}").contains(marker));
        assert!(!error.to_string().contains(marker));
        assert!(error.source().is_none());
    }
    let oversized = MessageParseError::TooLarge;
    assert!(!oversized.to_string().contains(marker));
    assert!(oversized.source().is_none());
}

#[test]
fn raw_parser_keeps_strict_shape_duplicate_keys_and_complete_document_rules() {
    let raw = serde_json::to_string(&message()).unwrap();
    for invalid in [
        raw.replace(
            "\"message_id\":\"m-1\"",
            "\"message_id\":\"m-1\",\"message_id\":\"m-1\"",
        ),
        format!("{raw}{{}}"),
        r#"["cwg-message-v1","m-1",["owner/repo",1,2,3],["codex","thread-1"],"CWG-001","body"]"#
            .into(),
    ] {
        assert!(matches!(
            Message::from_json_slice(invalid.as_bytes()),
            Err(MessageParseError::InvalidJson { .. })
        ));
    }
}

#[test]
fn escaped_field_names_cannot_hide_duplicate_keys() {
    let raw = serde_json::to_string(&message()).unwrap();
    for (field, duplicate) in [
        (
            r#""schema":"cwg-message-v1""#,
            r#""schema":"cwg-message-v1","\u0073chema":"cwg-message-v1""#,
        ),
        (
            r#""thread_id":"thread-1""#,
            r#""thread_id":"thread-1","thread_\u0069d":"thread-1""#,
        ),
    ] {
        let invalid = raw.replacen(field, duplicate, 1);
        assert_ne!(invalid, raw);
        assert!(serde_json::from_str::<Message>(&invalid).is_err());
        assert!(matches!(
            Message::from_json_slice(invalid.as_bytes()),
            Err(MessageParseError::InvalidJson { .. })
        ));
    }
}

#[test]
fn raw_parser_rejects_invalid_unicode_without_quoting_it() {
    let mut raw = serde_json::to_vec(&message()).unwrap();
    let index = raw
        .windows(9)
        .position(|window| window == b"Task data")
        .unwrap();
    raw[index] = 0xff;
    assert!(matches!(
        Message::from_json_slice(&raw),
        Err(MessageParseError::InvalidJson { .. })
    ));
    let raw = serde_json::to_string(&message())
        .unwrap()
        .replace("Task data", r"\uD800");
    assert!(matches!(
        Message::from_json_slice(raw.as_bytes()),
        Err(MessageParseError::InvalidJson { .. })
    ));
}
