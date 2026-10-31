//! Reproductions for the MCP 2026-07-28/2025-11-25 era boundaries.
//! These are local JSON-RPC protocol tests, not provider identity evidence.
use super::tests::{send, setup};
use super::*;
use std::fs;

fn metadata(version: &str) -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": version,
        "io.modelcontextprotocol/clientCapabilities": {}
    })
}

#[test]
fn modern_discover_and_list_emit_private_zero_ttl_cache_hints() {
    let (mut server, root) = setup();
    let meta = metadata(MODERN_VERSION);
    let discover = send(&mut server, json!({
        "jsonrpc": "2.0",
        "id": "discover",
        "method": "server/discover",
        "params": {"_meta": meta}
    }));
    assert_eq!(discover["result"]["resultType"], "complete");
    assert_eq!(discover["result"]["ttlMs"], 0);
    assert_eq!(discover["result"]["cacheScope"], "private");
    let list = send(&mut server, json!({
        "jsonrpc": "2.0",
        "id": "list",
        "method": "tools/list",
        "params": {"_meta": meta}
    }));
    assert_eq!(list["result"]["ttlMs"], 0);
    assert_eq!(list["result"]["cacheScope"], "private");
    assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 4);
    let tool = send(&mut server, json!({
        "jsonrpc": "2.0",
        "id": "tool",
        "method": "tools/call",
        "params": {
            "_meta": meta,
            "name": "cwg_binding_status",
            "arguments": {}
        }
    }));
    assert_eq!(tool["result"]["resultType"], "complete");
    assert!(tool["result"].get("ttlMs").is_none());
    assert_eq!(tool["result"]["structuredContent"]["provider_identity_verified"], false);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unsupported_modern_version_is_structured_and_does_not_poison_handshake() {
    let (mut server, root) = setup();
    let response = send(&mut server, json!({
        "jsonrpc": "2.0",
        "id": "bad-version",
        "method": "tools/list",
        "params": {"_meta": metadata("2027-01-01")}
    }));
    assert_eq!(response["error"]["code"], -32022);
    assert_eq!(response["error"]["data"]["requested"], "2027-01-01");
    assert_eq!(response["error"]["data"]["supported"], json!([MODERN_VERSION]));
    assert_eq!(server.phase, Phase::New);
    let accepted = send(&mut server, json!({
        "jsonrpc": "2.0",
        "id": "accepted",
        "method": "server/discover",
        "params": {"_meta": metadata(MODERN_VERSION)}
    }));
    assert_eq!(accepted["result"]["ttlMs"], 0);
    assert_eq!(server.phase, Phase::Modern);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_ping_is_valid_in_new_await_and_ready_but_does_not_change_phase() {
    let (mut server, root) = setup();
    let ping = json!({"jsonrpc": "2.0", "id": 1, "method": "ping"});
    assert_eq!(send(&mut server, ping.clone())["result"], json!({}));
    assert_eq!(server.phase, Phase::New);
    let init = send(&mut server, json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "initialize",
        "params": {
            "protocolVersion": LEGACY_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "local-test", "version": "1"}
        }
    }));
    assert_eq!(init["result"]["protocolVersion"], LEGACY_VERSION);
    assert_eq!(server.phase, Phase::LegacyAwaitInitialized);
    assert_eq!(send(&mut server, ping.clone())["result"], json!({}));
    assert_eq!(server.phase, Phase::LegacyAwaitInitialized);
    assert!(server.handle_json(br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
    assert_eq!(server.phase, Phase::LegacyReady);
    assert_eq!(send(&mut server, ping)["result"], json!({}));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn modern_ping_is_absent_even_before_discovery() {
    let (mut server, root) = setup();
    let ping = send(&mut server, json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "ping",
        "params": {"_meta": metadata(MODERN_VERSION)}
    }));
    assert_eq!(ping["error"]["code"], -32601);
    assert_eq!(server.phase, Phase::New);
    let discover = send(&mut server, json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "server/discover",
        "params": {"_meta": metadata(MODERN_VERSION)}
    }));
    assert_eq!(discover["result"]["resultType"], "complete");
    let again = send(&mut server, json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "ping",
        "params": {"_meta": metadata(MODERN_VERSION)}
    }));
    assert_eq!(again["error"]["code"], -32601);
    assert_eq!(server.phase, Phase::Modern);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn malformed_client_capabilities_do_not_claim_an_unsupported_version() {
    let (mut server, root) = setup();
    let invalid = json!({
        "io.modelcontextprotocol/protocolVersion": MODERN_VERSION,
        "io.modelcontextprotocol/clientCapabilities": null
    });
    let response = send(&mut server, json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "server/discover",
        "params": {"_meta": invalid}
    }));
    assert_eq!(response["error"]["code"], -32602);
    assert_eq!(server.phase, Phase::New);
    fs::remove_dir_all(root).unwrap();
}
