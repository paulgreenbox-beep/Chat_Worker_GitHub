#![cfg(windows)]
#[path = "support/windows_fixture.rs"]
mod windows_fixture;
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, ChildStdout, Command, ExitStatus, Stdio},
    sync::mpsc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct Fixture {
    root: PathBuf,
    session: String,
}
impl Fixture {
    fn new() -> Self {
        let root = windows_fixture::private_temp_dir("state-native");
        let session = format!("{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos());
        fs::write(root.join(".cwg-owned-native-fixture"), &session).unwrap();
        let fixture = Self { root, session };
        let (status, output) = fixture.spawn("init").finish();
        assert!(status.success(), "native initialization must actually succeed: {output}");
        fixture
    }
    fn spawn(&self, mode: &str) -> OwnedChild {
        let executable = PathBuf::from(env!("CARGO_BIN_EXE_cwg-state-fixture")).canonicalize().unwrap();
        let child = Command::new(&executable).arg(mode).arg(&self.root).arg(&self.session)
            .current_dir(&self.root).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
            .spawn().expect("spawn exact Cargo-owned synthetic fixture executable");
        OwnedChild { child, stdout: None, session: self.session.clone() }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) { fs::remove_dir_all(&self.root).expect("remove only this fixture's owned root"); }
}
struct OwnedChild {
    child: Child,
    stdout: Option<BufReader<ChildStdout>>,
    session: String,
}
impl OwnedChild {
    fn ready(&mut self) {
        let stdout = self.child.stdout.take().unwrap();
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            let result = reader.read_line(&mut line);
            let _ = send.send((result, line, reader));
        });
        let (result, line, reader) = receive.recv_timeout(Duration::from_secs(10)).expect("owned fixture READY deadline");
        result.unwrap();
        assert_eq!(line.trim_end(), format!("READY:{}:{}", self.child.id(), self.session));
        self.stdout = Some(reader);
    }
    fn release(&mut self) {
        self.child.stdin.as_mut().unwrap().write_all(b"g").unwrap();
    }
    fn wait(&mut self) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() { return status; }
            assert!(Instant::now() < deadline, "owned fixture exit deadline");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn kill_and_wait(&mut self) {
        // Child owns the native process handle created above, so this cannot
        // target a guessed PID, another executable or a PID reused later.
        self.child.kill().expect("terminate only this owned fixture process");
        self.child.wait().expect("verify owned process exit");
    }
    fn finish(mut self) -> (ExitStatus, String) {
        let status = self.wait();
        let mut output = String::new();
        if let Some(mut reader) = self.stdout.take() { reader.read_to_string(&mut output).unwrap(); }
        else { self.child.stdout.take().unwrap().read_to_string(&mut output).unwrap(); }
        self.child.stderr.take().unwrap().read_to_string(&mut output).unwrap();
        (status, output)
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            self.child.kill().expect("cleanup only owned fixture child");
            self.child.wait().expect("verify owned child cleanup");
        }
    }
}

#[test]
fn committed_claim_survives_owned_process_termination_without_retry() {
    let fixture = Fixture::new();
    let mut claimant = fixture.spawn("claim-wait"); claimant.ready(); claimant.kill_and_wait();
    let (status, output) = fixture.spawn("inspect").finish();
    assert!(status.success(), "{output}"); assert_eq!(output.trim(), "STATE:Delivering");
    let mut replay = fixture.spawn("claim"); replay.ready(); replay.release();
    let (status, output) = replay.finish(); assert_eq!(status.code(), Some(11)); assert_eq!(output.trim(), "REFUSED:WRONG_STATE");
    let (status, output) = fixture.spawn("unknown").finish(); assert!(status.success(), "{output}");
    let (status, output) = fixture.spawn("inspect").finish(); assert!(status.success(), "{output}"); assert_eq!(output.trim(), "STATE:Unknown");
    let mut replay = fixture.spawn("claim"); replay.ready(); replay.release();
    let (status, output) = replay.finish(); assert_eq!(status.code(), Some(11)); assert_eq!(output.trim(), "REFUSED:WRONG_STATE");
}

#[test]
fn independent_native_process_claims_have_exactly_one_winner() {
    let fixture = Fixture::new();
    let mut children: Vec<_> = (0..8).map(|_| fixture.spawn("claim")).collect();
    for child in &mut children { child.ready(); }
    for child in &mut children { child.release(); }
    let results: Vec<_> = children.into_iter().map(OwnedChild::finish).collect();
    assert_eq!(results.iter().filter(|(status, _)| status.success()).count(), 1);
    for (status, output) in results {
        if status.success() { assert_eq!(output.trim(), "CLAIMED"); }
        else { assert_eq!(status.code(), Some(11), "{output}"); assert!(matches!(output.trim(), "REFUSED:WRONG_STATE" | "REFUSED:STATE_BUSY_OR_LOCK_FAILED")); }
    }
}

#[test]
fn native_os_lock_blocks_another_process_and_is_released_on_owned_exit() {
    let fixture = Fixture::new();
    let mut holder = fixture.spawn("hold-lock"); holder.ready();
    let (status, output) = fixture.spawn("inspect").finish();
    assert_eq!(status.code(), Some(42)); assert_eq!(output.trim(), "STATE_ERROR:STATE_BUSY_OR_LOCK_FAILED");
    holder.kill_and_wait();
    let (status, output) = fixture.spawn("inspect").finish();
    assert!(status.success(), "{output}"); assert_eq!(output.trim(), "STATE:Verified");
}

#[test]
fn partial_native_append_survives_exit_and_is_never_truncated_or_repaired() {
    let fixture = Fixture::new(); let journal = fixture.root.join("inbox.jsonl"); let original = fs::read(&journal).unwrap();
    let mut writer = fixture.spawn("append-partial"); writer.ready(); writer.kill_and_wait();
    let raw = fs::read(&journal).unwrap(); assert!(raw.starts_with(&original)); assert!(raw.len() > original.len());
    let (status, output) = fixture.spawn("inspect").finish();
    assert_eq!(status.code(), Some(42)); assert_eq!(output.trim(), "STATE_ERROR:JOURNAL_CORRUPT_OR_TOO_LARGE");
    assert_eq!(fs::read(journal).unwrap(), raw);
}
