use cwg_protocol::{ChatTarget, DeliveryStatus, GithubSource, Message};
use cwg_router::{BindingKey, MessageKey, Router, RouterLimits, RoutingError};

fn message(id: &str, comment_id: u64, body: &str) -> Message {
    Message {
        schema: "cwg-message-v1".into(),
        message_id: id.into(),
        source: GithubSource {
            repository: "owner/repo".into(),
            issue_number: 1,
            comment_id,
            author_id: 3,
        },
        target: ChatTarget {
            provider: "codex".into(),
            thread_id: "thread-1".into(),
        },
        goal_id: "CWG-001".into(),
        body: body.into(),
    }
}

fn binding(message: &Message) -> BindingKey {
    BindingKey {
        repository: message.source.repository.clone(),
        goal_id: message.goal_id.clone(),
    }
}

fn register(router: &mut Router, message: &Message) {
    router
        .register(binding(message), message.target.clone())
        .unwrap();
}

fn queued_with_limits(limits: RouterLimits) -> (Router, MessageKey) {
    let mut router = Router::with_limits(limits);
    let message = message("m1", 1, "x");
    let key = MessageKey::from(&message);
    register(&mut router, &message);
    router.enqueue_verified(message).unwrap();
    (router, key)
}

#[test]
fn default_admission_budgets_are_explicit_and_finite() {
    assert_eq!(
        RouterLimits::default(),
        RouterLimits {
            max_bindings: 1024,
            max_messages: 4096,
            max_body_bytes: 16 * 1024 * 1024,
        }
    );
}

#[test]
fn binding_budget_is_exact_and_suspension_does_not_free_it() {
    let mut router = Router::with_limits(RouterLimits {
        max_bindings: 1,
        ..RouterLimits::default()
    });
    let first = message("m1", 1, "x");
    register(&mut router, &first);
    let mut second = first.clone();
    second.goal_id = "CWG-002".into();
    assert_eq!(
        router.register(binding(&second), second.target.clone()),
        Err(RoutingError::CapacityExceeded)
    );
    router.suspend_binding(&binding(&first)).unwrap();
    assert_eq!(
        router.register(binding(&second), second.target.clone()),
        Err(RoutingError::CapacityExceeded)
    );
    assert_eq!(
        router.register(binding(&first), first.target.clone()),
        Err(RoutingError::AlreadyRegistered)
    );
}

#[test]
fn zero_admission_budgets_fail_closed() {
    let message = message("m1", 1, "x");
    let mut no_bindings = Router::with_limits(RouterLimits {
        max_bindings: 0,
        ..RouterLimits::default()
    });
    assert_eq!(
        no_bindings.register(binding(&message), message.target.clone()),
        Err(RoutingError::CapacityExceeded)
    );
    for limits in [
        RouterLimits {
            max_messages: 0,
            ..RouterLimits::default()
        },
        RouterLimits {
            max_body_bytes: 0,
            ..RouterLimits::default()
        },
    ] {
        let mut router = Router::with_limits(limits);
        register(&mut router, &message);
        assert_eq!(
            router.enqueue_verified(message.clone()),
            Err(RoutingError::CapacityExceeded)
        );
        assert_eq!(router.state(&MessageKey::from(&message)), None);
    }
}

#[test]
fn aggregate_body_budget_counts_utf8_bytes_and_failure_does_not_reserve_ids() {
    let mut router = Router::with_limits(RouterLimits {
        max_body_bytes: 8,
        ..RouterLimits::default()
    });
    let first = message("m1", 1, "🚀");
    let first_key = MessageKey::from(&first);
    register(&mut router, &first);
    router.enqueue_verified(first).unwrap(); // Four UTF-8 bytes.
    router.enqueue_verified(message("m2", 2, "é")).unwrap(); // Two more bytes.
    let rejected = message("m3", 3, "€");
    assert_eq!(
        router.enqueue_verified(rejected.clone()),
        Err(RoutingError::CapacityExceeded)
    );
    assert_eq!(router.state(&MessageKey::from(&rejected)), None);
    router.enqueue_verified(message("m3", 3, "x")).unwrap(); // Same IDs were not reserved.
    router.enqueue_verified(message("m4", 4, "x")).unwrap(); // Exact eight-byte boundary.
    assert_eq!(
        router.enqueue_verified(message("m5", 5, "x")),
        Err(RoutingError::CapacityExceeded)
    );
    router.begin_delivery(&first_key).unwrap();
    router.mark_delivered(&first_key).unwrap();
    router.acknowledge(&first_key).unwrap();
    router.complete(&first_key).unwrap();
    assert_eq!(
        router.enqueue_verified(message("m5", 5, "x")),
        Err(RoutingError::CapacityExceeded),
        "completion must not silently evict message bytes or replay history"
    );
    assert_eq!(
        router.enqueue_verified(message("new-id", 1, "x")),
        Err(RoutingError::Conflict)
    );
}

