#![cfg(windows)]
#[path = "support/windows_fixture.rs"]
mod windows_fixture;
use cwg_security::windows_acl::{open_private_journal, read_private_config};
use cwg_state::Store;
use std::{fs, path::PathBuf};
use windows_fixture::FixtureGrant;
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        Self(windows_fixture::private_temp_dir("state-paths"))
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("release only owned path fixtures");
        assert!(!self.0.exists());
    }
}

#[test]
fn hardlinked_private_config_and_journal_are_refused_without_mutating_bytes() {
    let temp = Temp::new();
    let config = temp.0.join("config.json");
    fs::write(&config, b"synthetic config").unwrap();
    fs::hard_link(&config, temp.0.join("config-alias.json"))
        .expect("create actual owned NTFS hardlink fixture");
    assert_eq!(
        read_private_config(&config, 128).err(),
        Some("ACL_HARDLINK_REFUSED")
    );
    assert_eq!(fs::read(&config).unwrap(), b"synthetic config");
    drop(open_private_journal(&temp.0).unwrap());
    let journal = temp.0.join("inbox.jsonl");
    let bytes = fs::read(&journal).unwrap();
    fs::hard_link(&journal, temp.0.join("journal-alias.jsonl"))
        .expect("create actual owned journal hardlink");
    assert_eq!(
        open_private_journal(&temp.0).err(),
        Some("ACL_HARDLINK_REFUSED")
    );
    assert_eq!(Store::open(&temp.0).err(), Some("ACL_HARDLINK_REFUSED"));
    assert_eq!(fs::read(journal).unwrap(), bytes);
}

#[test]
fn native_junction_is_refused_as_config_ancestor_and_state_root() {
    let temp = Temp::new();
    let target = temp.0.join("real");
    fs::create_dir(&target).unwrap();
    windows_fixture::set_fixture_acl(&target, None).unwrap();
    fs::write(target.join("config.json"), b"synthetic config").unwrap();
    let junction = temp.0.join("junction");
    windows_fixture::create_fixture_junction(&junction, &target)
        .expect("create real owned Windows mount-point reparse fixture");
    assert_eq!(
        read_private_config(&junction.join("config.json"), 128).err(),
        Some("ACL_REPARSE_REFUSED")
    );
    assert_eq!(
        open_private_journal(&junction).err(),
        Some("ACL_REPARSE_REFUSED")
    );
    assert_eq!(Store::open(&junction).err(), Some("STATE_REPARSE_REFUSED"));
    assert_eq!(
        fs::read(target.join("config.json")).unwrap(),
        b"synthetic config"
    );
    assert!(!target.join("inbox.jsonl").exists());
}

#[test]
fn checked_ancestor_and_root_cannot_be_renamed_until_guard_is_released() {
    let temp = Temp::new();
    let parent = temp.0.join("parent");
    let root = parent.join("state");
    fs::create_dir(&parent).unwrap();
    fs::create_dir(&root).unwrap();
    windows_fixture::set_fixture_acl(&parent, None).unwrap();
    windows_fixture::set_fixture_acl(&root, None).unwrap();
    let guard = open_private_journal(&root).unwrap();
    let alternate_root = parent.join("state-renamed");
    let alternate_parent = temp.0.join("parent-renamed");
    assert!(
        fs::rename(&root, &alternate_root).is_err(),
        "held state-root handle must refuse replacement"
    );
    assert!(
        fs::rename(&parent, &alternate_parent).is_err(),
        "held ancestor handle must refuse replacement"
    );
    assert!(root.join("inbox.jsonl").exists());
    assert!(!alternate_root.exists());
    assert!(!alternate_parent.exists());
    drop(guard);
    fs::rename(&root, &alternate_root).expect("directory rename works after owned handles release");
    fs::rename(&alternate_root, &root).unwrap();
    fs::rename(&parent, &alternate_parent)
        .expect("ancestor rename works after owned handles release");
    fs::rename(&alternate_parent, &parent).unwrap();
}

#[test]
fn ancestor_allows_unrelated_creation_but_refuses_delete_child_rights() {
    let temp = Temp::new();
    let parent = temp.0.join("parent");
    let root = parent.join("state");
    fs::create_dir(&parent).unwrap();
    fs::create_dir(&root).unwrap();
    windows_fixture::set_fixture_acl(&root, None).unwrap();
    windows_fixture::set_fixture_acl(&parent, Some(FixtureGrant::EveryoneCreateOnly)).unwrap();
    drop(
        open_private_journal(&root)
            .expect("adding an unrelated child does not replace pinned existing entries"),
    );
    windows_fixture::set_fixture_acl(&parent, Some(FixtureGrant::EveryoneDeleteChild)).unwrap();
    assert_eq!(
        open_private_journal(&root).err(),
        Some("ACL_UNTRUSTED_ACCESS")
    );
    assert_eq!(Store::open(&root).err(), Some("ACL_UNTRUSTED_ACCESS"));
}

#[test]
fn actual_world_read_grant_on_config_is_refused() {
    let temp = Temp::new();
    let path = temp.0.join("config.json");
    fs::write(&path, b"synthetic private binding config").unwrap();
    windows_fixture::set_fixture_acl(&path, Some(FixtureGrant::EveryoneRead)).unwrap();
    assert_eq!(
        read_private_config(&path, 128).err(),
        Some("ACL_UNTRUSTED_ACCESS")
    );
    windows_fixture::set_fixture_acl(&path, None).unwrap();
    assert_eq!(
        read_private_config(&path, 128).unwrap(),
        b"synthetic private binding config"
    );
}
