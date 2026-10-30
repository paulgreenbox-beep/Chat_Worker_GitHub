use cwg_protocol::{ChatTarget, DeliveryStatus, GithubSource, InvalidMessage, Message, validate};
use serde_json::{Value, json};

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
        body: "Check source".into(),
    }
}

fn rejects(m: Message) {
    assert!(validate(&m).is_err(), "validate accepted {m:?}");
    let raw = serde_json::to_vec(&m).unwrap();
    assert!(
        serde_json::from_slice::<Message>(&raw).is_err(),
        "Deserialize bypassed validation"
    );
}

#[test]
fn rejects_wrong_schema_during_deserialization() {
    let mut m = message();
    m.schema = "cwg-message-v2".into();
    rejects(m);
}

#[test]
fn rejects_empty_message_identifier() {
    let mut m = message();
    m.message_id.clear();
    rejects(m);
}

#[test]
fn rejects_invalid_repository_names() {
    for name in [
        "",
        "owner",
        "/repo",
        "owner/",
        "owner/repo/extra",
        "../repo",
        "owner/..",
        "-owner/repo",
        "owner-/repo",
        "own--er/repo",
        "owner/re po",
        "ownér/repo",
    ] {
        let mut m = message();
        m.source.repository = name.into();
        rejects(m);
    }
    for name in [
        format!("{}/repo", "a".repeat(40)),
        format!("owner/{}", "r".repeat(101)),
    ] {
        let mut m = message();
        m.source.repository = name;
        rejects(m);
    }
}

#[test]
fn rejects_empty_thread_id() {
    let mut m = message();
    m.target.thread_id.clear();
    rejects(m);
}

#[test]
fn rejects_malformed_goal_identifiers() {
    for goal in [
        "",
        "CWG-",
        "MSS-",
        "CWG-anything",
        "CWG-1/2",
        "CWG-001\n",
        "CWG-００１",
        "CWG-000",
        "cwg-001",
        "OTHER-001",
    ] {
        let mut m = message();
        m.goal_id = goal.into();
        rejects(m);
    }
    let mut m = message();
    m.goal_id = format!("CWG-{}", "1".repeat(33));
    rejects(m);
}

#[test]
fn rejects_oversized_body_in_bytes() {
    let mut m = message();
    m.body = "x".repeat(65_537);
    rejects(m);
    let mut m = message();
    m.body = "🚀".repeat(16_385);
    rejects(m);
}

#[test]
fn rejects_unknown_json_fields_at_every_level() {
    for field in ["verified", "source", "target"] {
        let mut value = serde_json::to_value(message()).unwrap();
        if field == "verified" {
            value[field] = json!(true);
        } else {
            value[field]["trusted"] = json!(true);
        }
        assert!(serde_json::from_str::<Message>(&value.to_string()).is_err());
    }
}

#[test]
fn rejects_all_duplicate_json_keys_even_identical_values() {
    let raw = serde_json::to_string(&message()).unwrap();
    for (field, value) in [
        ("schema", "\"cwg-message-v1\""),
        ("message_id", "\"m-1\""),
        ("repository", "\"owner/repo\""),
        ("issue_number", "1"),
        ("comment_id", "2"),
        ("author_id", "3"),
        ("provider", "\"codex\""),
        ("thread_id", "\"thread-1\""),
        ("goal_id", "\"CWG-001\""),
        ("body", "\"Check source\""),
    ] {
        let item = format!("\"{field}\":{value}");
        let duplicate = raw.replacen(&item, &format!("{item},{item}"), 1);
        assert_ne!(duplicate, raw);
        assert!(
            serde_json::from_str::<Message>(&duplicate).is_err(),
            "duplicate {field}"
        );
    }
    for field in ["source", "target"] {
        let mut raw_object: Value = serde_json::from_str(&raw).unwrap();
        let object = raw_object.as_object_mut().unwrap().remove(field).unwrap();
        let rest = serde_json::to_string(&raw_object).unwrap();
        let duplicate = format!("{{\"{field}\":{object},\"{field}\":{object},{}", &rest[1..]);
        assert!(serde_json::from_str::<Message>(&duplicate).is_err());
    }
}

#[test]
fn rejects_invalid_unicode_json_and_non_ascii_identity() {
    let raw = serde_json::to_string(&message()).unwrap();
    for escape in [r"\uD800", r"\uDC00", r"\uD800\u0041"] {
        let invalid = raw.replacen("Check source", escape, 1);
        assert!(serde_json::from_str::<Message>(&invalid).is_err());
    }
    let mut bytes = raw.into_bytes();
    let index = bytes
        .windows(12)
        .position(|window| window == b"Check source")
        .unwrap();
    bytes[index] = 0xff;
    assert!(serde_json::from_slice::<Message>(&bytes).is_err());
    let mut m = message();
    m.target.thread_id = "thread-\u{200b}1".into();
    rejects(m);
    let mut m = message();
    m.message_id = "ｍ-1".into();
    rejects(m);
}