#[test]
fn message_budget_preserves_unknown_state_and_replay_history() {
    let (mut router, key) = queued_with_limits(RouterLimits {
        max_messages: 1,
        ..RouterLimits::default()
    });
    router.begin_delivery(&key).unwrap();
    router.mark_unknown(&key).unwrap();
    assert_eq!(
        router.enqueue_verified(message("m2", 2, "x")),
        Err(RoutingError::CapacityExceeded)
    );
    assert_eq!(router.state(&key), Some(DeliveryStatus::Unknown));
    assert_eq!(
        router.enqueue_verified(message("m1", 99, "x")),
        Err(RoutingError::Conflict)
    );
    assert_eq!(
        router.enqueue_verified(message("new-id", 1, "x")),
        Err(RoutingError::Conflict)
    );
    assert_eq!(
        router.begin_delivery(&key),
        Err(RoutingError::StaleOrWrongState)
    );
}

type Action = fn(&mut Router, &MessageKey) -> Result<(), RoutingError>;
const ACTIONS: [(DeliveryStatus, Action); 6] = [
    (DeliveryStatus::Verified, Router::begin_delivery),
    (DeliveryStatus::Delivering, Router::mark_delivered),
    (DeliveryStatus::Delivering, Router::mark_unknown),
    (DeliveryStatus::Delivered, Router::acknowledge),
    (DeliveryStatus::Acknowledged, Router::complete),
    (DeliveryStatus::Unknown, Router::block_unknown),
];

fn at(status: DeliveryStatus) -> (Router, MessageKey) {
    let (mut router, key) = queued_with_limits(RouterLimits::default());
    if status == DeliveryStatus::Verified {
        return (router, key);
    }
    router.begin_delivery(&key).unwrap();
    if status == DeliveryStatus::Delivering {
        return (router, key);
    }
    if status == DeliveryStatus::Unknown {
        router.mark_unknown(&key).unwrap();
        return (router, key);
    }
    router.mark_delivered(&key).unwrap();
    if status == DeliveryStatus::Delivered {
        return (router, key);
    }
    assert_eq!(status, DeliveryStatus::Acknowledged);
    router.acknowledge(&key).unwrap();
    (router, key)
}

#[test]
fn every_transition_and_query_checks_repository_goal_provider_and_thread() {
    for (status, action) in ACTIONS {
        let (mut router, key) = at(status);
        let mut wrong_repository = key.clone();
        wrong_repository.repository = "other/repo".into();
        let mut wrong_goal = key.clone();
        wrong_goal.goal_id = "CWG-002".into();
        let mut wrong_provider = key.clone();
        wrong_provider.target.provider = "other".into();
        let mut wrong_thread = key.clone();
        wrong_thread.target.thread_id = "thread-2".into();
        for forged in [wrong_repository, wrong_goal, wrong_provider, wrong_thread] {
            assert_eq!(router.state(&forged), None);
            assert_eq!(
                action(&mut router, &forged),
                Err(RoutingError::StaleOrWrongState)
            );
            assert_eq!(router.state(&key), Some(status));
        }
        let mut alias = key.clone();
        alias.repository = "OWNER/REPO".into();
        assert_eq!(router.state(&alias), Some(status));
        action(&mut router, &alias).unwrap();
    }
}

#[test]
fn malformed_message_keys_never_mutate_existing_messages() {
    let (mut router, key) = at(DeliveryStatus::Verified);
    let mut invalid_id = key.clone();
    invalid_id.message_id = "m".repeat(129);
    let mut invalid_repo = key.clone();
    invalid_repo.repository = "owner/repo/extra".into();
    let mut invalid_goal = key.clone();
    invalid_goal.goal_id = "CWG-".into();
    let mut invalid_target = key.clone();
    invalid_target.target.thread_id.clear();
    for malformed in [invalid_id, invalid_repo, invalid_goal, invalid_target] {
        assert_eq!(router.state(&malformed), None);
        for (_, action) in ACTIONS {
            assert_eq!(
                action(&mut router, &malformed),
                Err(RoutingError::StaleOrWrongState)
            );
            assert_eq!(router.state(&key), Some(DeliveryStatus::Verified));
        }
    }
}

