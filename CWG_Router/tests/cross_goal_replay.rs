use cwg_protocol::{ChatTarget, DeliveryStatus, GithubSource, Message};
use cwg_router::{BindingKey, MessageKey, Router, RoutingError};

fn message(goal: &str, thread: &str, id: &str, comment_id: u64) -> Message {
    Message {
        schema: "cwg-message-v1".into(),
        message_id: id.into(),
        source: GithubSource {
            repository: "owner/repo".into(),
            issue_number: 1,
            comment_id,
            author_id: 7,
        },
        target: ChatTarget {
            provider: "codex".into(),
            thread_id: thread.into(),
        },
        goal_id: goal.into(),
        body: "Synthetic untrusted routing context".into(),
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
fn comment_replay_across_goals_and_threads_does_not_reserve_the_new_message_id() {
    let mut router = Router::default();
    let first = message("CWG-1", "thread-one", "first", 10);
    let mut replay = message("CWG-2", "thread-two", "second", 10);
    let first_key = MessageKey::from(&first);
    let second_key = MessageKey::from(&replay);
    register(&mut router, &first);
    register(&mut router, &replay);

    router.enqueue_verified(first).unwrap();
    assert_eq!(router.state(&first_key), Some(DeliveryStatus::Verified));
    assert_eq!(router.state(&second_key), None);
    assert_eq!(
        router.enqueue_verified(replay.clone()),
        Err(RoutingError::Conflict),
        "a comment cannot be replayed by changing GOAL and thread"
    );
    assert_eq!(router.state(&first_key), Some(DeliveryStatus::Verified));
    assert_eq!(router.state(&second_key), None);

    // The failed replay must not reserve its previously unused message ID.
    replay.source.comment_id = 11;
    router.enqueue_verified(replay).unwrap();
    assert_eq!(router.state(&first_key), Some(DeliveryStatus::Verified));
    assert_eq!(router.state(&second_key), Some(DeliveryStatus::Verified));
    router.begin_delivery(&second_key).unwrap();
    assert_eq!(router.state(&first_key), Some(DeliveryStatus::Verified));
    assert_eq!(router.state(&second_key), Some(DeliveryStatus::Delivering));
}

#[test]
fn message_id_replay_across_goals_and_threads_does_not_reserve_the_new_comment() {
    let mut router = Router::default();
    let first = message("CWG-1", "thread-one", "shared", 10);
    let mut replay = message("CWG-2", "thread-two", "shared", 11);
    let first_key = MessageKey::from(&first);
    let rejected_key = MessageKey::from(&replay);
    register(&mut router, &first);
    register(&mut router, &replay);

    router.enqueue_verified(first).unwrap();
    assert_eq!(router.state(&first_key), Some(DeliveryStatus::Verified));
    assert_eq!(router.state(&rejected_key), None);
    assert_eq!(
        router.enqueue_verified(replay.clone()),
        Err(RoutingError::Conflict),
        "a message ID belongs to its repository even across different GOALs"
    );
    assert_eq!(router.state(&first_key), Some(DeliveryStatus::Verified));
    assert_eq!(router.state(&rejected_key), None);

    // The failed collision must not reserve its previously unused comment ID.
    replay.message_id = "second".into();
    let second_key = MessageKey::from(&replay);
    router.enqueue_verified(replay).unwrap();
    assert_eq!(router.state(&first_key), Some(DeliveryStatus::Verified));
    assert_eq!(router.state(&second_key), Some(DeliveryStatus::Verified));
    assert_eq!(router.state(&rejected_key), None);
    router.begin_delivery(&second_key).unwrap();
    assert_eq!(router.state(&first_key), Some(DeliveryStatus::Verified));
    assert_eq!(router.state(&second_key), Some(DeliveryStatus::Delivering));
    assert_eq!(router.state(&rejected_key), None);
}
