//! Actual loopback HTTP + CNG + WinHTTP + protected durable journal integration.
//! All IDs, bodies and signing keys are public synthetic fixtures, not accounts.
use super::*;
use cwg_github::Config;
use cwg_state::Store;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[path = "../../CWG_State/tests/support/windows_fixture.rs"]
mod windows_fixture;

const CREATED: &[u8] = include_bytes!("../tests/fixtures/created.json");
const KEY: &[u8] = b"CWG synthetic test key only";
const CREATED_MAC: &str = "sha256=998575572cd0bc20b804b17d43b02e5941fb7572713ddd208d84b01a41501195";
// Independently generated with the existing Node/OpenSSL HMAC implementation
// over CREATED with both numeric author IDs changed from 7 to 8.
const UNAUTHORIZED_MAC: &str =
    "sha256=0556b3160290a5ee160e481041ed8bc41c1095833b25edb2109b9be2730cd391";

fn config_value(state: &Path) -> Value {
    let binding = json!({"id":"main-chat","repository_id":10,"owner_id":20,"repository":"owner/repo",
        "issue_number":1,"goal_id":"CWG-1","generation":1,"account_id":"account-one",
        "target":{"provider":"codex","thread_id":"registered-thread"}});
    let mut other = binding.clone();
    other["id"] = json!("neighbor");
    other["issue_number"] = json!(2);
    other["goal_id"] = json!("CWG-2");
    other["account_id"] = json!("account-two");
    other["target"]["thread_id"] = json!("neighbor-thread");
    json!({"schema":"cwg-github-receiver-v1","enabled":true,"listen":"127.0.0.1:8731",
        "state_directory":state,"webhook_secret_env":"CWG_WEBHOOK_SECRET","github_token_env":"CWG_GITHUB_TOKEN",
        "routes":[{"binding":binding,"author_ids":[7],"not_before":"2026-10-30T00:00:00Z","installation_id":99},
            {"binding":other,"author_ids":[7],"not_before":"2026-10-30T00:00:00Z","installation_id":99}]})
}
fn config(root: &Path) -> Config {
    let state = root.join("state");
    std::fs::create_dir(&state).unwrap();
    Config::from_bytes(&serde_json::to_vec(&config_value(&state)).unwrap()).unwrap()
}

fn read_header(stream: &mut TcpStream) -> Vec<u8> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut raw = Vec::new();
    let mut byte = [0u8; 1];
    while raw.len() < 16_384 && !raw.ends_with(b"\r\n\r\n") {
        assert_eq!(
            stream.read(&mut byte).unwrap(),
            1,
            "fixture request interrupted"
        );
        raw.push(byte[0]);
    }
    assert!(raw.ends_with(b"\r\n\r\n"));
    raw
}

struct ApiFixture {
    address: SocketAddr,
    calls: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}
impl ApiFixture {
    fn start() -> Self {
        let event: Value = serde_json::from_slice(CREATED).unwrap();
        let values: BTreeMap<String, Vec<u8>> = [
            ("/repos/owner/repo", event["repository"].clone()),
            ("/repos/owner/repo/issues/1", event["issue"].clone()),
            (
                "/repos/owner/repo/issues/comments/60",
                event["comment"].clone(),
            ),
            (
                "/repos/owner/repo/collaborators/alice/permission",
                json!({"permission":"write","user":event["sender"]}),
            ),
        ]
        .into_iter()
        .map(|(path, value)| (path.to_owned(), serde_json::to_vec(&value).unwrap()))
        .collect();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let counter = Arc::clone(&calls);
        let halt = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            while !halt.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(3));
                        continue;
                    }
                    Err(error) => panic!("owned API listener failed: {error}"),
                };
                let raw = read_header(&mut stream);
                let header = String::from_utf8(raw).unwrap();
                assert!(
                    !header.to_ascii_lowercase().contains("authorization:"),
                    "fixture must not use credentials"
                );
                let line = header.lines().next().unwrap();
                let path = line
                    .strip_prefix("GET ")
                    .and_then(|value| value.strip_suffix(" HTTP/1.1"))
                    .unwrap();
                let body = values
                    .get(path)
                    .expect("only registered fixture API paths may be requested");
                counter.fetch_add(1, Ordering::SeqCst);
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
                stream.write_all(body).unwrap();
            }
        });
        Self {
            address,
            calls,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for ApiFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}

