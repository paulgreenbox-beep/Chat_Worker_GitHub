#![cfg(windows)]
#[path = "support/windows_fixture.rs"]
mod windows_fixture;
use cwg_security::windows_acl::{open_private_journal, read_private_config, validate_private_file_handle};
use std::{fs::{self, OpenOptions}, os::windows::fs::OpenOptionsExt, path::PathBuf};
use windows_fixture::{FixtureGrant, WritableMapping};
struct Temp(PathBuf);
impl Temp { fn new() -> Self { Self(windows_fixture::private_temp_dir("retained-access")) } }
impl Drop for Temp { fn drop(&mut self) { fs::remove_dir_all(&self.0).expect("remove only owned retained-access fixture"); } }

#[test]
fn security_regression_prior_writer_config_is_refused_after_acl_tightening() {
    let temp = Temp::new(); let path = temp.0.join("config.json"); fs::write(&path,b"synthetic configuration").unwrap();
    windows_fixture::set_fixture_acl(&path,Some(FixtureGrant::EveryoneFull)).unwrap();
    let writer = OpenOptions::new().read(true).write(true).share_mode(3).open(&path).unwrap();
    windows_fixture::set_fixture_acl(&path,None).unwrap();
    // Real DACL is now private, but Windows retains the earlier writer's access.
    validate_private_file_handle(&writer).unwrap();
    assert!(read_private_config(&path,128).is_err(), "private DACL alone must not admit a previously granted data writer");
    drop(writer);
    assert_eq!(read_private_config(&path,128).unwrap(), b"synthetic configuration");
}

#[test]
fn security_regression_writable_mapping_journal_is_refused_after_writer_closes() {
    let temp=Temp::new(); let path=temp.0.join("inbox.jsonl"); fs::write(&path,b"synthetic journal bytes").unwrap();
    let mut mapping=WritableMapping::new(&path).unwrap();
    windows_fixture::set_fixture_acl(&path,None).unwrap();
    let control=OpenOptions::new().read(true).share_mode(3).open(&path).unwrap();
    validate_private_file_handle(&control).unwrap(); drop(control);
    mapping.write_first_byte(b'S').unwrap();
    assert_eq!(fs::read(&path).unwrap()[0],b'S');
    assert!(open_private_journal(&temp.0).is_err(), "a live writable mapping retains write authority after its original file handle closes");
    drop(mapping);
    drop(open_private_journal(&temp.0).unwrap());
}
