use cwg_protocol::{ChatTarget, DeliveryStatus, GithubSource, Message};
use cwg_router::{BindingKey, MessageKey, Router, RoutingError};

fn message(repository: &str, id: &str, comment_id: u64) -> Message {
    Message {
        schema: "cwg-message-v1".into(),
        message_id: id.into(),
        source: GithubSource {
            repository: repository.into(),
            issue_number: 1,
            comment_id,
            author_id: 3,
        },
        target: ChatTarget {
            provider: "codex".into(),
            thread_id: "thread-1".into(),
        },
        goal_id: "CWG-001".into(),
        body: "x".into(),
    }
}

fn register(router: &mut Router, message: &Message) {
    router
        .register(
            BindingKey {
                repository: message.source.repository.clone(),
                goal_id: message.goal_id.clone(),
            },
            message.target.clone(),
        )
        .unwrap();
}

#[test]
fn default_message_capacity_is_finite() {
    let mut router = Router::default();
    register(&mut router, &message("owner/repo", "first", 1));
    for index in 1..=4096 {
        router
            .enqueue_verified(message("owner/repo", &format!("m-{index}"), index))
            .unwrap();
    }
    assert_eq!(
        router.enqueue_verified(message("owner/repo", "overflow", 4097)),
        Err(RoutingError::CapacityExceeded),
        "default Router must refuse admission above its message budget"
    );
}

#[test]
fn default_binding_capacity_is_finite() {
    let mut router = Router::default();
    for index in 1..=1024 {
        let mut message = message("owner/repo", "m-1", 1);
        message.goal_id = format!("CWG-{index}");
        register(&mut router, &message);
    }
    let message = message("owner/repo", "m-1", 1);
    assert_eq!(
        router.register(
            BindingKey {
                repository: "owner/repo".into(),
                goal_id: "CWG-1025".into(),
            },
            message.target,
        ),
        Err(RoutingError::CapacityExceeded),
        "default Router must refuse admission above its binding budget"
    );
}

#[test]
fn message_id_collision_in_another_repository_is_independent() {
    let mut router = Router::default();
    let first = message("owner/repo", "same-id", 1);
    let second = message("other/repo", "same-id", 1);
    register(&mut router, &first);
    register(&mut router, &second);
    let first_key = MessageKey::from(&first);
    let second_key = MessageKey::from(&second);
    router.enqueue_verified(first).unwrap();
    assert!(
        router.enqueue_verified(second).is_ok(),
        "a message ID in another repository must not reserve this repository's namespace"
    );
    router.begin_delivery(&first_key).unwrap();
    assert_eq!(router.state(&first_key), Some(DeliveryStatus::Delivering));
    assert_eq!(router.state(&second_key), Some(DeliveryStatus::Verified));
}
