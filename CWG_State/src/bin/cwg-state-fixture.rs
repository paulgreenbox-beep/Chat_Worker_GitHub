//! Owned, synthetic subprocess fixture for native State tests. Not a receiver,
//! service or production authorization path. Every destructive mode is fenced
//! to an explicitly marked cwg-state-native-* test directory.
use cwg_protocol::{ChatTarget, GithubSource, Message};
use cwg_state::{Binding, Store, Transition};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

fn binding() -> Binding {
    Binding {
        id: "native-fixture".into(),
        repository_id: 10,
        owner_id: 20,
        repository: "owner/repo".into(),
        issue_number: 1,
        goal_id: "CWG-1".into(),
        generation: 1,
        account_id: "synthetic-operator".into(),
        target: ChatTarget {
            provider: "codex".into(),
            thread_id: "synthetic-thread".into(),
        },
    }
}
fn message(binding: &Binding) -> Message {
    Message {
        schema: "cwg-message-v1".into(),
        message_id: "gh:10:1".into(),
        source: GithubSource {
            repository: binding.repository.clone(),
            issue_number: 1,
            comment_id: 1,
            author_id: 7,
        },
        target: binding.target.clone(),
        goal_id: binding.goal_id.clone(),
        body: "Synthetic native State fixture".into(),
    }
}
fn verify_fixture(root: &Path, session: &str) -> Result<(), &'static str> {
    if !root.is_absolute()
        || !root
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("cwg-state-native-"))
        || session.is_empty()
        || session.len() > 128
        || !session
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("FIXTURE_SCOPE");
    }
    let marker = cwg_security::windows_acl::read_private_config(
        &root.join(".cwg-owned-native-fixture"),
        128,
    )?;
    if marker != session.as_bytes() {
        return Err("FIXTURE_SESSION");
    }
    Ok(())
}
fn ready(session: &str) -> Result<(), &'static str> {
    println!("READY:{}:{session}", std::process::id());
    std::io::stdout().flush().map_err(|_| "FIXTURE_STDOUT")
}
fn wait_for_parent() -> Result<(), &'static str> {
    let mut token = [0u8; 1];
    std::io::stdin()
        .read_exact(&mut token)
        .map_err(|_| "FIXTURE_STDIN")?;
    if token != *b"g" {
        return Err("FIXTURE_START");
    }
    Ok(())
}
#[cfg(windows)]
fn probe_read_lock(root: &Path) -> Result<i32, &'static str> {
    use std::{
        fs::{OpenOptions, TryLockError},
        os::windows::fs::OpenOptionsExt,
    };
    // Read-only sharing is a deliberate test probe: it can open alongside the
    // writer guard, so this proves byte-lock contention rather than open denial.
    let file = OpenOptions::new()
        .read(true)
        .share_mode(3)
        .custom_flags(0x0020_0000)
        .open(root.join("inbox.jsonl"))
        .map_err(|_| "FIXTURE_PROBE_OPEN")?;
    cwg_security::windows_acl::validate_private_file_handle(&file)?;
    match file.try_lock() {
        Ok(()) => {
            println!("LOCK_ACQUIRED");
            Ok(0)
        }
        Err(TryLockError::WouldBlock) => {
            println!("LOCK_BUSY");
            Ok(11)
        }
        Err(TryLockError::Error(_)) => Err("FIXTURE_PROBE_LOCK_ERROR"),
    }
}
#[cfg(not(windows))]
fn probe_read_lock(_: &Path) -> Result<i32, &'static str> {
    Err("WINDOWS_ACL_REQUIRED")
}
fn run() -> Result<i32, &'static str> {
    let mut args = std::env::args_os().skip(1);
    let mode = args
        .next()
        .and_then(|arg| arg.into_string().ok())
        .ok_or("FIXTURE_ARGS")?;
    let root = PathBuf::from(args.next().ok_or("FIXTURE_ARGS")?);
    let session = args
        .next()
        .and_then(|arg| arg.into_string().ok())
        .ok_or("FIXTURE_ARGS")?;
    if args.next().is_some() {
        return Err("FIXTURE_ARGS");
    }
    verify_fixture(&root, &session)?;
    let b = binding();
    match mode.as_str() {
        "init" => {
            let store = Store::open(&root)?;
            store.register(&b)?;
            if !store.put_verified(&b, "synthetic-delivery", message(&b))? {
                return Err("FIXTURE_ALREADY_INITIALIZED");
            }
            println!("INITIALIZED:{}:{session}", std::process::id());
        }
        "claim" => {
            ready(&session)?;
            wait_for_parent()?;
            match Store::open(&root)
                .and_then(|store| store.advance(&b, "gh:10:1", Transition::Begin))
            {
                Ok(()) => println!("CLAIMED"),
                Err(reason @ ("WRONG_STATE" | "STATE_BUSY_OR_LOCK_FAILED")) => {
                    println!("REFUSED:{reason}");
                    return Ok(11);
                }
                Err(reason) => return Err(reason),
            }
        }
        "claim-wait" => {
            let store = Store::open(&root)?;
            store.advance(&b, "gh:10:1", Transition::Begin)?;
            ready(&session)?;
            wait_for_parent()?;
        }
        "hold-lock" => {
            let mut guard = cwg_security::windows_acl::open_private_journal(&root)?;
            guard.file_mut().try_lock().map_err(|_| "FIXTURE_LOCK")?;
            ready(&session)?;
            wait_for_parent()?;
        }
        "append-partial" => {
            let mut guard = cwg_security::windows_acl::open_private_journal(&root)?;
            let file = guard.file_mut();
            file.try_lock().map_err(|_| "FIXTURE_LOCK")?;
            file.write_all(b"{\"schema\":\"cwg-journal-v1\",\"sequence\":")
                .map_err(|_| "FIXTURE_WRITE")?;
            file.sync_all().map_err(|_| "FIXTURE_SYNC")?;
            ready(&session)?;
            wait_for_parent()?;
        }
        "inspect" => {
            let store = Store::open(&root)?;
            let rows = store.inbox(&b, 1)?;
            let item = rows.first().ok_or("FIXTURE_EMPTY")?;
            println!("STATE:{:?}", item.status);
        }
        "unknown" => {
            Store::open(&root)?.advance(&b, "gh:10:1", Transition::Unknown)?;
            println!("UNKNOWN_RECORDED");
        }
        "probe-read-lock" => return probe_read_lock(&root),
        _ => return Err("FIXTURE_MODE"),
    }
    Ok(0)
}
fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(reason) => {
            eprintln!("STATE_ERROR:{reason}");
            std::process::exit(42);
        }
    }
}
