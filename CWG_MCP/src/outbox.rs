//! Durable, data-only worker reports. This is NOT an authenticated GitHub
//! publisher, a task completion oracle, or a second delivery state machine.
use cwg_state::Binding;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_FILE: u64 = 8 * 1024 * 1024;
const MAX_RECORD: usize = 12 * 1024;
const MAX_REPORTS: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Report {
    schema: String,
    binding_id: String,
    repository_id: u64,
    generation: u64,
    message_id: String,
    report_id: String,
    kind: String,
    body: String,
    created_at_unix_ms: u64,
}

fn token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_:".contains(&b))
}

fn validate(record: &Report) -> Result<(), &'static str> {
    if record.schema != "cwg-worker-report-v1"
        || record.repository_id == 0
        || record.generation == 0
        || !token(&record.binding_id)
        || !token(&record.message_id)
        || !token(&record.report_id)
        || !matches!(record.kind.as_str(), "progress" | "result")
        || record.body.is_empty()
        || record.body.len() > 8192
        || record.created_at_unix_ms == 0
    {
        return Err("REPORT_INVALID");
    }
    Ok(())
}

// Reject observed links and Windows reparse points, including parent paths.
// Filesystem ACLs and hostile same-user races still require independent review.
fn no_links(path: &Path) -> Result<(), &'static str> {
    for part in path.ancestors() {
        match fs::symlink_metadata(part) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    return Err("REPORT_LINK_REFUSED");
                }
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if meta.file_attributes() & 0x400 != 0 {
                        return Err("REPORT_REPARSE_REFUSED");
                    }
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    if meta.is_file() && meta.nlink() != 1 {
                        return Err("REPORT_HARDLINK_REFUSED");
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && part == path => {}
            Err(_) => return Err("REPORT_PATH_INVALID"),
        }
    }
    Ok(())
}

// An I/O seam for fault tests, not an alternate persistence implementation.
// Production uses only File under the caller's held OS file lock.
trait JournalIo: Read + Write {
    fn synchronize(&mut self) -> std::io::Result<()>;
}
impl JournalIo for File {
    fn synchronize(&mut self) -> std::io::Result<()> {
        self.sync_all()
    }
}

fn update_journal(file: &mut impl JournalIo, proposed: &Report) -> Result<bool, &'static str> {
    validate(proposed)?;
    let mut existing = Vec::new();
    (&mut *file)
        .take(MAX_FILE + 1)
        .read_to_end(&mut existing)
        .map_err(|_| "REPORT_READ")?;
    if existing.len() as u64 > MAX_FILE
        || (!existing.is_empty() && existing.last() != Some(&b'\n'))
    {
        return Err("REPORT_CORRUPT");
    }
    let mut count = 0usize;
    let mut seen = BTreeSet::new();
    let mut duplicate = false;
    for row in existing.split_inclusive(|b| *b == b'\n') {
        // The writer emits objects only. Serde's derived struct decoder would
        // otherwise also accept positional arrays as a second journal format.
        if row.len() > MAX_RECORD
            || row.iter().copied().find(|b| !b.is_ascii_whitespace()) != Some(b'{')
        {
            return Err("REPORT_CORRUPT");
        }
        let record: Report = cwg_security::decode(row, MAX_RECORD)
            .map_err(|_| "REPORT_CORRUPT")?;
        validate(&record).map_err(|_| "REPORT_CORRUPT")?;
        count += 1;
        // Check while replaying, before an idempotent duplicate can return.
        if count > MAX_REPORTS {
            return Err("REPORT_CAPACITY");
        }
        if !seen.insert((record.binding_id.clone(), record.report_id.clone())) {
            return Err("REPORT_DUPLICATE_JOURNAL_ENTRY");
        }
        if record.binding_id == proposed.binding_id && record.report_id == proposed.report_id {
            if record.repository_id != proposed.repository_id
                || record.generation != proposed.generation
                || record.message_id != proposed.message_id
                || record.kind != proposed.kind
                || record.body != proposed.body
            {
                return Err("REPORT_ID_CONFLICT");
            }
            duplicate = true;
        }
    }
    if duplicate {
        // A full append can be visible after sync_all failed. A repeat must
        // reconcile durability, not merely observe cached bytes and claim it.
        file.synchronize().map_err(|_| "REPORT_COMMIT_UNKNOWN")?;
        return Ok(false);
    }
    if count >= MAX_REPORTS {
        return Err("REPORT_CAPACITY");
    }
    let mut encoded = serde_json::to_vec(proposed).map_err(|_| "REPORT_ENCODE")?;
    encoded.push(b'\n');
    if encoded.len() > MAX_RECORD
        || existing.len() as u64 + encoded.len() as u64 > MAX_FILE
    {
        return Err("REPORT_CAPACITY");
    }
    file.write_all(&encoded)
        .and_then(|_| file.synchronize())
        .map_err(|_| "REPORT_COMMIT_UNKNOWN")?;
    Ok(true)
}

