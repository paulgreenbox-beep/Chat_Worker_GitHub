#![cfg(windows)]
//! Read-only State scopes and pagination on actual protected Windows files.
#[path = "support/windows_fixture.rs"]
mod windows_fixture;

use cwg_protocol::{ChatTarget, DeliveryStatus, GithubSource, Message};
use cwg_state::{Binding, Store};
use std::{fs, path::PathBuf};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        Self(windows_fixture::private_temp_dir("scoped-reads"))
    }
    fn store(&self) -> Store {
        Store::open(&self.0).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove only owned scoped-read fixture");
    }
}

fn binding(id: &str, issue: u64, goal: &str, thread: &str) -> Binding {
    Binding {
        id: id.into(),
        repository_id: 10,
        owner_id: 20,
        repository: "owner/repo".into(),
        issue_number: issue,
        goal_id: goal.into(),
        generation: 1,
        account_id: format!("account-{id}"),
        target: ChatTarget {
            provider: "codex".into(),
            thread_id: thread.into(),
        },
    }
}

fn message(binding: &Binding, comment: u64) -> Message {
    Message {
        schema: "cwg-message-v1".into(),
        message_id: format!("gh:10:{comment}"),
        source: GithubSource {
            repository: binding.repository.clone(),
            issue_number: binding.issue_number,
            comment_id: comment,
            author_id: 20,
        },
        target: binding.target.clone(),
        goal_id: binding.goal_id.clone(),
        body: "untrusted task data".into(),
    }
}

#[test]
fn pages_skip_only_the_bound_route_and_reads_never_acknowledge() {
    let temp = Fixture::new();
    let store = temp.store();
    let a = binding("agent-a", 1, "CWG-001", "thread-a");
    let b = binding("agent-b", 2, "CWG-002", "thread-b");
    store.register(&a).unwrap();
    store.register(&b).unwrap();
    for i in 0..105u64 {
        let own = message(&a, 2 * i + 1);
        let foreign = message(&b, 2 * i + 2);
        assert!(store.put_verified(&a, &format!("delivery-a-{i}"), own).unwrap());
        assert!(store.put_verified(&b, &format!("delivery-b-{i}"), foreign).unwrap());
    }

    let first = store.inbox_page(&a, 0, 100).unwrap();
    assert_eq!(first.len(), 100);
    assert_eq!(first[0].message.message_id, "gh:10:1");
    assert_eq!(first[99].message.message_id, "gh:10:199");
    let second = store.inbox_page(&a, 100, 100).unwrap();
    assert_eq!(second.len(), 5);
    assert_eq!(second[0].message.message_id, "gh:10:201");
    assert_eq!(second[4].message.message_id, "gh:10:209");
    assert_eq!(store.inbox_page(&b, 0, 1).unwrap()[0].message.message_id, "gh:10:2");
    assert!(store.inbox_page(&a, 1024, 1).unwrap().is_empty());
    assert_eq!(store.inbox_page(&a, 1025, 1).err(), Some("INBOX_PAGE_INVALID"));
    assert_eq!(store.inbox_page(&a, 0, 0).err(), Some("INBOX_PAGE_INVALID"));
    assert_eq!(store.inbox_page(&a, 0, 101).err(), Some("INBOX_PAGE_INVALID"));
    assert_eq!(store.inbox_page(&a, 0, 1).unwrap().len(), 1);

    assert!(store.get_message(&a, "gh:10:2").unwrap().is_none());
    assert!(store.get_message(&a, "nonexistent").unwrap().is_none());
    assert_eq!(store.get_message(&a, "").err(), Some("MESSAGE_ID_INVALID"));
    assert_eq!(
        store.get_message(&a, &"bad-".repeat(40)).err(),
        Some("MESSAGE_ID_INVALID")
    );
    let mut stale = a.clone();
    stale.generation += 1;
    assert_eq!(
        store.get_message(&stale, "gh:10:1").err(),
        Some("BINDING_STALE_OR_UNKNOWN")
    );
    for item in first.iter().chain(second.iter()) {
        assert_eq!(item.status, DeliveryStatus::Verified);
    }
    assert_eq!(
        store.get_message(&a, "gh:10:1").unwrap().unwrap().status,
        DeliveryStatus::Verified
    );
    drop(store);
    assert_eq!(
        temp.store().get_message(&a, "gh:10:1").unwrap().unwrap().status,
        DeliveryStatus::Verified
    );
    assert_eq!(temp.store().inbox_page(&b, 100, 10).unwrap().len(), 5);
}