struct ReceiverFixture {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Vec<Result<bool>>>>,
}
impl ReceiverFixture {
    fn start(mut cfg: Config, api: SocketAddr, count: usize) -> Self {
        let store = open_store(&cfg).unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        cfg.listen = address;
        let stop = Arc::new(AtomicBool::new(false));
        let halt = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            let mut outcomes = Vec::new();
            let end = Instant::now() + Duration::from_secs(8);
            while outcomes.len() < count && !halt.load(Ordering::SeqCst) && Instant::now() < end {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(3));
                        continue;
                    }
                    Err(error) => panic!("owned Receiver listener failed: {error}"),
                };
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let outcome = receive_one(
                    &mut stream,
                    &cfg,
                    &store,
                    KEY,
                    || windows::FixtureClient::new(api, Instant::now() + Duration::from_secs(2)),
                    Instant::now() + Duration::from_secs(2),
                );
                write_response(&mut stream, outcome).unwrap();
                outcomes.push(outcome);
            }
            outcomes
        });
        Self {
            address,
            stop,
            worker: Some(worker),
        }
    }
    fn finish(mut self) -> Vec<Result<bool>> {
        self.worker.take().unwrap().join().unwrap()
    }
}
impl Drop for ReceiverFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
fn post(address: SocketAddr, delivery: &str, mac: &str, body: &[u8]) -> String {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(4)))
        .unwrap();
    write!(stream, "POST /github HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nX-GitHub-Event: issue_comment\r\nX-GitHub-Delivery: {delivery}\r\nX-Hub-Signature-256: {mac}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
    stream.write_all(body).unwrap();
    stream.shutdown(Shutdown::Write).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

#[test]
fn native_receiver_end_to_end_uses_cng_winhttp_and_durable_scoped_inbox() {
    let root = windows_fixture::private_temp_dir("receiver-e2e");
    let cfg = config(&root);
    let api = ApiFixture::start();
    let receiver = ReceiverFixture::start(cfg.clone(), api.address, 4);
    assert!(
        post(receiver.address, "delivery-one", CREATED_MAC, CREATED)
            .starts_with("HTTP/1.1 202 Accepted")
    );
    assert_eq!(api.calls.load(Ordering::SeqCst), 4);
    assert!(
        post(receiver.address, "delivery-two", CREATED_MAC, CREATED).starts_with("HTTP/1.1 200 OK")
    );
    assert_eq!(api.calls.load(Ordering::SeqCst), 8);
    let unauthorized = String::from_utf8(CREATED.to_vec())
        .unwrap()
        .replace("\"id\":7", "\"id\":8");
    assert!(
        post(
            receiver.address,
            "delivery-unauthorized",
            UNAUTHORIZED_MAC,
            unauthorized.as_bytes()
        )
        .starts_with("HTTP/1.1 403 Forbidden")
    );
    assert_eq!(
        api.calls.load(Ordering::SeqCst),
        8,
        "unauthorized source must not call API"
    );
    let bad = "sha256=0000000000000000000000000000000000000000000000000000000000000000";
    assert!(
        post(receiver.address, "delivery-bad-mac", bad, CREATED)
            .starts_with("HTTP/1.1 403 Forbidden")
    );
    assert_eq!(
        receiver.finish(),
        vec![
            Ok(true),
            Ok(false),
            Err("ACTOR_OR_SOURCE_NOT_ALLOWED"),
            Err("SIGNATURE_MISMATCH")
        ]
    );
    let store = Store::open(&cfg.state_directory).unwrap();
    let rows = store.inbox(&cfg.routes[0].binding, 100).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].message.target, cfg.routes[0].binding.target);
    assert!(store.inbox(&cfg.routes[1].binding, 100).unwrap().is_empty());
    // Reopen Receiver/Store on the same protected journal; process restart is
    // independently exercised by State's owned native child-process tests.
    let restarted = ReceiverFixture::start(cfg.clone(), api.address, 1);
    assert!(
        post(
            restarted.address,
            "delivery-after-reopen",
            CREATED_MAC,
            CREATED
        )
        .starts_with("HTTP/1.1 200 OK")
    );
    assert_eq!(restarted.finish(), vec![Ok(false)]);
    assert_eq!(api.calls.load(Ordering::SeqCst), 12);
    assert_eq!(
        Store::open(&cfg.state_directory)
            .unwrap()
            .inbox(&cfg.routes[0].binding, 100)
            .unwrap()
            .len(),
        1
    );
    println!(
        "NATIVE_E2E: loopback HTTP + BCrypt + WinHTTP + protected fsync journal; accepted=1 duplicate=2 refused=2 neighbor=0"
    );
}

#[test]
fn native_owner_config_refuses_corruption_unprotected_acl_and_alias_paths() {
    use windows_fixture::{FixtureGrant, set_fixture_acl};
    let root = windows_fixture::private_temp_dir("receiver-config");
    let cfg = config(&root);
    let path = root.join("receiver.json");
    let raw = serde_json::to_vec(&config_value(&cfg.state_directory)).unwrap();
    std::fs::write(&path, &raw).unwrap();
    assert!(config_from_file(&path).is_ok());
    std::fs::write(&path, b"{\"bad\":\"PRIVATE_FIXTURE_VALUE\"").unwrap();
    let error = config_from_file(&path).unwrap_err();
    assert!(!error.contains("PRIVATE_FIXTURE_VALUE"));
    std::fs::write(&path, &raw).unwrap();
    assert!(config_from_file(&root.join("state").join("..").join("receiver.json")).is_err());
    assert!(config_from_file(&root.join("missing.json")).is_err());
    let alternate = PathBuf::from(format!("{}:alternate", path.display()));
    assert!(config_from_file(&alternate).is_err());
    set_fixture_acl(&path, Some(FixtureGrant::EveryoneRead)).unwrap();
    assert!(
        config_from_file(&path).is_err(),
        "readable-by-other-users owner config must be refused"
    );
}

#[test]
fn native_cng_and_cleanup_failures_are_service_errors_not_authorization_success() {
    for code in [
        "CNG_OPEN_FAILED",
        "CNG_CLOSE_FAILED",
        "CNG_HASH_AND_CLOSE_FAILED",
        "GITHUB_CLEANUP_UNKNOWN",
        "STATE_COMMIT_UNKNOWN",
    ] {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let reader = thread::spawn(move || {
            let mut stream = TcpStream::connect(addr).unwrap();
            let mut text = String::new();
            stream.read_to_string(&mut text).unwrap();
            text
        });
        let (mut stream, _) = listener.accept().unwrap();
        write_response(&mut stream, Err(code)).unwrap();
        drop(stream);
        assert!(
            reader
                .join()
                .unwrap()
                .starts_with("HTTP/1.1 503 Service Unavailable")
        );
    }
}
