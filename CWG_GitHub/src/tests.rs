use super::*;
use serde_json::{Value, json};
use std::cell::RefCell;

const FIXTURE: &[u8] = include_bytes!("../tests/fixtures/created.json");
struct Api { values: std::collections::BTreeMap<String, Vec<u8>>, calls: RefCell<Vec<String>> }
impl GithubApi for Api {
    fn get(&self, path: &str) -> Result<Vec<u8>> {
        self.calls.borrow_mut().push(path.into());
        self.values.get(path).cloned().ok_or("GITHUB_TEST_UNAVAILABLE")
    }
}
fn fixture() -> (Config, Api, Event) {
    let value: Value = serde_json::from_slice(FIXTURE).unwrap();
    let mut values = std::collections::BTreeMap::new();
    for (p, v) in [
        ("/repos/owner/repo", value["repository"].clone()),
        ("/repos/owner/repo/issues/1", value["issue"].clone()),
        ("/repos/owner/repo/issues/comments/60", value["comment"].clone()),
        ("/repos/owner/repo/collaborators/alice/permission", json!({"permission":"write","user":value["sender"]})),
    ] { values.insert(p.into(), serde_json::to_vec(&v).unwrap()); }
    let cfg = Config::from_bytes(&serde_json::to_vec(&json!({
        "schema":"cwg-github-receiver-v1", "enabled":true, "listen":"127.0.0.1:8731",
        "state_directory":std::env::temp_dir().join("cwg-unused-config-fixture"),
        "webhook_secret_env":"CWG_WEBHOOK_SECRET", "github_token_env":"CWG_GITHUB_TOKEN",
        "routes":[{"binding":{"id":"main-chat","repository_id":10,"owner_id":20,"repository":"owner/repo","issue_number":1,"goal_id":"CWG-1","generation":1,"account_id":"operator","target":{"provider":"codex","thread_id":"registered-thread"}},"author_ids":[7],"not_before":"2026-10-30T00:00:00Z","installation_id":99}]
    })).unwrap()).unwrap();
    (cfg, Api { values, calls: RefCell::new(Vec::new()) }, decode(FIXTURE, MAX_EVENT_BYTES).unwrap())
}
fn change(api: &mut Api, path: &str, f: impl FnOnce(&mut Value)) {
    let bytes = api.values.get_mut(path).unwrap(); let mut v: Value = serde_json::from_slice(bytes).unwrap(); f(&mut v); *bytes = serde_json::to_vec(&v).unwrap();
}
#[test] fn routes_from_config_not_task_text() {
    let (cfg, api, e) = fixture(); let (_, m) = reconcile(&cfg, &api, &e).unwrap();
    assert_eq!(m.target.thread_id, "registered-thread"); assert_eq!(m.message_id, "gh:10:60");
    assert_eq!(api.calls.borrow().len(), 4);
}
#[test] fn unauthorized_sender_causes_no_api_requests() {
    let (cfg, api, mut e) = fixture(); e.sender.id = 100;
    assert!(reconcile(&cfg, &api, &e).is_err()); assert!(api.calls.borrow().is_empty());
}
#[test] fn source_owner_transfer_is_refused() {
    let (cfg, mut api, e) = fixture(); change(&mut api, "/repos/owner/repo", |v| v["owner"]["id"] = json!(900));
    assert!(reconcile(&cfg, &api, &e).is_err());
}
#[test] fn recreated_repository_is_refused() {
    let (cfg, mut api, e) = fixture(); change(&mut api, "/repos/owner/repo", |v| v["id"] = json!(11));
    assert!(reconcile(&cfg, &api, &e).is_err());
}
#[test] fn revoked_actor_permission_is_refused() {
    let (cfg, mut api, e) = fixture(); change(&mut api, "/repos/owner/repo/collaborators/alice/permission", |v| v["permission"] = json!("read"));
    assert!(reconcile(&cfg, &api, &e).is_err());
}
#[test] fn permission_identity_must_match() {
    let (cfg, mut api, e) = fixture(); change(&mut api, "/repos/owner/repo/collaborators/alice/permission", |v| v["user"]["id"] = json!(8));
    assert!(reconcile(&cfg, &api, &e).is_err());
}
#[test] fn changed_or_edited_comment_is_not_delivered() {
    for field in ["body", "updated_at"] {
        let (cfg, mut api, e) = fixture(); change(&mut api, "/repos/owner/repo/issues/comments/60", |v| v[field] = json!("changed"));
        assert!(reconcile(&cfg, &api, &e).is_err());
    }
}
#[test] fn historical_comment_before_registration_is_refused() {
    let (mut cfg, api, e) = fixture(); cfg.routes[0].not_before = "2026-10-30T13:00:00Z".into(); assert!(reconcile(&cfg, &api, &e).is_err());
}
#[test] fn payload_url_is_not_used_as_an_api_destination() {
    let (cfg, api, mut e) = fixture(); e.comment.issue_url = "https://attacker.invalid/private".into();
    assert!(reconcile(&cfg, &api, &e).is_err()); assert!(api.calls.borrow().iter().all(|p| p.starts_with("/repos/owner/repo")));
}
#[test] fn api_errors_are_not_authorization() {
    let (cfg, mut api, e) = fixture(); api.values.clear(); assert!(reconcile(&cfg, &api, &e).is_err());
}
#[test] fn wrong_installation_and_bot_are_rejected() {
    let (cfg, api, mut e) = fixture(); e.installation = Some(Installation { id: 100 }); assert!(reconcile(&cfg, &api, &e).is_err());
    e.installation = Some(Installation { id: 99 }); e.sender.kind = "Bot".into(); assert!(reconcile(&cfg, &api, &e).is_err());
}
#[test] fn closed_locked_and_pr_threads_are_rejected() {
    for (field, value) in [("state", json!("closed")), ("locked", json!(true)), ("pull_request", json!({"url":"not-followed"}))] {
        let (cfg, mut api, e) = fixture(); change(&mut api, "/repos/owner/repo/issues/1", |v| v[field] = value);
        assert!(reconcile(&cfg, &api, &e).is_err());
    }
}
#[test] fn ambiguous_routes_and_public_listen_are_rejected() {
    let (mut cfg, _, _) = fixture(); cfg.routes.push(cfg.routes[0].clone()); assert!(cfg.validate().is_err());
    cfg.routes.pop(); cfg.listen = "0.0.0.0:8731".parse().unwrap(); assert!(cfg.validate().is_err());
}
#[test] fn calendar_cutoff_is_validated() {
    for s in ["", "2026-02-30T00:00:00Z", "2026-13-01T00:00:00Z", "2026-01-01T24:00:00Z"] { assert!(!github_time(s)); }
    assert!(github_time("2024-02-29T23:59:59Z"));
}

