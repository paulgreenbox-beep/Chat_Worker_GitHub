//! Bounded append-only local inbox. Replays the existing Router, not a second
//! delivery state machine. Callers must authenticate before put_verified.
#![forbid(unsafe_code)]
use cwg_protocol::{ChatTarget, DeliveryStatus, Message, validate, validate_binding};
use cwg_router::{BindingKey, MessageKey, Router};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub type Result<T> = std::result::Result<T, &'static str>;
const MAX_JOURNAL: u64 = 32 * 1024 * 1024;
const MAX_RECORD: usize = 512 * 1024;
const MAX_MESSAGES: usize = 1024;
const MAX_RECORDS: u64 = 20_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub id: String,
    pub repository_id: u64,
    pub owner_id: u64,
    pub repository: String,
    pub issue_number: u64,
    pub goal_id: String,
    pub generation: u64,
    /// Operator-selected account namespace; not proof of a live Codex account.
    pub account_id: String,
    pub target: ChatTarget,
}
impl Binding {
    pub fn validate(&self) -> Result<()> {
        validate_binding(&self.repository, &self.goal_id, &self.target).map_err(|_| "BINDING_INVALID")?;
        if self.repository_id == 0 || self.owner_id == 0 || self.issue_number == 0 || self.generation == 0
            || !identifier(&self.id) || !identifier(&self.account_id) {
            return Err("BINDING_IDENTITY");
        }
        Ok(())
    }
}
fn identifier(s: &str) -> bool {
    !s.is_empty() && s.len() <= 128 && s.bytes().all(|c| c.is_ascii_alphanumeric() || b"-_:".contains(&c))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    binding_id: String,
    generation: u64,
    repository_id: u64,
    delivery_id: String,
    message: Message,
}
#[derive(Clone, Debug, Serialize)]
pub struct Item {
    pub binding_id: String,
    pub generation: u64,
    pub message: Message,
    pub status: DeliveryStatus,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Transition { Begin, Accepted, Unknown, Acknowledge, Complete, Block }
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum Event {
    Bind { binding: Binding },
    Put { item: Pending },
    Advance { binding_id: String, message_id: String, transition: Transition },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record { schema: String, sequence: u64, event: Event }
#[derive(Default)]
struct View {
    router: Router,
    bindings: BTreeMap<String, Binding>,
    messages: BTreeMap<String, Pending>,
    deliveries: BTreeSet<String>,
    order: Vec<String>,
    sequence: u64,
}
impl View {
    fn apply(&mut self, event: &Event) -> Result<()> {
        match event {
            Event::Bind { binding } => {
                binding.validate()?;
                if self.bindings.len() >= 128 || self.bindings.contains_key(&binding.id)
                    || self.bindings.values().any(|b| b.repository_id == binding.repository_id && b.issue_number == binding.issue_number) {
                    return Err("BINDING_CONFLICT");
                }
                self.router.register(BindingKey { repository: binding.repository.clone(), goal_id: binding.goal_id.clone() }, binding.target.clone()).map_err(|_| "BINDING_CONFLICT")?;
                self.bindings.insert(binding.id.clone(), binding.clone());
            }
            Event::Put { item } => {
                validate(&item.message).map_err(|_| "MESSAGE_INVALID")?;
                let b = self.bindings.get(&item.binding_id).ok_or("BINDING_UNKNOWN")?;
                if b.repository_id != item.repository_id || b.generation != item.generation
                    || !b.repository.eq_ignore_ascii_case(&item.message.source.repository)
                    || b.issue_number != item.message.source.issue_number || b.goal_id != item.message.goal_id
                    || b.target != item.message.target || !identifier(&item.delivery_id)
                    || item.message.message_id != format!("gh:{}:{}", b.repository_id, item.message.source.comment_id) {
                    return Err("MESSAGE_BINDING_MISMATCH");
                }
                if self.messages.len() >= MAX_MESSAGES || self.deliveries.contains(&item.delivery_id) {
                    return Err("INBOX_CAPACITY_OR_REPLAY");
                }
                self.router.enqueue_verified(item.message.clone()).map_err(|_| "MESSAGE_CONFLICT")?;
                self.deliveries.insert(item.delivery_id.clone());
                self.order.push(item.message.message_id.clone());
                self.messages.insert(item.message.message_id.clone(), item.clone());
            }
            Event::Advance { binding_id, message_id, transition } => {
                let item = self.messages.get(message_id).ok_or("MESSAGE_UNKNOWN")?;
                if &item.binding_id != binding_id { return Err("MESSAGE_UNKNOWN"); }
                let key = MessageKey::from(&item.message);
                let result = match transition {
                    Transition::Begin => self.router.begin_delivery(&key),
                    Transition::Accepted => self.router.mark_delivered(&key),
                    Transition::Unknown => self.router.mark_unknown(&key),
                    Transition::Acknowledge => self.router.acknowledge(&key),
                    Transition::Complete => self.router.complete(&key),
                    Transition::Block => self.router.block_unknown(&key),
                };
                result.map_err(|_| "WRONG_STATE")?;
            }
        }
        Ok(())
    }
    fn binding(&self, b: &Binding) -> Result<()> {
        if self.bindings.get(&b.id) == Some(b) { Ok(()) } else { Err("BINDING_STALE_OR_UNKNOWN") }
    }
}

/// The parent directory must already exist and be protected from other users.
/// This file lock is concurrency control, not an OS security sandbox.
pub struct Store { path: PathBuf }
impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        if !root.is_absolute() || !root.is_dir() { return Err("STATE_ROOT_REQUIRED"); }
        no_links(root)?;
        Ok(Self { path: root.join("inbox.jsonl") })
    }
    fn transact<T>(&self, op: impl FnOnce(&mut View) -> Result<(T, Option<Event>)>) -> Result<T> {
        no_links(&self.path)?;
        let mut options = OpenOptions::new();
        options.read(true).append(true).create(true);
        #[cfg(unix)]
        { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
        let mut file = options.open(&self.path).map_err(|_| "STATE_OPEN")?;
        file.try_lock().map_err(|_| "STATE_BUSY_OR_LOCK_FAILED")?;
        // The handle and its OS lock live through load, validation, append and fsync.
        let mut view = load(&mut file)?;
        let (value, event) = op(&mut view)?;
        if let Some(event) = event {
            if view.sequence >= MAX_RECORDS { return Err("JOURNAL_CAPACITY"); }
            view.apply(&event)?;
            let record = Record { schema: "cwg-journal-v1".into(), sequence: view.sequence + 1, event };
            let mut bytes = serde_json::to_vec(&record).map_err(|_| "JOURNAL_ENCODE")?;
            bytes.push(b'\n');
            if bytes.len() > MAX_RECORD || file.metadata().map_err(|_| "STATE_METADATA")?.len() + bytes.len() as u64 > MAX_JOURNAL {
                return Err("JOURNAL_CAPACITY");
            }
            // Any error here has an uncertain persistence outcome: caller MUST
            // reconcile by reading, never assume that nothing was appended.
            file.write_all(&bytes).and_then(|_| file.sync_all()).map_err(|_| "STATE_COMMIT_UNKNOWN")?;
        }
        Ok(value)
    }
    pub fn register(&self, binding: &Binding) -> Result<()> {
        binding.validate()?;
        self.transact(|v| match v.bindings.get(&binding.id) {
            Some(old) if old == binding => Ok(((), None)),
            Some(_) => Err("BINDING_IMMUTABLE"),
            None => Ok(((), Some(Event::Bind { binding: binding.clone() }))),
        })
    }
    /// Returns false for an identical, already committed source message. A
    /// changed delivery header cannot replay the same numeric comment identity.
    pub fn put_verified(&self, binding: &Binding, delivery_id: &str, message: Message) -> Result<bool> {
        if !identifier(delivery_id) { return Err("DELIVERY_ID_INVALID"); }
        self.transact(|v| {
            v.binding(binding)?;
            if let Some(old) = v.messages.get(&message.message_id) {
                if old.message == message && old.binding_id == binding.id && old.generation == binding.generation {
                    return Ok((false, None));
                }
                return Err("MESSAGE_CONFLICT");
            }
            Ok((true, Some(Event::Put { item: Pending {
                binding_id: binding.id.clone(), generation: binding.generation,
                repository_id: binding.repository_id, delivery_id: delivery_id.into(), message,
            } })))
        })
    }
    /// Trusted controller only. This records an observation; it does NOT prove
    /// provider acceptance/worker completion. Not exposed by the receiver CLI.
    pub fn advance(&self, binding: &Binding, id: &str, transition: Transition) -> Result<()> {
        self.transact(|v| {
            v.binding(binding)?;
            Ok(((), Some(Event::Advance { binding_id: binding.id.clone(), message_id: id.into(), transition })))
        })
    }
    pub fn inbox(&self, binding: &Binding, limit: usize) -> Result<Vec<Item>> {
        if !(1..=100).contains(&limit) { return Err("INBOX_LIMIT"); }
        self.transact(|v| {
            v.binding(binding)?;
            let items = v.order.iter().filter_map(|id| {
                let row = &v.messages[id];
                (row.binding_id == binding.id).then(|| Item {
                    binding_id: row.binding_id.clone(), generation: row.generation,
                    message: row.message.clone(), status: v.router.state(&MessageKey::from(&row.message)).expect("replayed message"),
                })
            }).take(limit).collect();
            Ok((items, None))
        })
    }
}
fn load(file: &mut File) -> Result<View> {
    let mut raw = Vec::new();
    file.take(MAX_JOURNAL + 1).read_to_end(&mut raw).map_err(|_| "STATE_READ")?;
    if raw.len() as u64 > MAX_JOURNAL || (!raw.is_empty() && raw.last() != Some(&b'\n')) {
        return Err("JOURNAL_CORRUPT_OR_TOO_LARGE");
    }
    let mut view = View::default();
    for line in raw.split_inclusive(|b| *b == b'\n') {
        if line.len() > MAX_RECORD || view.sequence >= MAX_RECORDS { return Err("JOURNAL_CAPACITY"); }
        let record: Record = serde_json::from_slice(line).map_err(|_| "JOURNAL_CORRUPT")?;
        if record.schema != "cwg-journal-v1" || record.sequence != view.sequence + 1 { return Err("JOURNAL_SEQUENCE"); }
        view.apply(&record.event)?;
        view.sequence = record.sequence;
    }
    Ok(view)
}
fn no_links(path: &Path) -> Result<()> {
    for p in path.ancestors() {
        match fs::symlink_metadata(p) {
            Ok(m) => {
                if m.file_type().is_symlink() { return Err("STATE_LINK_REFUSED"); }
                #[cfg(windows)]
                { use std::os::windows::fs::MetadataExt; if m.file_attributes() & 0x400 != 0 { return Err("STATE_REPARSE_REFUSED"); } }
                #[cfg(unix)]
                { use std::os::unix::fs::MetadataExt; if m.is_file() && m.nlink() != 1 { return Err("STATE_HARDLINK_REFUSED"); } }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && p == path => {}
            Err(_) => return Err("STATE_PATH_INVALID"),
        }
    }
    Ok(())
}
