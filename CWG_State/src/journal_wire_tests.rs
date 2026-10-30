//! Pure parser tests; no filesystem, token, network or deployment claim.
use super::*;
use serde_json::{Value, json};

const TRANSITIONS: [(Transition, &str); 6] = [
    (Transition::Begin, "BEGIN"),
    (Transition::Accepted, "ACCEPTED"),
    (Transition::Unknown, "UNKNOWN"),
    (Transition::Acknowledge, "ACKNOWLEDGE"),
    (Transition::Complete, "COMPLETE"),
    (Transition::Block, "BLOCK"),
];

fn record(transition: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schema": "cwg-journal-v1",
        "sequence": 1,
        "event": {
            "kind": "Advance",
            "binding_id": "binding-1",
            "message_id": "gh:1:2",
            "transition": transition
        }
    }))
    .unwrap()
}

#[test]
fn all_six_canonical_strings_keep_the_existing_writer_format() {
    for (transition, name) in TRANSITIONS {
        let encoded = serde_json::to_vec(&transition).unwrap();
        assert_eq!(encoded, serde_json::to_vec(name).unwrap());
        assert_eq!(
            serde_json::from_slice::<Transition>(&encoded).unwrap(),
            transition
        );
        let decoded: Record = cwg_security::decode(&record(json!(name)), MAX_RECORD).unwrap();
        match decoded.event {
            Event::Advance {
                transition: actual, ..
            } => assert_eq!(actual, transition),
            _ => panic!("wrong event variant"),
        }
    }
}

#[test]
fn object_null_aliases_are_refused_directly_and_inside_a_journal_record() {
    for (_, name) in TRANSITIONS {
        // Build raw JSON instead of a pre-decoded map, exercising both boundaries.
        let object = format!("{{\"{name}\":null}}");
        assert!(serde_json::from_str::<Transition>(&object).is_err());
        assert!(
            cwg_security::decode::<Transition>(object.as_bytes(), MAX_RECORD).is_err()
        );
        let nested = record(serde_json::from_str(&object).unwrap());
        assert!(cwg_security::decode::<Record>(&nested, MAX_RECORD).is_err());
    }
}

#[test]
fn other_noncanonical_values_cannot_reach_journal_replay() {
    for value in [
        Value::Null,
        json!(true),
        json!(1),
        json!([]),
        json!(["BEGIN"]),
        json!("begin"),
        json!(" BEGIN"),
        json!("BEGIN\n"),
        json!("DELIVERING"),
        json!(""),
    ] {
        assert!(
            cwg_security::decode::<Record>(&record(value), MAX_RECORD).is_err()
        );
    }
}

#[test]
fn transition_parse_failure_does_not_apply_or_normalize_an_event() {
    let invalid = record(json!({ "BEGIN": null }));
    let mut view = View::default();
    let before = (view.sequence, view.bindings.len(), view.messages.len());
    let decoded = cwg_security::decode::<Record>(&invalid, MAX_RECORD);
    assert!(decoded.is_err());
    if let Ok(record) = decoded {
        let _ = view.apply(&record.event);
    }
    assert_eq!((view.sequence, view.bindings.len(), view.messages.len()), before);
}