fn array_fields(value: &Value, fields: &[&str]) -> Value {
    Value::Array(fields.iter().map(|field| value[*field].clone()).collect())
}
fn config_value(cfg: &Config) -> Value {
    let routes: Vec<Value> = cfg.routes.iter().map(|route| json!({"binding":route.binding,"author_ids":route.author_ids,"not_before":route.not_before,"installation_id":route.installation_id})).collect();
    json!({"schema":cfg.schema,"enabled":cfg.enabled,"listen":cfg.listen.to_string(),"state_directory":cfg.state_directory,"webhook_secret_env":cfg.webhook_secret_env,"github_token_env":cfg.github_token_env,"routes":routes})
}
#[test]
fn audit_config_and_nested_routes_require_objects() {
    let (cfg, _, _) = fixture(); let value = config_value(&cfg);
    let array = array_fields(&value, &["schema","enabled","listen","state_directory","webhook_secret_env","github_token_env","routes"]);
    assert!(Config::from_bytes(&serde_json::to_vec(&array).unwrap()).is_err(), "positional Config accepted");
    let mut nested = value.clone();
    nested["routes"][0] = array_fields(&value["routes"][0], &["binding","author_ids","not_before","installation_id"]);
    assert!(Config::from_bytes(&serde_json::to_vec(&nested).unwrap()).is_err(), "positional Route accepted");
}
#[test]
fn audit_event_and_every_nested_identity_require_objects() {
    let original: Value = serde_json::from_slice(FIXTURE).unwrap();
    let mut variants = vec![("Event", array_fields(&original, &["action","repository","issue","comment","sender","installation"]))];
    for (field, fields) in [("repository", vec!["id","full_name","owner"]), ("issue", vec!["id","number","state","locked","pull_request"]),
        ("comment", vec!["id","body","user","issue_url","created_at","updated_at"]), ("sender", vec!["id","login","type"]), ("installation", vec!["id"])] {
        let mut value = original.clone(); value[field] = array_fields(&original[field], &fields); variants.push((field, value));
    }
    let mut owner = original.clone(); owner["repository"]["owner"] = array_fields(&original["repository"]["owner"], &["id","login","type"]); variants.push(("owner", owner));
    let mut author = original.clone(); author["comment"]["user"] = array_fields(&original["comment"]["user"], &["id","login","type"]); variants.push(("comment.user", author));
    for (name, value) in variants {
        assert!(decode_model::<Event>(&serde_json::to_vec(&value).unwrap(), MAX_EVENT_BYTES).is_err(), "positional {name} accepted");
    }
}
#[test]
fn audit_api_typed_arrays_are_refused() {
    let (cfg, mut api, event) = fixture();
    let path = "/repos/owner/repo/collaborators/alice/permission";
    change(&mut api, path, |value| *value = array_fields(value, &["permission","user"]));
    assert_eq!(reconcile(&cfg, &api, &event).err(), Some("JSON_SCHEMA"));
    let (cfg, mut api, event) = fixture();
    change(&mut api, path, |value| value["user"] = array_fields(&value["user"], &["id","login","type"]));
    assert_eq!(reconcile(&cfg, &api, &event).err(), Some("JSON_SCHEMA"));
}
macro_rules! api_rejection {
    ($name:ident, $path:expr, $change:expr, $code:expr) => {
        #[test] fn $name() {
            let (cfg, mut api, event) = fixture(); change(&mut api, $path, $change);
            assert_eq!(reconcile(&cfg, &api, &event).err(), Some($code));
        }
    };
}
api_rejection!(audit_api_repository_id, "/repos/owner/repo", |v| v["id"] = json!(11), "API_RECONCILIATION_FAILED");
api_rejection!(audit_api_repository_owner, "/repos/owner/repo", |v| v["owner"]["id"] = json!(21), "API_RECONCILIATION_FAILED");
api_rejection!(audit_api_repository_name, "/repos/owner/repo", |v| v["full_name"] = json!("other/repo"), "API_RECONCILIATION_FAILED");
api_rejection!(audit_api_issue_id, "/repos/owner/repo/issues/1", |v| v["id"] = json!(51), "API_RECONCILIATION_FAILED");
api_rejection!(audit_api_issue_number, "/repos/owner/repo/issues/1", |v| v["number"] = json!(2), "API_RECONCILIATION_FAILED");
api_rejection!(audit_api_issue_closed, "/repos/owner/repo/issues/1", |v| v["state"] = json!("closed"), "API_RECONCILIATION_FAILED");
api_rejection!(audit_api_issue_locked, "/repos/owner/repo/issues/1", |v| v["locked"] = json!(true), "API_RECONCILIATION_FAILED");
api_rejection!(audit_api_issue_is_pr, "/repos/owner/repo/issues/1", |v| v["pull_request"] = json!({"url":"ignored"}), "API_RECONCILIATION_FAILED");
api_rejection!(audit_api_comment_zero, "/repos/owner/repo/issues/comments/60", |v| v["id"] = json!(0), "API_RECONCILIATION_FAILED");
api_rejection!(audit_api_comment_id, "/repos/owner/repo/issues/comments/60", |v| v["id"] = json!(61), "API_RECONCILIATION_FAILED");
api_rejection!(audit_api_comment_author_id, "/repos/owner/repo/issues/comments/60", |v| v["user"]["id"] = json!(8), "API_RECONCILIATION_FAILED");
api_rejection!(audit_api_comment_author_login, "/repos/owner/repo/issues/comments/60", |v| v["user"]["login"] = json!("mallory"), "API_RECONCILIATION_FAILED");
api_rejection!(audit_api_comment_author_bot, "/repos/owner/repo/issues/comments/60", |v| v["user"]["type"] = json!("Bot"), "API_RECONCILIATION_FAILED");
api_rejection!(audit_api_comment_issue_url, "/repos/owner/repo/issues/comments/60", |v| v["issue_url"] = json!("https://attacker.invalid/issues/1"), "API_RECONCILIATION_FAILED");
api_rejection!(audit_api_comment_calendar, "/repos/owner/repo/issues/comments/60", |v| v["created_at"] = json!("2026-02-30T00:00:00Z"), "API_RECONCILIATION_FAILED");
api_rejection!(audit_api_comment_edited, "/repos/owner/repo/issues/comments/60", |v| v["updated_at"] = json!("2026-10-30T12:00:01Z"), "API_RECONCILIATION_FAILED");
api_rejection!(audit_api_comment_body, "/repos/owner/repo/issues/comments/60", |v| v["body"] = json!("changed"), "API_RECONCILIATION_FAILED");
api_rejection!(audit_api_permission_none, "/repos/owner/repo/collaborators/alice/permission", |v| v["permission"] = json!("none"), "CURRENT_PERMISSION_DENIED");
api_rejection!(audit_api_permission_read, "/repos/owner/repo/collaborators/alice/permission", |v| v["permission"] = json!("read"), "CURRENT_PERMISSION_DENIED");
api_rejection!(audit_api_permission_triage, "/repos/owner/repo/collaborators/alice/permission", |v| v["permission"] = json!("triage"), "CURRENT_PERMISSION_DENIED");
api_rejection!(audit_api_permission_unknown, "/repos/owner/repo/collaborators/alice/permission", |v| v["permission"] = json!("custom-role"), "CURRENT_PERMISSION_DENIED");
api_rejection!(audit_api_permission_id, "/repos/owner/repo/collaborators/alice/permission", |v| v["user"]["id"] = json!(8), "CURRENT_PERMISSION_DENIED");
api_rejection!(audit_api_permission_login, "/repos/owner/repo/collaborators/alice/permission", |v| v["user"]["login"] = json!("mallory"), "CURRENT_PERMISSION_DENIED");
api_rejection!(audit_api_permission_bot, "/repos/owner/repo/collaborators/alice/permission", |v| v["user"]["type"] = json!("Bot"), "CURRENT_PERMISSION_DENIED");

