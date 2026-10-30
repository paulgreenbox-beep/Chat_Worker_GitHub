use cwg_protocol::{ChatTarget, DeliveryStatus, GithubSource, Message};
use cwg_state::{Binding, Store, Transition};
#[cfg(not(windows))]
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, path::PathBuf};
#[cfg(windows)]
#[path = "support/windows_fixture.rs"]
mod windows_fixture;
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        #[cfg(windows)]
        let p = windows_fixture::private_temp_dir("state");
        #[cfg(not(windows))]
        let p = {
            let p = std::env::temp_dir().join(format!(
                "cwg-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&p).unwrap();
            p
        };
        Self(p)
    }
    fn store(&self) -> Store {
        Store::open(&self.0).unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn binding() -> Binding {
    Binding {
        id: "main-chat".into(),
        repository_id: 10,
        owner_id: 20,
        repository: "owner/repo".into(),
        issue_number: 1,
        goal_id: "CWG-1".into(),
        generation: 1,
        account_id: "operator".into(),
        target: ChatTarget {
            provider: "codex".into(),
            thread_id: "thread-one".into(),
        },
    }
}
fn message(b: &Binding, comment: u64) -> Message {
    Message {
        schema: "cwg-message-v1".into(),
        message_id: format!("gh:{}:{comment}", b.repository_id),
        source: GithubSource {
            repository: b.repository.clone(),
            issue_number: b.issue_number,
            comment_id: comment,
            author_id: 7,
        },
        target: b.target.clone(),
        goal_id: b.goal_id.clone(),
        body: "Untrusted task data".into(),
    }
}
#[test]
fn survives_reopen_and_changed_delivery_header() {
    let t = Temp::new();
    let b = binding();
    t.store().register(&b).unwrap();
    assert!(
        t.store()
            .put_verified(&b, "delivery-a", message(&b, 100))
            .unwrap()
    );
    assert!(
        !t.store()
            .put_verified(&b, "delivery-b", message(&b, 100))
            .unwrap()
    );
    assert_eq!(t.store().inbox(&b, 100).unwrap().len(), 1);
}
#[test]
fn changed_body_cannot_replace_committed_message() {
    let t = Temp::new();
    let b = binding();
    let s = t.store();
    s.register(&b).unwrap();
    s.put_verified(&b, "a", message(&b, 1)).unwrap();
    let mut m = message(&b, 1);
    m.body = "edited".into();
    assert!(s.put_verified(&b, "b", m).is_err());
}
#[test]
fn binding_is_immutable_across_restart() {
    let t = Temp::new();
    let mut b = binding();
    t.store().register(&b).unwrap();
    b.target.thread_id = "neighbor".into();
    assert!(t.store().register(&b).is_err());
}
#[test]
fn unfinished_claim_is_not_retryable_after_restart() {
    let t = Temp::new();
    let b = binding();
    let s = t.store();
    s.register(&b).unwrap();
    s.put_verified(&b, "a", message(&b, 1)).unwrap();
    s.advance(&b, "gh:10:1", Transition::Begin).unwrap();
    assert!(t.store().advance(&b, "gh:10:1", Transition::Begin).is_err());
    assert_eq!(
        t.store().inbox(&b, 1).unwrap()[0].status,
        DeliveryStatus::Delivering
    );
    s.advance(&b, "gh:10:1", Transition::Unknown).unwrap();
    assert!(s.advance(&b, "gh:10:1", Transition::Begin).is_err());
}
#[test]
fn partial_append_blocks_without_truncating() {
    let t = Temp::new();
    let b = binding();
    t.store().register(&b).unwrap();
    let p = t.0.join("inbox.jsonl");
    let mut raw = fs::read(&p).unwrap();
    raw.extend(b"{\"partial\":");
    fs::write(&p, &raw).unwrap();
    assert!(t.store().inbox(&b, 1).is_err());
    assert_eq!(fs::read(p).unwrap(), raw);
}
#[test]
fn neighbor_binding_cannot_read_or_transition_other_message() {
    let t = Temp::new();
    let b = binding();
    let mut other = b.clone();
    other.id = "other".into();
    other.issue_number = 2;
    other.goal_id = "CWG-2".into();
    other.target.thread_id = "thread-two".into();
    let s = t.store();
    s.register(&b).unwrap();
    s.register(&other).unwrap();
    s.put_verified(&b, "a", message(&b, 1)).unwrap();
    assert!(s.inbox(&other, 100).unwrap().is_empty());
    assert!(s.advance(&other, "gh:10:1", Transition::Begin).is_err());
}
#[test]
fn out_of_order_ack_does_not_change_state() {
    let t = Temp::new();
    let b = binding();
    let s = t.store();
    s.register(&b).unwrap();
    s.put_verified(&b, "a", message(&b, 1)).unwrap();
    assert!(s.advance(&b, "gh:10:1", Transition::Acknowledge).is_err());
    assert_eq!(s.inbox(&b, 1).unwrap()[0].status, DeliveryStatus::Verified);
}
#[test]
fn same_delivery_cannot_name_a_second_source() {
    let t = Temp::new();
    let b = binding();
    let s = t.store();
    s.register(&b).unwrap();
    s.put_verified(&b, "a", message(&b, 1)).unwrap();
    assert!(s.put_verified(&b, "a", message(&b, 2)).is_err());
}
#[test]
fn security_regression_new_delivery_alias_is_durable() {
    let t = Temp::new();
    let b = binding();
    let s = t.store();
    s.register(&b).unwrap();
    assert!(s.put_verified(&b, "delivery-a", message(&b, 1)).unwrap());
    assert!(!s.put_verified(&b, "delivery-b", message(&b, 1)).unwrap());
    assert!(
        t.store()
            .put_verified(&b, "delivery-b", message(&b, 2))
            .is_err(),
        "a delivery ID already observed for comment 1 must not name comment 2 after restart"
    );
    assert_eq!(t.store().inbox(&b, 100).unwrap().len(), 1);
}
#[test]
fn security_regression_delivery_alias_cannot_cross_sources() {
    let t = Temp::new();
    let b = binding();
    let s = t.store();
    s.register(&b).unwrap();
    assert!(s.put_verified(&b, "delivery-a", message(&b, 1)).unwrap());
    assert!(s.put_verified(&b, "delivery-b", message(&b, 2)).unwrap());
    assert!(
        s.put_verified(&b, "delivery-b", message(&b, 1)).is_err(),
        "canonical-message dedupe must not conceal delivery ID correlation to another source"
    );
    assert_eq!(s.inbox(&b, 100).unwrap().len(), 2);
}
#[test]
fn security_regression_journal_record_requires_object() {
    let t = Temp::new();
    let b = binding();
    t.store().register(&b).unwrap();
    let path = t.0.join("inbox.jsonl");
    let record: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let positional = serde_json::json!([record["schema"], record["sequence"], record["event"]]);
    let mut raw = serde_json::to_vec(&positional).unwrap();
    raw.push(b'\n');
    fs::write(&path, &raw).unwrap();
    assert!(
        t.store().inbox(&b, 1).is_err(),
        "a journal record must be a named-field object"
    );
    assert_eq!(fs::read(path).unwrap(), raw);
}
#[cfg(windows)]
#[test]
fn security_regression_state_root_requires_private_acl() {
    let t = Temp::new();
    // The fixture is this test's newly created, synthetic directory only. No
    // shell, recursive grant, inherited host ACL or external path is modified.
    let executable = PathBuf::from(std::env::var_os("SystemRoot").expect("Windows SystemRoot"))
        .join("System32")
        .join("icacls.exe");
    let result = std::process::Command::new(executable)
        .arg(&t.0)
        .args(["/grant", "*S-1-1-0:(OI)(CI)(F)", "/Q"])
        .output()
        .expect("run existing Windows ACL utility for owned fixture");
    assert!(
        result.status.success(),
        "owned fixture ACL setup must actually succeed"
    );
    assert!(
        Store::open(&t.0).is_err(),
        "State must refuse an actual World-writable/readable root, not trust an operator-only comment"
    );
}
#[test]
fn sequence_corruption_is_rejected() {
    let t = Temp::new();
    let b = binding();
    t.store().register(&b).unwrap();
    let p = t.0.join("inbox.jsonl");
    let raw = fs::read_to_string(&p)
        .unwrap()
        .replace("\"sequence\":1", "\"sequence\":2");
    fs::write(p, raw).unwrap();
    assert!(t.store().inbox(&b, 1).is_err());
}
#[test]
fn binding_decode_requires_object_and_validated_identity() {
    let b = binding();
    let mut value = serde_json::to_value(&b).unwrap();
    let positional = serde_json::json!([
        value["id"],
        value["repository_id"],
        value["owner_id"],
        value["repository"],
        value["issue_number"],
        value["goal_id"],
        value["generation"],
        value["account_id"],
        value["target"]
    ]);
    assert!(serde_json::from_value::<Binding>(positional).is_err());
    value["generation"] = serde_json::json!(0);
    assert!(serde_json::from_value::<Binding>(value).is_err());
}
#[test]
fn persisted_pending_item_requires_object_without_repairing_the_journal() {
    let t = Temp::new();
    let b = binding();
    let s = t.store();
    s.register(&b).unwrap();
    s.put_verified(&b, "a", message(&b, 1)).unwrap();
    let path = t.0.join("inbox.jsonl");
    let original = fs::read_to_string(&path).unwrap();
    let mut records: Vec<serde_json::Value> = original
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let item = records[1]["event"]["item"].clone();
    records[1]["event"]["item"] = serde_json::json!([
        item["binding_id"],
        item["generation"],
        item["repository_id"],
        item["delivery_id"],
        item["message"]
    ]);
    let raw = records
        .iter()
        .map(|record| format!("{record}\n"))
        .collect::<String>();
    fs::write(&path, &raw).unwrap();
    assert!(s.inbox(&b, 1).is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), raw);
}
#[test]
fn duplicate_journal_enum_fields_are_refused_without_truncation() {
    let t = Temp::new();
    let b = binding();
    let s = t.store();
    s.register(&b).unwrap();
    let path = t.0.join("inbox.jsonl");
    let original = fs::read_to_string(&path).unwrap();
    let raw = original.replacen(
        "\"kind\":\"Bind\"",
        "\"kind\":\"Bind\",\"kind\":\"Bind\"",
        1,
    );
    assert_ne!(raw, original);
    fs::write(&path, &raw).unwrap();
    assert!(s.inbox(&b, 1).is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), raw);
}
#[test]
fn concurrent_claims_have_one_winner() {
    let t = Temp::new();
    let b = binding();
    t.store().register(&b).unwrap();
    t.store().put_verified(&b, "a", message(&b, 1)).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let joins: Vec<_> = (0..8)
        .map(|_| {
            let b = b.clone();
            let p = t.0.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                Store::open(&p)
                    .and_then(|store| store.advance(&b, "gh:10:1", Transition::Begin))
                    .is_ok()
            })
        })
        .collect();
    assert_eq!(
        joins
            .into_iter()
            .map(|j| j.join().expect("owned claimant thread must not panic"))
            .filter(|ok| *ok)
            .count(),
        1
    );
}
#[cfg(unix)]
#[test]
fn symlink_root_refused() {
    let t = Temp::new();
    let link = t.0.join("link");
    std::os::unix::fs::symlink(&t.0, &link).unwrap();
    assert!(Store::open(&link).is_err());
}