#[test]
fn suspension_before_enqueue_is_irreversible_and_scoped() {
    let mut router = Router::default();
    let first = message("m1", 1, "x");
    let mut key = binding(&first);
    register(&mut router, &first);
    key.repository = "OWNER/REPO".into();
    router.suspend_binding(&key).unwrap();
    router.suspend_binding(&key).unwrap(); // Idempotent, never resumes admission.
    assert_eq!(
        router.enqueue_verified(first.clone()),
        Err(RoutingError::BindingSuspended)
    );
    assert_eq!(router.state(&MessageKey::from(&first)), None);
    for target in [
        first.target.clone(),
        ChatTarget {
            provider: "codex".into(),
            thread_id: "thread-2".into(),
        },
    ] {
        assert_eq!(
            router.register(key.clone(), target),
            Err(RoutingError::AlreadyRegistered)
        );
    }
    let mut second = message("m2", 2, "x");
    second.goal_id = "CWG-002".into();
    register(&mut router, &second);
    let second_key = MessageKey::from(&second);
    router.enqueue_verified(second).unwrap();
    router.begin_delivery(&second_key).unwrap();
}

#[test]
fn suspension_before_claim_preserves_verified_message_and_history() {
    let (mut router, key) = at(DeliveryStatus::Verified);
    router
        .suspend_binding(&binding(&message("m1", 1, "x")))
        .unwrap();
    assert_eq!(
        router.begin_delivery(&key),
        Err(RoutingError::BindingSuspended)
    );
    assert_eq!(router.state(&key), Some(DeliveryStatus::Verified));
    assert_eq!(
        router.enqueue_verified(message("m2", 2, "x")),
        Err(RoutingError::BindingSuspended)
    );
    assert_eq!(
        router.register(binding(&message("m1", 1, "x")), key.target.clone()),
        Err(RoutingError::AlreadyRegistered)
    );
}

#[test]
fn suspension_allows_recording_inflight_unknown_but_never_retries() {
    let (mut router, key) = at(DeliveryStatus::Delivering);
    router
        .suspend_binding(&binding(&message("m1", 1, "x")))
        .unwrap();
    router.mark_unknown(&key).unwrap();
    assert_eq!(router.state(&key), Some(DeliveryStatus::Unknown));
    assert_eq!(
        router.begin_delivery(&key),
        Err(RoutingError::StaleOrWrongState)
    );
    router.block_unknown(&key).unwrap();
    assert_eq!(router.state(&key), Some(DeliveryStatus::Blocked));
    assert_eq!(
        router.begin_delivery(&key),
        Err(RoutingError::StaleOrWrongState)
    );
    assert_eq!(
        router.enqueue_verified(message("m1", 1, "x")),
        Err(RoutingError::BindingSuspended)
    );
}

#[test]
fn suspension_allows_recording_inflight_success_without_reopening_binding() {
    let (mut router, key) = at(DeliveryStatus::Delivering);
    router
        .suspend_binding(&binding(&message("m1", 1, "x")))
        .unwrap();
    router.mark_delivered(&key).unwrap();
    router.acknowledge(&key).unwrap();
    router.complete(&key).unwrap();
    assert_eq!(router.state(&key), Some(DeliveryStatus::Completed));
    assert_eq!(
        router.enqueue_verified(message("m2", 2, "x")),
        Err(RoutingError::BindingSuspended)
    );
}

#[test]
fn suspension_of_unknown_or_malformed_binding_changes_nothing() {
    let (mut router, key) = at(DeliveryStatus::Verified);
    for binding in [
        BindingKey {
            repository: "other/repo".into(),
            goal_id: "CWG-001".into(),
        },
        BindingKey {
            repository: "owner/repo".into(),
            goal_id: "CWG-002".into(),
        },
        BindingKey {
            repository: "o".repeat(141),
            goal_id: "CWG-001".into(),
        },
    ] {
        assert_eq!(
            router.suspend_binding(&binding),
            Err(RoutingError::UnboundTarget)
        );
        assert_eq!(router.state(&key), Some(DeliveryStatus::Verified));
    }
    router.begin_delivery(&key).unwrap();
}