#[test]
fn audit_each_api_error_propagates_without_authorization_fallback() {
    for (index, path) in ["/repos/owner/repo", "/repos/owner/repo/issues/1", "/repos/owner/repo/issues/comments/60", "/repos/owner/repo/collaborators/alice/permission"].iter().enumerate() {
        let (cfg, mut api, event) = fixture(); api.values.remove(*path);
        assert_eq!(reconcile(&cfg, &api, &event).err(), Some("GITHUB_TEST_UNAVAILABLE"));
        assert_eq!(api.calls.borrow().len(), index + 1);
    }
}
#[cfg(windows)]
#[test] fn signed_fixture_commits_once_and_bad_mac_never_reads_api() {
    use std::time::{SystemTime, UNIX_EPOCH};
    let (mut cfg, api, _) = fixture();
    let dir = std::env::temp_dir().join(format!("cwg-ingress-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
    std::fs::create_dir(&dir).unwrap(); cfg.state_directory = dir.clone();
    let store = Store::open(&dir).unwrap(); store.register(&cfg.routes[0].binding).unwrap();
    let ingress = Ingress { config: &cfg, api: &api, store: &store, secret: b"CWG synthetic test key only" };
    assert!(ingress.receive("issue_comment", "d-1", "sha256=bad", FIXTURE).is_err()); assert!(api.calls.borrow().is_empty());
    let mac = "sha256=998575572cd0bc20b804b17d43b02e5941fb7572713ddd208d84b01a41501195";
    assert!(ingress.receive("issue_comment", "d-1", mac, FIXTURE).unwrap());
    assert!(!ingress.receive("issue_comment", "d-2", mac, FIXTURE).unwrap());
    assert_eq!(store.inbox(&cfg.routes[0].binding, 100).unwrap().len(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}