/// Append one non-authoritative report to an owner-controlled local journal.
/// Identical report ids are idempotent; conflicting ids are refused. Both new
/// records and matching repeats synchronize before reporting persistence.
pub(crate) fn append(
    root: &Path,
    binding: &Binding,
    message_id: &str,
    report_id: &str,
    kind: &str,
    body: &str,
) -> Result<bool, &'static str> {
    binding.validate().map_err(|_| "REPORT_BINDING_INVALID")?;
    if !root.is_absolute() || !root.is_dir() {
        return Err("REPORT_ROOT_INVALID");
    }
    no_links(root)?;
    let path = root.join("worker-reports.jsonl");
    no_links(&path)?;
    let now = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "REPORT_CLOCK")?
            .as_millis(),
    )
    .map_err(|_| "REPORT_CLOCK")?;
    let proposed = Report {
        schema: "cwg-worker-report-v1".into(),
        binding_id: binding.id.clone(),
        repository_id: binding.repository_id,
        generation: binding.generation,
        message_id: message_id.into(),
        report_id: report_id.into(),
        kind: kind.into(),
        body: body.into(),
        created_at_unix_ms: now,
    };
    validate(&proposed)?;
    let mut options = OpenOptions::new();
    options.read(true).append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path).map_err(|_| "REPORT_OPEN")?;
    file.try_lock().map_err(|_| "REPORT_BUSY")?;
    update_journal(&mut file, &proposed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cwg_protocol::ChatTarget;
    fn binding() -> Binding {
        Binding {
            id: "sample".into(),
            repository_id: 1,
            owner_id: 2,
            repository: "owner/repo".into(),
            issue_number: 3,
            goal_id: "CWG-001".into(),
            generation: 1,
            account_id: "test-account".into(),
            target: ChatTarget {
                provider: "codex".into(),
                thread_id: "thread-1".into(),
            },
        }
    }
    #[test]
    fn durable_reopen_deduplicates_and_blocks_conflicts() {
        let root = std::env::temp_dir().join(format!(
            "cwg-mcp-report-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let b = binding();
        assert_eq!(append(&root, &b, "gh:1:4", "r1", "progress", "started"), Ok(true));
        assert_eq!(append(&root, &b, "gh:1:4", "r1", "progress", "started"), Ok(false));
        assert_eq!(
            append(&root, &b, "gh:1:4", "r1", "result", "done"),
            Err("REPORT_ID_CONFLICT")
        );
        assert_eq!(append(&root, &b, "gh:1:4", "r2", "result", "reported"), Ok(true));
        let mut file = OpenOptions::new().append(true)
            .open(root.join("worker-reports.jsonl")).unwrap();
        file.write_all(b"torn").unwrap();
        file.sync_all().unwrap();
        drop(file);
        // Even an identical earlier ID must not hide a corrupt tail.
        assert_eq!(append(&root, &b, "gh:1:4", "r1", "progress", "started"), Err("REPORT_CORRUPT"));
        fs::remove_dir_all(root).unwrap();
    }

    #[derive(Default)]
    struct FaultJournal {
        data: std::io::Cursor<Vec<u8>>,
        sync_fails: bool,
        writes: usize,
        syncs: usize,
    }
    impl Read for FaultJournal {
        fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
            self.data.read(bytes)
        }
    }
    impl Write for FaultJournal {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.writes += 1;
            self.data.write(bytes)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl JournalIo for FaultJournal {
        fn synchronize(&mut self) -> std::io::Result<()> {
            self.syncs += 1;
            if self.sync_fails {
                Err(std::io::Error::other("injected sync failure"))
            } else {
                Ok(())
            }
        }
    }
    fn proposed(id: &str) -> Report {
        Report {
            schema: "cwg-worker-report-v1".into(),
            binding_id: "sample".into(),
            repository_id: 1,
            generation: 1,
            message_id: "gh:1:4".into(),
            report_id: id.into(),
            kind: "progress".into(),
            body: "started".into(),
            created_at_unix_ms: 1,
        }
    }
    fn encoded(record: &Report) -> Vec<u8> {
        let mut bytes = serde_json::to_vec(record).unwrap();
        bytes.push(b'\n');
        bytes
    }
    fn memory(bytes: Vec<u8>) -> FaultJournal {
        FaultJournal {
            data: std::io::Cursor::new(bytes),
            ..FaultJournal::default()
        }
    }

    #[test]
    fn visible_unsynced_record_never_becomes_persisted_from_a_duplicate_alone() {
        let record = proposed("r1");
        let mut journal = FaultJournal { sync_fails: true, ..FaultJournal::default() };
        assert_eq!(update_journal(&mut journal, &record), Err("REPORT_COMMIT_UNKNOWN"));
        assert_eq!(journal.data.get_ref(), &encoded(&record));
        let writes = journal.writes;
        journal.data.set_position(0);
        assert_eq!(update_journal(&mut journal, &record), Err("REPORT_COMMIT_UNKNOWN"));
        assert_eq!(journal.writes, writes);
        assert_eq!(journal.syncs, 2);
        journal.sync_fails = false;
        journal.data.set_position(0);
        assert_eq!(update_journal(&mut journal, &record), Ok(false));
        assert_eq!(journal.writes, writes);
        assert_eq!(journal.syncs, 3);
    }

    #[test]
    fn over_capacity_journal_cannot_hide_behind_an_early_matching_id() {
        let mut bytes = Vec::new();
        for n in 0..=MAX_REPORTS {
            bytes.extend(encoded(&proposed(&format!("r{n}"))));
        }
        assert!((bytes.len() as u64) < MAX_FILE);
        let mut journal = memory(bytes);
        assert_eq!(update_journal(&mut journal, &proposed("r0")), Err("REPORT_CAPACITY"));
        assert_eq!((journal.writes, journal.syncs), (0, 0));
    }

    #[test]
    fn exact_capacity_can_reconcile_but_cannot_append() {
        let mut bytes = Vec::new();
        for n in 0..MAX_REPORTS {
            bytes.extend(encoded(&proposed(&format!("r{n}"))));
        }
        let mut journal = memory(bytes);
        assert_eq!(update_journal(&mut journal, &proposed("r0")), Ok(false));
        assert_eq!((journal.writes, journal.syncs), (0, 1));
        journal.data.set_position(0);
        assert_eq!(update_journal(&mut journal, &proposed("new")), Err("REPORT_CAPACITY"));
    }

    #[test]
    fn positional_or_duplicate_key_records_are_not_a_second_wire_format() {
        for bytes in [
            b"[\"cwg-worker-report-v1\",\"sample\",1,1,\"gh:1:4\",\"r1\",\"progress\",\"started\",1]\n".to_vec(),
            String::from_utf8(encoded(&proposed("r1"))).unwrap()
                .replace("\"generation\":1", "\"generation\":1,\"generation\":1").into_bytes(),
        ] {
            let mut journal = memory(bytes);
            assert_eq!(update_journal(&mut journal, &proposed("r1")), Err("REPORT_CORRUPT"));
            assert_eq!((journal.writes, journal.syncs), (0, 0));
        }
    }

    #[test]
    fn conflicting_identity_is_never_resynchronized_as_the_requested_report() {
        let record = proposed("r1");
        for mutation in 0..5 {
            let mut changed = record.clone();
            match mutation {
                0 => changed.repository_id += 1,
                1 => changed.generation += 1,
                2 => changed.message_id = "gh:1:5".into(),
                3 => changed.kind = "result".into(),
                _ => changed.body = "changed".into(),
            }
            let mut journal = memory(encoded(&record));
            assert_eq!(update_journal(&mut journal, &changed), Err("REPORT_ID_CONFLICT"));
            assert_eq!((journal.writes, journal.syncs), (0, 0));
        }
    }

    #[test]
    fn matching_record_does_not_mask_a_torn_tail_or_duplicate_stored_id() {
        let record = proposed("r1");
        for tail in [b"torn".to_vec(), encoded(&record)] {
            let mut bytes = encoded(&record);
            bytes.extend(tail);
            let mut journal = memory(bytes);
            assert!(update_journal(&mut journal, &record).is_err());
            assert_eq!((journal.writes, journal.syncs), (0, 0));
        }
    }
}