#[test]
fn rejects_invalid_numeric_identifiers() {
    let raw = serde_json::to_string(&message()).unwrap();
    for (field, original) in [("issue_number", 1), ("comment_id", 2), ("author_id", 3)] {
        for value in [
            "0",
            "-1",
            "1.5",
            "1e0",
            "18446744073709551616",
            "\"2\"",
            "null",
            "true",
        ] {
            let invalid = raw.replacen(
                &format!("\"{field}\":{original}"),
                &format!("\"{field}\":{value}"),
                1,
            );
            assert!(
                serde_json::from_str::<Message>(&invalid).is_err(),
                "{field}: {value}"
            );
        }
    }
}

#[test]
fn validates_standalone_source_and_target_deserialization() {
    let mut source = message().source;
    source.author_id = 0;
    assert!(
        serde_json::from_str::<GithubSource>(&serde_json::to_string(&source).unwrap()).is_err()
    );
    let mut target = message().target;
    target.thread_id.clear();
    assert!(serde_json::from_str::<ChatTarget>(&serde_json::to_string(&target).unwrap()).is_err());
}

#[test]
fn rejects_invalid_provider_and_identity_lengths() {
    for provider in ["", "code x", "Codex", "codex\n", "codex/other", "1codex"] {
        let mut m = message();
        m.target.provider = provider.into();
        rejects(m);
    }
    let mut m = message();
    m.target.provider = "c".repeat(33);
    rejects(m);
    let mut m = message();
    m.message_id = "m".repeat(129);
    rejects(m);
    let mut m = message();
    m.target.thread_id = "t".repeat(129);
    rejects(m);
}

#[test]
fn valid_unicode_body_and_boundaries_round_trip() {
    for body in [
        "Задача: проверить 🚀\n不可信数据".into(),
        "x".repeat(65_536),
        "🚀".repeat(16_384),
    ] {
        let mut m = message();
        m.body = body;
        assert_eq!(validate(&m), Ok(()));
        assert_eq!(
            serde_json::from_slice::<Message>(&serde_json::to_vec(&m).unwrap()).unwrap(),
            m
        );
    }
    let mut m = message();
    m.message_id = "m".repeat(128);
    m.target.thread_id = "t".repeat(128);
    m.target.provider = "p".repeat(32);
    m.goal_id = format!("MSS-{}", "1".repeat(32));
    m.source.repository = format!("{}/{}", "a".repeat(39), "r".repeat(100));
    m.source.issue_number = u64::MAX;
    m.source.comment_id = u64::MAX;
    m.source.author_id = u64::MAX;
    assert_eq!(
        serde_json::from_slice::<Message>(&serde_json::to_vec(&m).unwrap()).unwrap(),
        m
    );
    let raw = serde_json::to_string(&message())
        .unwrap()
        .replace("Check source", r"\uD83D\uDE80");
    assert_eq!(serde_json::from_str::<Message>(&raw).unwrap().body, "🚀");
}

#[test]
fn delivery_status_has_exact_wire_names() {
    for (status, name) in [
        (DeliveryStatus::Pending, "PENDING"),
        (DeliveryStatus::Verified, "VERIFIED"),
        (DeliveryStatus::Delivering, "DELIVERING"),
        (DeliveryStatus::Delivered, "DELIVERED"),
        (DeliveryStatus::Acknowledged, "ACKNOWLEDGED"),
        (DeliveryStatus::Completed, "COMPLETED"),
        (DeliveryStatus::Blocked, "BLOCKED"),
        (DeliveryStatus::Unknown, "UNKNOWN"),
    ] {
        assert_eq!(serde_json::to_value(status).unwrap(), json!(name));
        assert_eq!(
            serde_json::from_value::<DeliveryStatus>(json!(name)).unwrap(),
            status
        );
    }
    for value in [
        json!("unknown"),
        json!("RETRY"),
        json!(0),
        json!({"UNKNOWN":true}),
    ] {
        assert!(serde_json::from_value::<DeliveryStatus>(value).is_err());
    }
}

#[test]
fn schema_error_keeps_its_classification() {
    let mut m = message();
    m.schema.clear();
    assert_eq!(validate(&m), Err(InvalidMessage::Schema));
}

#[test]
fn required_fields_and_malformed_json_are_rejected() {
    let value = serde_json::to_value(message()).unwrap();
    for field in [
        "schema",
        "message_id",
        "source",
        "target",
        "goal_id",
        "body",
    ] {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_str::<Message>(&missing.to_string()).is_err(),
            "{field}"
        );
    }
    let raw = serde_json::to_string(&message()).unwrap();
    for invalid in [
        format!("{raw}{{}}"),
        raw[..raw.len() - 1].to_owned(),
        "[]".into(),
        "null".into(),
    ] {
        assert!(serde_json::from_str::<Message>(&invalid).is_err());
    }
}

#[test]
fn validation_error_categories_are_stable_and_do_not_echo_body() {
    let mut m = message();
    m.source.author_id = 0;
    assert_eq!(validate(&m), Err(InvalidMessage::Identity));
    let mut m = message();
    m.source.repository = "../repo".into();
    assert_eq!(validate(&m), Err(InvalidMessage::Format));
    let mut m = message();
    m.body.clear();
    assert_eq!(validate(&m), Err(InvalidMessage::Size));
    for error in [
        InvalidMessage::Schema,
        InvalidMessage::Identity,
        InvalidMessage::Format,
        InvalidMessage::Size,
    ] {
        assert!(!error.to_string().is_empty());
        assert!(!error.to_string().contains("Check source"));
        let _: &dyn std::error::Error = &error;
    }
}
