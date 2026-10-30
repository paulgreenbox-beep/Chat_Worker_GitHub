//! Owned, synthetic subprocess fixture for native State tests. Not a receiver,
//! service or production authorization path. Every destructive mode is fenced
//! to an explicitly marked cwg-state-native-* test directory.
use cwg_protocol::{ChatTarget, GithubSource, Message};
use cwg_state::{Binding, Store, Transition};
use std::{io::{Read, Write}, path::{Path, PathBuf}};

fn binding() -> Binding {
    Binding { id: "native-fixture".into(), repository_id: 10, owner_id: 20,
        repository: "owner/repo".into(), issue_number: 1, goal_id: "CWG-1".into(),
        generation: 1, account_id: "synthetic-operator".into(),
        target: ChatTarget { provider: "codex".into(), thread_id: "synthetic-thread".into() } }
}
fn message(binding: &Binding) -> Message {
    Message { schema: "cwg-message-v1".into(), message_id: "gh:10:1".into(),
        source: GithubSource { repository: binding.repository.clone(), issue_number: 1,
            comment_id: 1, author_id: 7 }, target: binding.target.clone(),
        goal_id: binding.goal_id.clone(), body: "Synthetic native State fixture".into() }
}
fn verify_fixture(root: &Path, session: &str) -> Result<(), &'static str> {
    if !root.is_absolute() || !root.file_name().is_some_and(|name| name.to_string_lossy().starts_with("cwg-state-native-"))
        || session.is_empty() || session.len() > 128 || !session.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err("FIXTURE_SCOPE");
    }
    let marker = cwg_security::windows_acl::read_private_config(&root.join(".cwg-owned-native-fixture"), 128)?;
    if marker != session.as_bytes() { return Err("FIXTURE_SESSION"); }
    Ok(())
}
fn ready(session: &str) -> Result<(), &'static str> {
    println!("READY:{}:{session}", std::process::id());
    std::io::stdout().flush().map_err(|_| "FIXTURE_STDOUT")
}
fn wait_for_parent() -> Result<(), &'static str> {
    let mut token = [0u8; 1];
    std::io::stdin().read_exact(&mut token).map_err(|_| "FIXTURE_STDIN")?;
    if token != [b'g'] { return Err("FIXTURE_START"); }
    Ok(())
}
fn run() -> Result<i32, &'static str> {
    let mut args = std::env::args_os().skip(1);
    let mode = args.next().and_then(|arg| arg.into_string().ok()).ok_or("FIXTURE_ARGS")?;
    let root = PathBuf::from(args.next().ok_or("FIXTURE_ARGS")?);
    let session = args.next().and_then(|arg| arg.into_string().ok()).ok_or("FIXTURE_ARGS")?;
    if args.next().is_some() { return Err("FIXTURE_ARGS"); }
    verify_fixture(&root, &session)?;
    let b = binding();
    let store = Store::open(&root)?;
    match mode.as_str() {
        "init" => {
            store.register(&b)?;
            if !store.put_verified(&b, "synthetic-delivery", message(&b))? { return Err("FIXTURE_ALREADY_INITIALIZED"); }
            println!("INITIALIZED:{}:{session}", std::process::id());
        }
        "claim" => {
            ready(&session)?; wait_for_parent()?;
            match store.advance(&b, "gh:10:1", Transition::Begin) {
                Ok(()) => println!("CLAIMED"),
                Err(reason @ ("WRONG_STATE" | "STATE_BUSY_OR_LOCK_FAILED")) => {
                    println!("REFUSED:{reason}"); return Ok(11);
                }
                Err(reason) => return Err(reason),
            }
        }
        "claim-wait" => {
            store.advance(&b, "gh:10:1", Transition::Begin)?;
            ready(&session)?; wait_for_parent()?;
        }
        "hold-lock" => {
            let mut guard = cwg_security::windows_acl::open_private_journal(&root)?;
            guard.file_mut().try_lock().map_err(|_| "FIXTURE_LOCK")?;
            ready(&session)?; wait_for_parent()?;
        }
        "append-partial" => {
            let mut guard = cwg_security::windows_acl::open_private_journal(&root)?;
            let file = guard.file_mut();
            file.try_lock().map_err(|_| "FIXTURE_LOCK")?;
            file.write_all(b"{\"schema\":\"cwg-journal-v1\",\"sequence\":").map_err(|_| "FIXTURE_WRITE")?;
            file.sync_all().map_err(|_| "FIXTURE_SYNC")?;
            ready(&session)?; wait_for_parent()?;
        }
        "inspect" => {
            let rows = store.inbox(&b, 1)?;
            let item = rows.first().ok_or("FIXTURE_EMPTY")?;
            println!("STATE:{:?}", item.status);
        }
        "unknown" => { store.advance(&b, "gh:10:1", Transition::Unknown)?; println!("UNKNOWN_RECORDED"); }
        _ => return Err("FIXTURE_MODE"),
    }
    Ok(0)
}
fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(reason) => { eprintln!("STATE_ERROR:{reason}"); std::process::exit(42); }
    }
}
