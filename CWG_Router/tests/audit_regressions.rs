use cwg_protocol::{ChatTarget, DeliveryStatus, GithubSource, Message};
use cwg_router::{BindingKey, Router, RoutingError};

fn message() -> Message {
    Message {
        schema: "cwg-message-v1".into(),
        message_id: "m1".into(),
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
        body: "Untrusted task context".into(),
    }
}

#[test]
fn refuses_acknowledgement_before_provider_acceptance() {
    let mut router = Router::default();
    let m = message();
    router
        .register(
            BindingKey {
                repository: m.source.repository.clone(),
                goal_id: m.goal_id.clone(),
            },
            m.target.clone(),
        )
        .unwrap();
    router.enqueue_verified(m).unwrap();
    router.begin_delivery("m1").unwrap();
    assert_eq!(
        router.acknowledge("m1"),
        Err(RoutingError::StaleOrWrongState)
    );
    assert_eq!(router.state("m1"), Some(DeliveryStatus::Delivering));
}

#[test]
fn invalid_binding_is_not_registered() {
    let m = message();
    let mut router = Router::default();
    assert_eq!(
        router.register(
            BindingKey {
                repository: m.source.repository,
                goal_id: "CWG-".into()
            },
            m.target
        ),
        Err(RoutingError::InvalidMessage)
    );
}
