//! Bounded append-only local inbox. Replays the existing Router, not a second
//! delivery state machine. Callers must authenticate before put_verified.
#![forbid(unsafe_code)]
use cwg_protocol::{ChatTarget, DeliveryStatus, Message, validate, validate_binding};
use cwg_router::{BindingKey, MessageKey, Router};
use cwg_security::windows_acl::open_private_journal;
use serde::de::{MapAccess, Visitor, value::MapAccessDeserializer};
use serde::{Deserialize, Deserializer, Serialize};
use std::{
    collections::BTreeMap,
    fmt,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub type Result<T> = std::result::Result<T, &'static str>;
const MAX_JOURNAL: u64 = 32 * 1024 * 1024;
const MAX_RECORD: usize = 512 * 1024;
const MAX_MESSAGES: usize = 1024;
const MAX_RECORDS: u64 = 20_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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

// Owner configuration and persisted records are named-field objects. Derived
// struct decoding alone also accepts positional arrays, which the writer never
// emits and which must not become another local trust-boundary representation.
fn object_only<'de, D, T>(deserializer: D) -> std::result::Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Object<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for Object<T> {
        type Value = T;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a named-field object")
        }
        fn visit_map<M: MapAccess<'de>>(self, map: M) -> std::result::Result<T, M::Error> {
            T::deserialize(MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(Object(std::marker::PhantomData))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BindingWire {
    id: String,
    repository_id: u64,
    owner_id: u64,
    repository: String,
    issue_number: u64,
    goal_id: String,
    generation: u64,
    account_id: String,
    target: ChatTarget,
}
impl<'de> Deserialize<'de> for Binding {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let raw: BindingWire = object_only(deserializer)?;
        let binding = Self {
            id: raw.id,
            repository_id: raw.repository_id,
            owner_id: raw.owner_id,
            repository: raw.repository,
            issue_number: raw.issue_number,
            goal_id: raw.goal_id,
            generation: raw.generation,
            account_id: raw.account_id,
            target: raw.target,
        };
        binding.validate().map_err(serde::de::Error::custom)?;
        Ok(binding)
    }
}
impl Binding {
    pub fn validate(&self) -> Result<()> {
        validate_binding(&self.repository, &self.goal_id, &self.target)
            .map_err(|_| "BINDING_INVALID")?;
        if self.repository_id == 0
            || self.owner_id == 0
            || self.issue_number == 0
            || self.generation == 0
            || !identifier(&self.id)
            || !identifier(&self.account_id)
        {
            return Err("BINDING_IDENTITY");
        }
        Ok(())
    }
}
fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_:".contains(&c))
}

