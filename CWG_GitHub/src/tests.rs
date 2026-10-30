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
