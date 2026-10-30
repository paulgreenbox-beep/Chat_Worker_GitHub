use cwg_protocol::{ChatTarget, GithubSource, Message};

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
        body: "PRIVATE_BODY_FOR_DEBUG_REGRESSION".into(),
    }
}

#[test]
fn rejects_full_root_array() {
    let raw = r#"["cwg-message-v1","m-1",{"repository":"owner/repo","issue_number":1,"comment_id":2,"author_id":3},{"provider":"codex","thread_id":"thread-1"},"CWG-001","body"]"#;
    assert!(serde_json::from_str::<Message>(raw).is_err());
}

#[test]
fn rejects_full_source_array() {
    let raw = r#"{"schema":"cwg-message-v1","message_id":"m-1","source":["owner/repo",1,2,3],"target":{"provider":"codex","thread_id":"thread-1"},"goal_id":"CWG-001","body":"body"}"#;
    assert!(serde_json::from_str::<Message>(raw).is_err());
    assert!(serde_json::from_str::<GithubSource>(r#"["owner/repo",1,2,3]"#).is_err());
}

#[test]
fn rejects_full_target_array() {
    let raw = r#"{"schema":"cwg-message-v1","message_id":"m-1","source":{"repository":"owner/repo","issue_number":1,"comment_id":2,"author_id":3},"target":["codex","thread-1"],"goal_id":"CWG-001","body":"body"}"#;
    assert!(serde_json::from_str::<Message>(raw).is_err());
    assert!(serde_json::from_str::<ChatTarget>(r#"["codex","thread-1"]"#).is_err());
}

#[test]
fn message_debug_redacts_body() {
    let message = message();
    let debug = format!("{message:?}");
    assert!(!debug.contains(&message.body));
    assert!(debug.contains("body_bytes"));
    assert!(debug.contains(&message.body.len().to_string()));
}
