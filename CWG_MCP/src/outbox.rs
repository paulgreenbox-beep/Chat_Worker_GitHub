//! Durable, data-only worker reports. This is NOT an authenticated GitHub
//! publisher, a task completion oracle, or a second delivery state machine.
use cwg_state::Binding;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
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

/// Append one non-authoritative report to an owner-controlled local journal.
/// Identical report ids are idempotent; conflicting ids are refused.
pub(crate) fn append(
    root: &Path,
    binding: &Binding,
    message_id: &str,
    report_id: &str,
    kind: &str,
    body: &str,
) -> Result<bool, &'static str> {
    if !root.is_absolute() || !root.is_dir() {
        return Err("REPORT_ROOT_INVALID");
    }
    no_links(root)?;
    let path = root.join("worker-reports.jsonl");
    no_links(&path)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "REPORT_CLOCK")?
        .as_millis() as u64;
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
    let mut existing = Vec::new();
    (&mut file)
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
        if row.len() > MAX_RECORD {
            return Err("REPORT_CORRUPT");
        }
        let record: Report = cwg_security::decode(row, MAX_RECORD)
            .map_err(|_| "REPORT_CORRUPT")?;
        validate(&record).map_err(|_| "REPORT_CORRUPT")?;
        count += 1;
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
        return Ok(false);
    }
    if count >= MAX_REPORTS {
        return Err("REPORT_CAPACITY");
    }
    let mut encoded = serde_json::to_vec(&proposed).map_err(|_| "REPORT_ENCODE")?;
    encoded.push(b'\n');
    if encoded.len() > MAX_RECORD
        || existing.len() as u64 + encoded.len() as u64 > MAX_FILE
    {
        return Err("REPORT_CAPACITY");
    }
    file.write_all(&encoded)
        .and_then(|_| file.sync_all())
        .map_err(|_| "REPORT_COMMIT_UNKNOWN")?;
    Ok(true)
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
        fs::remove_dir_all(root).unwrap();
    }
}