#[derive(Clone, Debug, Serialize)]
struct Pending {
    binding_id: String,
    generation: u64,
    repository_id: u64,
    delivery_id: String,
    message: Message,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingWire {
    binding_id: String,
    generation: u64,
    repository_id: u64,
    delivery_id: String,
    message: Message,
}
impl<'de> Deserialize<'de> for Pending {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let raw: PendingWire = object_only(deserializer)?;
        Ok(Self {
            binding_id: raw.binding_id,
            generation: raw.generation,
            repository_id: raw.repository_id,
            delivery_id: raw.delivery_id,
            message: raw.message,
        })
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Item {
    pub binding_id: String,
    pub generation: u64,
    pub message: Message,
    pub status: DeliveryStatus,
}
mod transition;
pub use transition::Transition;
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum Event {
    Bind {
        binding: Binding,
    },
    Put {
        item: Pending,
    },
    DeliveryAlias {
        binding_id: String,
        message_id: String,
        delivery_id: String,
    },
    Advance {
        binding_id: String,
        message_id: String,
        transition: Transition,
    },
}
#[derive(Serialize)]
struct Record {
    schema: String,
    sequence: u64,
    event: Event,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordWire {
    schema: String,
    sequence: u64,
    event: Event,
}
impl<'de> Deserialize<'de> for Record {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let raw: RecordWire = object_only(deserializer)?;
        Ok(Self {
            schema: raw.schema,
            sequence: raw.sequence,
            event: raw.event,
        })
    }
}
#[derive(Default)]
struct View {
    router: Router,
    bindings: BTreeMap<String, Binding>,
    messages: BTreeMap<String, Pending>,
    deliveries: BTreeMap<String, String>,
    order: Vec<String>,
    sequence: u64,
}
impl View {
    fn apply(&mut self, event: &Event) -> Result<()> {
        match event {
            Event::Bind { binding } => {
                binding.validate()?;
                if self.bindings.len() >= 128
                    || self.bindings.contains_key(&binding.id)
                    || self.bindings.values().any(|b| {
                        b.repository_id == binding.repository_id
                            && b.issue_number == binding.issue_number
                    })
                {
                    return Err("BINDING_CONFLICT");
                }
                self.router
                    .register(
                        BindingKey {
                            repository: binding.repository.clone(),
                            goal_id: binding.goal_id.clone(),
                        },
                        binding.target.clone(),
                    )
                    .map_err(|_| "BINDING_CONFLICT")?;
                self.bindings.insert(binding.id.clone(), binding.clone());
            }
            Event::Put { item } => {
                validate(&item.message).map_err(|_| "MESSAGE_INVALID")?;
                let b = self
                    .bindings
                    .get(&item.binding_id)
                    .ok_or("BINDING_UNKNOWN")?;
                if b.repository_id != item.repository_id
                    || b.generation != item.generation
                    || !b
                        .repository
                        .eq_ignore_ascii_case(&item.message.source.repository)
                    || b.issue_number != item.message.source.issue_number
                    || b.goal_id != item.message.goal_id
                    || b.target != item.message.target
                    || !identifier(&item.delivery_id)
                    || item.message.message_id
                        != format!("gh:{}:{}", b.repository_id, item.message.source.comment_id)
                {
                    return Err("MESSAGE_BINDING_MISMATCH");
                }
                if self.messages.len() >= MAX_MESSAGES
                    || self.deliveries.contains_key(&item.delivery_id)
                    || self.messages.contains_key(&item.message.message_id)
                {
                    return Err("INBOX_CAPACITY_OR_REPLAY");
                }
                self.router
                    .enqueue_verified(item.message.clone())
                    .map_err(|_| "MESSAGE_CONFLICT")?;
                self.deliveries
                    .insert(item.delivery_id.clone(), item.message.message_id.clone());
                self.order.push(item.message.message_id.clone());
                self.messages
                    .insert(item.message.message_id.clone(), item.clone());
            }
            Event::DeliveryAlias {
                binding_id,
                message_id,
                delivery_id,
            } => {
                let item = self.messages.get(message_id).ok_or("MESSAGE_UNKNOWN")?;
                if &item.binding_id != binding_id
                    || !identifier(delivery_id)
                    || self.deliveries.contains_key(delivery_id)
                {
                    return Err("DELIVERY_CONFLICT");
                }
                self.deliveries
                    .insert(delivery_id.clone(), message_id.clone());
            }
            Event::Advance {
                binding_id,
                message_id,
                transition,
            } => {
                let item = self.messages.get(message_id).ok_or("MESSAGE_UNKNOWN")?;
                if &item.binding_id != binding_id {
                    return Err("MESSAGE_UNKNOWN");
                }
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
        if self.bindings.get(&b.id) == Some(b) {
            Ok(())
        } else {
            Err("BINDING_STALE_OR_UNKNOWN")
        }
    }
}

/// The parent directory must already exist and be protected from other users.
/// This file lock is concurrency control, not an OS security sandbox.
pub struct Store {
    root: PathBuf,
}
impl Store {
    pub fn open(root: &Path) -> Result<Self> {
        if !root.is_absolute() || !root.is_dir() {
            return Err("STATE_ROOT_REQUIRED");
        }
        no_links(root)?;
        // Inspect actual opened directory/file owners and DACLs before accepting
        // the store. The guard pins checked ancestors during this inspection.
        let _guard = open_private_journal(root).map_err(state_path_error)?;
        Ok(Self { root: root.into() })
    }
    fn transact<T>(&self, op: impl FnOnce(&mut View) -> Result<(T, Option<Event>)>) -> Result<T> {
        let mut guard = open_private_journal(&self.root).map_err(state_path_error)?;
        let file = guard.file_mut();
        file.try_lock().map_err(|_| "STATE_BUSY_OR_LOCK_FAILED")?;
        // The handle and its OS lock live through load, validation, append and fsync.
        let mut view = load(file)?;
        let (value, event) = op(&mut view)?;
        if let Some(event) = event {
            if view.sequence >= MAX_RECORDS {
                return Err("JOURNAL_CAPACITY");
            }
            view.apply(&event)?;
            let record = Record {
                schema: "cwg-journal-v1".into(),
                sequence: view.sequence + 1,
                event,
            };
            let mut bytes = serde_json::to_vec(&record).map_err(|_| "JOURNAL_ENCODE")?;
            bytes.push(b'\n');
            if bytes.len() > MAX_RECORD
                || file
                    .metadata()
                    .map_err(|_| "STATE_METADATA")?
                    .len()
                    .checked_add(bytes.len() as u64)
                    .is_none_or(|total| total > MAX_JOURNAL)
            {
                return Err("JOURNAL_CAPACITY");
            }
            // Any error here has an uncertain persistence outcome: caller MUST
            // reconcile by reading, never assume that nothing was appended.
            commit_bytes(file, Some(&bytes))?;
        } else {
            // A prior full append may be visible even when its sync failed.
            // A no-op/read reconciliation cannot acknowledge durable success
            // until it has successfully synchronized the observed prefix.
            commit_bytes(file, None)?;
        }
        Ok(value)
    }
    pub fn register(&self, binding: &Binding) -> Result<()> {
        binding.validate()?;
        self.transact(|v| match v.bindings.get(&binding.id) {
            Some(old) if old == binding => Ok(((), None)),
            Some(_) => Err("BINDING_IMMUTABLE"),
            None => Ok((
                (),
                Some(Event::Bind {
                    binding: binding.clone(),
                }),
            )),
        })
    }
    /// Returns false for an identical, already committed source message. A
    /// changed delivery header cannot replay the same numeric comment identity.
    pub fn put_verified(
        &self,
        binding: &Binding,
        delivery_id: &str,
        message: Message,
    ) -> Result<bool> {
        if !identifier(delivery_id) {
            return Err("DELIVERY_ID_INVALID");
        }
        self.transact(|v| {
            v.binding(binding)?;
            if v.deliveries
                .get(delivery_id)
                .is_some_and(|old| old != &message.message_id)
            {
                return Err("DELIVERY_CONFLICT");
            }
            if let Some(old) = v.messages.get(&message.message_id) {
                if old.message == message
                    && old.binding_id == binding.id
                    && old.generation == binding.generation
                {
                    let event =
                        (!v.deliveries.contains_key(delivery_id)).then(|| Event::DeliveryAlias {
                            binding_id: binding.id.clone(),
                            message_id: message.message_id.clone(),
                            delivery_id: delivery_id.into(),
                        });
                    return Ok((false, event));
                }
                return Err("MESSAGE_CONFLICT");
            }
            Ok((
                true,
                Some(Event::Put {
                    item: Pending {
                        binding_id: binding.id.clone(),
                        generation: binding.generation,
                        repository_id: binding.repository_id,
                        delivery_id: delivery_id.into(),
                        message,
                    },
                }),
            ))
        })
    }
    /// Trusted controller only. This records an observation; it does NOT prove
    /// provider acceptance/worker completion. Not exposed by the receiver CLI.
    pub fn advance(&self, binding: &Binding, id: &str, transition: Transition) -> Result<()> {
        if !identifier(id) {
            return Err("MESSAGE_ID_INVALID");
        }
        self.transact(|v| {
            v.binding(binding)?;
            Ok((
                (),
                Some(Event::Advance {
                    binding_id: binding.id.clone(),
                    message_id: id.into(),
                    transition,
                }),
            ))
        })
    }
    /// Read one message in the exact registered binding without acknowledging it.
    pub fn get_message(&self, binding: &Binding, id: &str) -> Result<Option<Item>> {
        if !identifier(id) {
            return Err("MESSAGE_ID_INVALID");
        }
        self.transact(|v| {
            v.binding(binding)?;
            let found = v.messages.get(id)
                .filter(|row| row.binding_id == binding.id)
                .map(|row| Item {
                    binding_id: row.binding_id.clone(),
                    generation: row.generation,
                    message: row.message.clone(),
                    status: v.router.state(&MessageKey::from(&row.message))
                        .expect("replayed message"),
                });
            Ok((found, None))
        })
    }

    /// Bounded, binding-scoped page; read-only even after an uncertain sync.
    pub fn inbox_page(&self, binding: &Binding, offset: usize, limit: usize) -> Result<Vec<Item>> {
        if !(1..=100).contains(&limit) || offset > MAX_MESSAGES {
            return Err("INBOX_PAGE_INVALID");
        }
        self.transact(|v| {
            v.binding(binding)?;
            let rows = v.order.iter().filter_map(|id| {
                let row = &v.messages[id];
                (row.binding_id == binding.id).then(|| Item {
                    binding_id: row.binding_id.clone(),
                    generation: row.generation,
                    message: row.message.clone(),
                    status: v.router.state(&MessageKey::from(&row.message))
                        .expect("replayed message"),
                })
            }).skip(offset).take(limit).collect();
            Ok((rows, None))
        })
    }
    pub fn inbox(&self, binding: &Binding, limit: usize) -> Result<Vec<Item>> {
        if !(1..=100).contains(&limit) {
            return Err("INBOX_LIMIT");
        }
        self.transact(|v| {
            v.binding(binding)?;
            let items = v
                .order
                .iter()
                .filter_map(|id| {
                    let row = &v.messages[id];
                    (row.binding_id == binding.id).then(|| Item {
                        binding_id: row.binding_id.clone(),
                        generation: row.generation,
                        message: row.message.clone(),
                        status: v
                            .router
                            .state(&MessageKey::from(&row.message))
                            .expect("replayed message"),
                    })
                })
                .take(limit)
                .collect();
            Ok((items, None))
        })
    }
}

fn state_path_error(error: &'static str) -> &'static str {
    if error == "ACL_FILE_BUSY" {
        "STATE_BUSY_OR_LOCK_FAILED"
    } else {
        error
    }
}
fn load(file: &mut File) -> Result<View> {
    let mut raw = Vec::new();
    file.take(MAX_JOURNAL + 1)
        .read_to_end(&mut raw)
        .map_err(|_| "STATE_READ")?;
    if raw.len() as u64 > MAX_JOURNAL || (!raw.is_empty() && raw.last() != Some(&b'\n')) {
        return Err("JOURNAL_CORRUPT_OR_TOO_LARGE");
    }
    let mut view = View::default();
    for line in raw.split_inclusive(|b| *b == b'\n') {
        if line.len() > MAX_RECORD || view.sequence >= MAX_RECORDS {
            return Err("JOURNAL_CAPACITY");
        }
        let record: Record =
            cwg_security::decode(line, MAX_RECORD).map_err(|_| "JOURNAL_CORRUPT")?;
        if record.schema != "cwg-journal-v1" || record.sequence != view.sequence + 1 {
            return Err("JOURNAL_SEQUENCE");
        }
        view.apply(&record.event)?;
        view.sequence = record.sequence;
    }
    Ok(view)
}

trait CommitIo {
    fn append(&mut self, bytes: &[u8]) -> std::io::Result<()>;
    fn synchronize(&mut self) -> std::io::Result<()>;
}
impl CommitIo for File {
    fn append(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.write_all(bytes)
    }
    fn synchronize(&mut self) -> std::io::Result<()> {
        self.sync_all()
    }
}
fn commit_bytes(io: &mut impl CommitIo, bytes: Option<&[u8]>) -> Result<()> {
    if let Some(bytes) = bytes {
        io.append(bytes).map_err(|_| "STATE_COMMIT_UNKNOWN")?;
    }
    io.synchronize().map_err(|_| "STATE_COMMIT_UNKNOWN")
}

fn no_links(path: &Path) -> Result<()> {
    for p in path.ancestors() {
        match fs::symlink_metadata(p) {
            Ok(m) => {
                if m.file_type().is_symlink() {
                    return Err("STATE_LINK_REFUSED");
                }
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if m.file_attributes() & 0x400 != 0 {
                        return Err("STATE_REPARSE_REFUSED");
                    }
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    if m.is_file() && m.nlink() != 1 {
                        return Err("STATE_HARDLINK_REFUSED");
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && p == path => {}
            Err(_) => return Err("STATE_PATH_INVALID"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod commit_tests {
    use super::*;
    #[derive(Default)]
    struct FaultIo {
        bytes: Vec<u8>,
        partial_limit: Option<usize>,
        fail_sync: bool,
        append_calls: usize,
        sync_calls: usize,
    }
    impl CommitIo for FaultIo {
        fn append(&mut self, bytes: &[u8]) -> std::io::Result<()> {
            self.append_calls += 1;
            if let Some(limit) = self.partial_limit {
                self.bytes
                    .extend_from_slice(&bytes[..limit.min(bytes.len())]);
                return Err(std::io::Error::other("injected partial append"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(())
        }
        fn synchronize(&mut self) -> std::io::Result<()> {
            self.sync_calls += 1;
            if self.fail_sync {
                Err(std::io::Error::other("injected fsync failure"))
            } else {
                Ok(())
            }
        }
    }
    #[test]
    fn partial_append_is_unknown_and_never_automatically_replayed_or_removed() {
        let mut io = FaultIo {
            partial_limit: Some(3),
            ..FaultIo::default()
        };
        assert_eq!(
            commit_bytes(&mut io, Some(b"record\n")),
            Err("STATE_COMMIT_UNKNOWN")
        );
        assert_eq!(io.bytes, b"rec");
        assert_eq!(io.append_calls, 1);
        assert_eq!(io.sync_calls, 0);
    }
    #[test]
    fn visible_append_with_failed_sync_is_not_durable_until_reconciliation_sync_succeeds() {
        let mut io = FaultIo {
            fail_sync: true,
            ..FaultIo::default()
        };
        assert_eq!(
            commit_bytes(&mut io, Some(b"complete record\n")),
            Err("STATE_COMMIT_UNKNOWN")
        );
        assert_eq!(io.bytes, b"complete record\n");
        assert_eq!(commit_bytes(&mut io, None), Err("STATE_COMMIT_UNKNOWN"));
        assert_eq!(io.append_calls, 1);
        assert_eq!(io.sync_calls, 2);
        io.fail_sync = false;
        assert_eq!(commit_bytes(&mut io, None), Ok(()));
        assert_eq!(io.append_calls, 1);
        assert_eq!(io.sync_calls, 3);
        assert_eq!(io.bytes, b"complete record\n");
    }
    #[test]
    fn even_a_no_op_refuses_success_when_observed_journal_cannot_be_synchronized() {
        let mut io = FaultIo {
            fail_sync: true,
            ..FaultIo::default()
        };
        assert_eq!(commit_bytes(&mut io, None), Err("STATE_COMMIT_UNKNOWN"));
        assert_eq!(io.append_calls, 0);
        assert_eq!(io.sync_calls, 1);
    }
}

#[cfg(test)]
mod journal_wire_tests;
