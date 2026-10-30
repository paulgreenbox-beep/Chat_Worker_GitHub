//! Multi-route logical controller regressions; not provider authentication.
use cwg_protocol::{ChatTarget, DeliveryStatus, GithubSource, Message};
use cwg_router::{BindingKey, MessageKey, Router, RouterLimits, RoutingError};

fn message(repository: &str, goal: &str, thread: &str, id: &str, comment: u64) -> Message {
    Message {
        schema: "cwg-message-v1".into(),
        message_id: id.into(),
        source: GithubSource {
            repository: repository.into(),
            issue_number: 1,
            comment_id: comment,
            author_id: 2,
        },
        target: ChatTarget {
            provider: "codex".into(),
            thread_id: thread.into(),
        },
        goal_id: goal.into(),
        body: "Untrusted task data".into(),
    }
}

fn bind(router: &mut Router, message: &Message) -> BindingKey {
    let binding = BindingKey {
        repository: message.source.repository.clone(),
        goal_id: message.goal_id.clone(),
    };
    router
        .register(binding.clone(), message.target.clone())
        .unwrap();
    binding
}

#[test]
fn suspension_retains_all_inflight_outcomes_without_freezing_a_neighbor() {
    // Suspending an owner cannot undo an already dispatched effect.
    for outcome in [DeliveryStatus::Delivered, DeliveryStatus::Unknown] {
        let mut router = Router::default();
        let a = message("owner/repo", "CWG-001", "thread-a", "m-a", 1);
        let b = message("owner/repo", "CWG-002", "thread-b", "m-b", 2);
        let binding_a = bind(&mut router, &a);
        bind(&mut router, &b);
        let key_a = MessageKey::from(&a);
        let key_b = MessageKey::from(&b);
        router.enqueue_verified(a).unwrap();
        router.enqueue_verified(b).unwrap();
        router.begin_delivery(&key_a).unwrap();
        router.suspend_binding(&binding_a).unwrap();
        router.suspend_binding(&binding_a).unwrap();
        if outcome == DeliveryStatus::Delivered {
            router.mark_delivered(&key_a).unwrap();
            router.acknowledge(&key_a).unwrap();
            router.complete(&key_a).unwrap();
            assert_eq!(router.state(&key_a), Some(DeliveryStatus::Completed));
        } else {
            router.mark_unknown(&key_a).unwrap();
            router.block_unknown(&key_a).unwrap();
            assert_eq!(router.state(&key_a), Some(DeliveryStatus::Blocked));
        }
        assert_eq!(
            router.begin_delivery(&key_a),
            Err(RoutingError::StaleOrWrongState)
        );
        router.begin_delivery(&key_b).unwrap();
        router.mark_delivered(&key_b).unwrap();
        router.acknowledge(&key_b).unwrap();
        router.complete(&key_b).unwrap();
        assert_eq!(router.state(&key_b), Some(DeliveryStatus::Completed));
    }
}

#[test]
fn a_suspended_route_refusal_reserves_neither_identity_nor_budget() {
    let mut router = Router::with_limits(RouterLimits {
        max_bindings: 2,
        max_messages: 1,
        max_body_bytes: "Untrusted task data".len(),
    });
    let rejected = message("owner/repo", "CWG-001", "thread-a", "shared-id", 11);
    let valid = message("OWNER/REPO", "CWG-002", "thread-b", "shared-id", 11);
    let binding = bind(&mut router, &rejected);
    bind(&mut router, &valid);
    router.suspend_binding(&binding).unwrap();
    assert_eq!(
        router.enqueue_verified(rejected),
        Err(RoutingError::BindingSuspended)
    );
    let valid_key = MessageKey::from(&valid);
    router.enqueue_verified(valid).unwrap();
    assert_eq!(router.state(&valid_key), Some(DeliveryStatus::Verified));
}

#[test]
fn a_cross_thread_observation_cannot_close_the_other_threads_unknown_outcome() {
    let mut router = Router::default();
    let a = message("owner/repo", "CWG-001", "thread-a", "one", 10);
    let b = message("owner/repo", "CWG-002", "thread-b", "two", 20);
    bind(&mut router, &a);
    bind(&mut router, &b);
    let key_a = MessageKey::from(&a);
    let key_b = MessageKey::from(&b);
    router.enqueue_verified(a).unwrap();
    router.enqueue_verified(b).unwrap();
    router.begin_delivery(&key_a).unwrap();
    router.mark_unknown(&key_a).unwrap();
    let mut forged = key_a.clone();
    forged.target = key_b.target.clone();
    assert_eq!(
        router.block_unknown(&forged),
        Err(RoutingError::StaleOrWrongState)
    );
    assert_eq!(router.state(&key_a), Some(DeliveryStatus::Unknown));
    assert_eq!(router.state(&key_b), Some(DeliveryStatus::Verified));
    assert_eq!(router.state(&forged), None);
    router.block_unknown(&key_a).unwrap();
}
