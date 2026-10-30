//! Public-contract regressions; logical routing only, not authenticated chat tests.
use cwg_protocol::{ChatTarget, DeliveryStatus, GithubSource, Message};
use cwg_router::{BindingKey, MessageKey, Router, RouterLimits, RoutingError};

fn message(goal: &str, thread: &str, id: &str, comment: u64) -> Message {
    Message {
        schema: "cwg-message-v1".into(),
        message_id: id.into(),
        source: GithubSource {
            repository: "owner/repo".into(),
            issue_number: 1,
            comment_id: comment,
            author_id: 2,
        },
        target: ChatTarget {
            provider: "codex".into(),
            thread_id: thread.into(),
        },
        goal_id: goal.into(),
        body: "data".into(),
    }
}
fn register(router: &mut Router, message: &Message) -> BindingKey {
    let key = BindingKey {
        repository: message.source.repository.clone(),
        goal_id: message.goal_id.clone(),
    };
    router
        .register(key.clone(), message.target.clone())
        .unwrap();
    key
}

#[test]
fn revoked_inflight_outcome_remains_recordable_without_reopening_a_claim() {
    let mut router = Router::default();
    let first = message("CWG-1", "thread-a", "m1", 10);
    let second = message("CWG-2", "thread-b", "m2", 11);
    let binding = register(&mut router, &first);
    register(&mut router, &second);
    let a = MessageKey::from(&first);
    let b = MessageKey::from(&second);
    router.enqueue_verified(first.clone()).unwrap();
    router.enqueue_verified(second).unwrap();
    router.begin_delivery(&a).unwrap();
    router.suspend_binding(&binding).unwrap();
    router.mark_unknown(&a).unwrap();
    router.block_unknown(&a).unwrap();
    assert_eq!(router.state(&a), Some(DeliveryStatus::Blocked));
    assert_eq!(
        router.begin_delivery(&a),
        Err(RoutingError::StaleOrWrongState)
    );
    assert_eq!(
        router.acknowledge(&a),
        Err(RoutingError::StaleOrWrongState)
    );
    assert_eq!(
        router.enqueue_verified(first),
        Err(RoutingError::BindingSuspended)
    );
    router.begin_delivery(&b).unwrap();
    router.mark_delivered(&b).unwrap();
    router.acknowledge(&b).unwrap();
    router.complete(&b).unwrap();
    assert_eq!(router.state(&b), Some(DeliveryStatus::Completed));
    assert_eq!(router.state(&a), Some(DeliveryStatus::Blocked));
}

#[test]
fn failed_capacity_admission_reserves_neither_id_and_completion_never_evicts_replay() {
    let mut router = Router::with_limits(RouterLimits {
        max_bindings: 1,
        max_messages: 1,
        max_body_bytes: 4,
    });
    let valid = message("CWG-1", "thread-a", "m1", 10);
    register(&mut router, &valid);
    let key = MessageKey::from(&valid);
    let mut oversized = valid.clone();
    oversized.body.push('!');
    assert_eq!(
        router.enqueue_verified(oversized),
        Err(RoutingError::CapacityExceeded)
    );
    assert_eq!(router.state(&key), None);
    router.enqueue_verified(valid.clone()).unwrap();
    router.begin_delivery(&key).unwrap();
    router.mark_delivered(&key).unwrap();
    router.acknowledge(&key).unwrap();
    router.complete(&key).unwrap();
    let mut replay = valid;
    replay.message_id = "different-id".into();
    assert_eq!(
        router.enqueue_verified(replay),
        Err(RoutingError::Conflict)
    );
    assert_eq!(router.state(&key), Some(DeliveryStatus::Completed));
}
