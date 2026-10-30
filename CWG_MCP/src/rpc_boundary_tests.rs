//! Protocol-boundary regression tests. These do not authenticate a Codex chat.
use super::tests::{init, send, setup};
use super::*;

fn metadata() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": MODERN_VERSION,
        "io.modelcontextprotocol/clientCapabilities": {}
    })
}

fn initialize_request(info: Value) -> Value {
    json!({"jsonrpc":"2.0", "id":1, "method":"initialize", "params":{
        "protocolVersion": LEGACY_VERSION,
        "capabilities": {},
        "clientInfo": info
    }})
}

#[test]
fn initialized_notification_with_bad_params_cannot_open_the_tool_gate() {
    let (mut server, root) = setup();
    let response = send(&mut server, initialize_request(json!({"name":"test","version":"1"})));
    assert!(response.get("error").is_none());
    for invalid in [json!(null), json!([]), json!(1), json!("text"),
        json!({"_meta":[]}), json!({"_meta":null}), json!({"extra":true})] {
        let notification = json!({"jsonrpc":"2.0", "method":"notifications/initialized", "params":invalid});
        assert!(server.handle_json(&serde_json::to_vec(&notification).unwrap()).is_none());
        assert_eq!(server.phase, Phase::LegacyAwaitInitialized);
        let blocked = send(&mut server, json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}));
        assert_eq!(blocked["error"]["code"], -32002);
    }
    assert!(server.handle_json(br#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{"_meta":{}}}"#).is_none());
    assert_eq!(server.phase, Phase::LegacyReady);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn explicit_nonobject_list_params_do_not_mean_omitted_params() {
    let (mut server, root) = setup();
    init(&mut server);
    for invalid in [json!(null), json!([]), json!(true), json!(0), json!("text"),
        json!({"cursor":null}), json!({"cursor":[]}), json!({"_meta":false})] {
        let result = send(&mut server, json!({"jsonrpc":"2.0","id":3,"method":"tools/list","params":invalid}));
        assert_eq!(result["error"]["code"], -32602);
        assert_eq!(server.phase, Phase::LegacyReady);
    }
    for params in [json!({}), json!({"_meta":{"extension":true}})] {
        let result = send(&mut server, json!({"jsonrpc":"2.0","id":3,"method":"tools/list","params":params}));
        assert!(result.get("error").is_none());
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn malformed_first_modern_list_cannot_lock_out_a_legacy_handshake() {
    let (mut server, root) = setup();
    let result = send(&mut server, json!({"jsonrpc":"2.0","id":4,"method":"tools/list","params":{
        "_meta":metadata(), "unexpected":true
    }}));
    assert_eq!(result["error"]["code"], -32602);
    assert_eq!(server.phase, Phase::New);
    init(&mut server);
    assert_eq!(server.phase, Phase::LegacyReady);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn malformed_first_modern_tool_envelope_cannot_change_protocol_phase() {
    let (mut server, root) = setup();
    for params in [
        json!({"_meta":metadata()}),
        json!({"_meta":metadata(),"name":7}),
        json!({"_meta":metadata(),"name":"unknown"}),
        json!({"_meta":metadata(),"name":"cwg_binding_status","arguments":[]}),
        json!({"_meta":metadata(),"name":"cwg_binding_status","extra":true}),
    ] {
        let result = send(&mut server, json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":params}));
        assert_eq!(result["error"]["code"], -32602);
        assert_eq!(server.phase, Phase::New);
    }
    let accepted = send(&mut server, json!({"jsonrpc":"2.0","id":6,"method":"tools/list","params":{"_meta":metadata()}}));
    assert!(accepted.get("error").is_none());
    assert_eq!(accepted["result"]["resultType"], "complete");
    assert_eq!(server.phase, Phase::Modern);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn required_client_info_fields_are_checked_before_handshake_mutation() {
    let (mut server, root) = setup();
    for invalid in [json!({}), json!({"name":"test"}), json!({"version":"1"}),
        json!({"name":7,"version":"1"}), json!({"name":"test","version":null}), json!([])] {
        let response = send(&mut server, initialize_request(invalid));
        assert_eq!(response["error"]["code"], -32602);
        assert_eq!(server.phase, Phase::New);
    }
    init(&mut server);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn bad_optional_modern_client_info_does_not_establish_the_modern_era() {
    let (mut server, root) = setup();
    for invalid in [json!({}), json!({"name":false,"version":"1"}), json!(null)] {
        let mut meta = metadata();
        meta["io.modelcontextprotocol/clientInfo"] = invalid;
        let response = send(&mut server, json!({"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta":meta}}));
        assert_eq!(response["error"]["code"], -32602);
        assert_eq!(server.phase, Phase::New);
    }
    let mut meta = metadata();
    meta["io.modelcontextprotocol/clientInfo"] = json!({"name":"test","version":"1","title":"display"});
    let response = send(&mut server, json!({"jsonrpc":"2.0","id":2,"method":"server/discover","params":{"_meta":meta}}));
    assert!(response.get("error").is_none());
    assert_eq!(server.phase, Phase::Modern);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_tool_call_rejects_nonobject_metadata_without_a_tool_effect() {
    let (mut server, root) = setup();
    init(&mut server);
    for meta in [json!(null), json!([]), json!(1), json!(true)] {
        let response = send(&mut server, json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{
            "name":"cwg_report_progress", "_meta":meta,
            "arguments":{"message_id":"gh:11:55","report_id":"bad-meta","kind":"progress","body":"untrusted"}
        }}));
        assert_eq!(response["error"]["code"], -32602);
        assert!(!root.join("worker-reports.jsonl").exists());
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn empty_method_params_accept_only_omission_or_metadata_objects() {
    assert!(validate_empty_params(None).is_ok());
    for good in [json!({}), json!({"_meta":{}}), json!({"_meta":{"custom":[]}})] {
        assert!(validate_empty_params(Some(&good)).is_ok());
    }
    for bad in [json!(null), json!([]), json!(false), json!(""), json!({"_meta":null}), json!({"extra":1})] {
        assert!(validate_empty_params(Some(&bad)).is_err());
    }
}
