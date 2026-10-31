//! Native Windows MCP file protections: owned fixtures only, no live credentials.
#![cfg(windows)]
use super::tests::setup;
use super::*;
use crate::windows_fixture::{self, FixtureGrant, WritableMapping};
use std::{
    fs::{self, OpenOptions},
    os::windows::fs::OpenOptionsExt,
};

fn owner_config(root: &Path) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schema": "cwg-github-receiver-v1",
        "enabled": true,
        "listen": "127.0.0.1:8731",
        "state_directory": root,
        "webhook_secret_env": "CWG_WEBHOOK_SECRET",
        "github_token_env": "CWG_GITHUB_TOKEN",
        "routes": [{
            "binding": {
                "id": "agent-one",
                "repository_id": 11,
                "owner_id": 22,
                "repository": "owner/repo",
                "issue_number": 7,
                "goal_id": "CWG-001",
                "generation": 1,
                "account_id": "account-one",
                "target": {"provider": "codex", "thread_id": "thread-abc"}
            },
            "author_ids": [22],
            "not_before": "2026-10-30T00:00:00Z",
            "installation_id": null
        }]
    }))
    .unwrap()
}

#[test]
fn owner_config_opens_only_as_a_private_file_and_rejects_links() {
    let (original, root) = setup();
    let config = root.join("receiver.json");
    fs::write(&config, owner_config(&root)).unwrap();
    let server = Server::open(&config, "agent-one").unwrap();
    assert_eq!(server.binding, original.binding);
    assert_eq!(server.phase, Phase::New);

    windows_fixture::set_fixture_acl(&config, Some(FixtureGrant::Read)).unwrap();
    assert!(Server::open(&config, "agent-one").is_err());
    windows_fixture::set_fixture_acl(&config, None).unwrap();
    assert_eq!(Server::open(&config, "agent-one").unwrap().binding, original.binding);

    let alias = root.join("receiver-hardlink.json");
    fs::hard_link(&config, &alias).unwrap();
    assert!(Server::open(&config, "agent-one").is_err());
    fs::remove_file(&alias).unwrap();
    assert_eq!(Server::open(&config, "agent-one").unwrap().binding, original.binding);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn private_report_journal_refuses_retained_writer_and_hardlink() {
    let (server, root) = setup();
    let path = root.join("worker-reports.jsonl");
    let binding = server.binding.clone();
    assert_eq!(
        outbox::append(&root, &binding, "gh:11:55", "r1", "progress", "start"),
        Ok(true)
    );
    windows_fixture::set_fixture_acl(&path, Some(FixtureGrant::Full)).unwrap();
    let writer = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(3)
        .open(&path)
        .unwrap();
    windows_fixture::set_fixture_acl(&path, None).unwrap();
    assert_eq!(
        outbox::append(&root, &binding, "gh:11:55", "r2", "progress", "next"),
        Err("REPORT_PRIVATE_FILE_REQUIRED")
    );
    drop(writer);
    assert_eq!(
        outbox::append(&root, &binding, "gh:11:55", "r2", "progress", "next"),
        Ok(true)
    );

    let alias = root.join("report-hardlink.jsonl");
    fs::hard_link(&path, &alias).unwrap();
    assert_eq!(
        outbox::append(&root, &binding, "gh:11:55", "r3", "progress", "more"),
        Err("REPORT_PRIVATE_FILE_REQUIRED")
    );
    fs::remove_file(&alias).unwrap();
    assert_eq!(
        outbox::append(&root, &binding, "gh:11:55", "r3", "progress", "more"),
        Ok(true)
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn writable_mapping_blocks_report_journal_even_after_writer_closes() {
    let (server, root) = setup();
    let binding = server.binding.clone();
    assert_eq!(
        outbox::append(&root, &binding, "gh:11:55", "r1", "progress", "start"),
        Ok(true)
    );
    let path = root.join("worker-reports.jsonl");
    let mapping = WritableMapping::new(&path).unwrap();
    windows_fixture::set_fixture_acl(&path, None).unwrap();
    assert_eq!(
        outbox::append(&root, &binding, "gh:11:55", "r2", "progress", "next"),
        Err("REPORT_PRIVATE_FILE_REQUIRED")
    );
    drop(mapping);
    assert_eq!(
        outbox::append(&root, &binding, "gh:11:55", "r2", "progress", "next"),
        Ok(true)
    );
    fs::remove_dir_all(root).unwrap();
}
