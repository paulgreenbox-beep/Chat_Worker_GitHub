use cwg_protocol::DeliveryStatus;
use serde_json::{Map, Value};

#[test]
fn delivery_status_accepts_only_exact_string_wire_values() {
    let cases = [
        (DeliveryStatus::Pending, "PENDING"),
        (DeliveryStatus::Verified, "VERIFIED"),
        (DeliveryStatus::Delivering, "DELIVERING"),
        (DeliveryStatus::Delivered, "DELIVERED"),
        (DeliveryStatus::Acknowledged, "ACKNOWLEDGED"),
        (DeliveryStatus::Completed, "COMPLETED"),
        (DeliveryStatus::Blocked, "BLOCKED"),
        (DeliveryStatus::Unknown, "UNKNOWN"),
    ];

    // Preserve every existing uppercase string representation and its round trip
    // before testing the alternative externally tagged object representation.
    for &(status, name) in &cases {
        let expected = Value::String(name.to_owned());
        let raw = serde_json::to_vec(&status).unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&raw).unwrap(), expected);
        assert_eq!(
            serde_json::from_slice::<DeliveryStatus>(&raw).unwrap(),
            status
        );
        assert_eq!(
            serde_json::from_value::<DeliveryStatus>(expected).unwrap(),
            status
        );
    }

    // Exercise both decoding paths for all variants, even on the pre-fix
    // implementation, rather than stopping at the first accepted object.
    let mut accepted_objects = Vec::new();
    for &(_, name) in &cases {
        let mut object = Map::new();
        object.insert(name.to_owned(), Value::Null);
        let value = Value::Object(object);
        let raw = serde_json::to_vec(&value).unwrap();
        if serde_json::from_slice::<DeliveryStatus>(&raw).is_ok() {
            accepted_objects.push(format!("{name}:raw"));
        }
        if serde_json::from_value::<DeliveryStatus>(value).is_ok() {
            accepted_objects.push(format!("{name}:value"));
        }
    }
    assert!(
        accepted_objects.is_empty(),
        "DeliveryStatus must use a JSON string, accepted objects: {accepted_objects:?}"
    );
}
