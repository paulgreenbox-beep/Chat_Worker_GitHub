//! In-memory, non-executing routing model. An authenticated controller must
//! authorize and persist bindings and verify GitHub events before calling this.
#![forbid(unsafe_code)]
use cwg_protocol::{ChatTarget, DeliveryStatus, Message, validate, validate_binding};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutingError {
    InvalidMessage,
    UnboundTarget,
    Conflict,
    AlreadyRegistered,
    StaleOrWrongState,
    CapacityExceeded,
    BindingSuspended,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct BindingKey {
    pub repository: String,
    pub goal_id: String,
}
impl BindingKey {
    fn canonicalize(mut self) -> Self {
        // GitHub owner/repository names are ASCII case-insensitive. Provider,
        // thread and GOAL IDs remain exact, case-sensitive identities.
        self.repository.make_ascii_lowercase();
        self
    }
}

/// Structural correlation data supplied by a trusted controller, not caller
/// authentication or evidence that any receipt actually came from this route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageKey {
    pub repository: String,
    pub goal_id: String,
    pub target: ChatTarget,
    pub message_id: String,
}
impl From<&Message> for MessageKey {
    fn from(message: &Message) -> Self {
        Self {
            repository: message.source.repository.clone(),
            goal_id: message.goal_id.clone(),
            target: message.target.clone(),
            message_id: message.message_id.clone(),
        }
    }
}
impl MessageKey {
    fn storage_key(&self) -> Option<(String, String)> {
        if self.message_id.is_empty()
            || self.message_id.len() > 128
            || !self
                .message_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_:".contains(&b))
            || validate_binding(&self.repository, &self.goal_id, &self.target).is_err()
        {
            return None;
        }
        Some((
            self.repository.to_ascii_lowercase(),
            self.message_id.clone(),
        ))
    }

    fn matches(&self, message: &Message) -> bool {
        self.repository
            .eq_ignore_ascii_case(&message.source.repository)
            && self.message_id == message.message_id
            && self.goal_id == message.goal_id
            && self.target == message.target
    }
}

/// Admission budgets bound stored message bodies and collection cardinality.
/// They are not an allocator quota or an operating-system isolation boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouterLimits {
    pub max_bindings: usize,
    pub max_messages: usize,
    pub max_body_bytes: usize,
}
impl Default for RouterLimits {
    fn default() -> Self {
        Self {
            max_bindings: 1024,
            max_messages: 4096,
            max_body_bytes: 16 * 1024 * 1024,
        }
    }
}

struct Binding {
    target: ChatTarget,
    suspended: bool,
}

pub struct Router {
    bindings: BTreeMap<BindingKey, Binding>,
    messages: BTreeMap<(String, String), (Message, DeliveryStatus)>,
    comment_ids: BTreeSet<(String, u64)>,
    limits: RouterLimits,
    body_bytes: usize,
}
impl Default for Router {
    fn default() -> Self {
        Self::with_limits(RouterLimits::default())
    }
}

fn checked_total(current: usize, additional: usize, limit: usize) -> Result<usize, RoutingError> {
    current
        .checked_add(additional)
        .filter(|total| *total <= limit)
        .ok_or(RoutingError::CapacityExceeded)
}
impl Router {
    /// Zero budgets are valid and refuse the corresponding admissions. Budgets
    /// never trigger eviction: messages and replay history remain stored.
    pub fn with_limits(limits: RouterLimits) -> Self {
        Self {
            bindings: BTreeMap::new(),
            messages: BTreeMap::new(),
            comment_ids: BTreeSet::new(),
            limits,
            body_bytes: 0,
        }
    }

    /// Called ONLY after independent owner approval by an external controller.
    pub fn register(&mut self, key: BindingKey, target: ChatTarget) -> Result<(), RoutingError> {
        validate_binding(&key.repository, &key.goal_id, &target)
            .map_err(|_| RoutingError::InvalidMessage)?;
        let key = key.canonicalize();
        if self.bindings.contains_key(&key) {
            return Err(RoutingError::AlreadyRegistered);
        }
        checked_total(self.bindings.len(), 1, self.limits.max_bindings)?;
        self.bindings.insert(
            key,
            Binding {
                target,
                suspended: false,
            },
        );
        Ok(())
    }

    /// A trusted controller may suspend a binding after owner revocation or a
    /// stale-thread observation. This only closes admission/new claims; outcome
    /// recording for work already in flight stays available. No resume/rebind
    /// operation exists, and messages/replay history are never discarded.
    pub fn suspend_binding(&mut self, key: &BindingKey) -> Result<(), RoutingError> {
        // Bound allocations for keys that did not originate from a valid model.
        if key.repository.len() > 140 || key.goal_id.len() > 36 {
            return Err(RoutingError::UnboundTarget);
        }
        let key = key.clone().canonicalize();
        let binding = self
            .bindings
            .get_mut(&key)
            .ok_or(RoutingError::UnboundTarget)?;
        binding.suspended = true;
        Ok(())
    }

    /// Message must already have passed signature, identity and API reconciliation.
    pub fn enqueue_verified(&mut self, message: Message) -> Result<(), RoutingError> {
        validate(&message).map_err(|_| RoutingError::InvalidMessage)?;
        let key = BindingKey {
            repository: message.source.repository.clone(),
            goal_id: message.goal_id.clone(),
        }
        .canonicalize();
        let binding = self.bindings.get(&key).ok_or(RoutingError::UnboundTarget)?;
        if binding.target != message.target {
            return Err(RoutingError::UnboundTarget);
        }
        if binding.suspended {
            return Err(RoutingError::BindingSuspended);
        }
        let storage_key = (key.repository.clone(), message.message_id.clone());
        let cid = (key.repository, message.source.comment_id);
        if self.messages.contains_key(&storage_key) || self.comment_ids.contains(&cid) {
            return Err(RoutingError::Conflict);
        }
        checked_total(self.messages.len(), 1, self.limits.max_messages)?;
        let body_bytes = checked_total(
            self.body_bytes,
            message.body.len(),
            self.limits.max_body_bytes,
        )?;
        self.comment_ids.insert(cid);
        self.messages
            .insert(storage_key, (message, DeliveryStatus::Verified));
        self.body_bytes = body_bytes;
        Ok(())
    }
    pub fn state(&self, key: &MessageKey) -> Option<DeliveryStatus> {
        self.messages
            .get(&key.storage_key()?)
            .filter(|(message, _)| key.matches(message))
            .map(|(_, status)| *status)
    }
    pub fn begin_delivery(&mut self, key: &MessageKey) -> Result<(), RoutingError> {
        self.transition(key, DeliveryStatus::Verified, DeliveryStatus::Delivering)
    }
    pub fn mark_unknown(&mut self, key: &MessageKey) -> Result<(), RoutingError> {
        self.transition(key, DeliveryStatus::Delivering, DeliveryStatus::Unknown)
    }
    /// The trusted controller independently observed provider acceptance.
    pub fn mark_delivered(&mut self, key: &MessageKey) -> Result<(), RoutingError> {
        self.transition(key, DeliveryStatus::Delivering, DeliveryStatus::Delivered)
    }
    pub fn acknowledge(&mut self, key: &MessageKey) -> Result<(), RoutingError> {
        self.transition(key, DeliveryStatus::Delivered, DeliveryStatus::Acknowledged)
    }
    /// Called only after independent work evidence; this model cannot verify it.
    pub fn complete(&mut self, key: &MessageKey) -> Result<(), RoutingError> {
        self.transition(key, DeliveryStatus::Acknowledged, DeliveryStatus::Completed)
    }
    /// Owner review may close UNKNOWN, but never makes it retryable.
    pub fn block_unknown(&mut self, key: &MessageKey) -> Result<(), RoutingError> {
        self.transition(key, DeliveryStatus::Unknown, DeliveryStatus::Blocked)
    }
    fn transition(
        &mut self,
        key: &MessageKey,
        from: DeliveryStatus,
        to: DeliveryStatus,
    ) -> Result<(), RoutingError> {
        let storage_key = key.storage_key().ok_or(RoutingError::StaleOrWrongState)?;
        let (message, status) = self
            .messages
            .get_mut(&storage_key)
            .ok_or(RoutingError::StaleOrWrongState)?;
        if !key.matches(message) || *status != from {
            return Err(RoutingError::StaleOrWrongState);
        }
        if to == DeliveryStatus::Delivering {
            let binding_key = BindingKey {
                repository: storage_key.0,
                goal_id: message.goal_id.clone(),
            };
            let binding = self
                .bindings
                .get(&binding_key)
                .ok_or(RoutingError::UnboundTarget)?;
            if binding.target != message.target {
                return Err(RoutingError::UnboundTarget);
            }
            if binding.suspended {
                return Err(RoutingError::BindingSuspended);
            }
        }
        *status = to;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use cwg_protocol::{GithubSource, Message};
    fn msg() -> Message {
        Message {
            schema: "cwg-message-v1".into(),
            message_id: "m1".into(),
            source: GithubSource {
                repository: "owner/repo".into(),
                issue_number: 1,
                comment_id: 2,
                author_id: 3,
            },
            target: ChatTarget {
                provider: "codex".into(),
                thread_id: "thread-1".into(),
            },
            goal_id: "CWG-001".into(),
            body: "Check source".into(),
        }
    }
    fn key(id: &str) -> MessageKey {
        let mut message = msg();
        message.message_id = id.into();
        MessageKey::from(&message)
    }
    #[test]
    fn exact_binding_and_duplicate_refusal() {
        let mut r = Router::default();
        let m = msg();
        assert_eq!(
            r.enqueue_verified(m.clone()),
            Err(RoutingError::UnboundTarget)
        );
        r.register(
            BindingKey {
                repository: "owner/repo".into(),
                goal_id: "CWG-001".into(),
            },
            m.target.clone(),
        )
        .unwrap();
        r.enqueue_verified(m.clone()).unwrap();
        assert_eq!(r.enqueue_verified(m), Err(RoutingError::Conflict));
    }
    #[test]
    fn uncertain_delivery_does_not_retry() {
        let mut r = Router::default();
        let m = msg();
        r.register(
            BindingKey {
                repository: "owner/repo".into(),
                goal_id: "CWG-001".into(),
            },
            m.target.clone(),
        )
        .unwrap();
        r.enqueue_verified(m).unwrap();
        r.begin_delivery(&key("m1")).unwrap();
        r.mark_unknown(&key("m1")).unwrap();
        assert_eq!(
            r.begin_delivery(&key("m1")),
            Err(RoutingError::StaleOrWrongState)
        );
    }

    #[test]
    fn admission_accounting_rejects_overflow_without_wrapping() {
        assert_eq!(
            checked_total(usize::MAX, 1, usize::MAX),
            Err(RoutingError::CapacityExceeded)
        );
        assert_eq!(checked_total(usize::MAX - 1, 1, usize::MAX), Ok(usize::MAX));
        assert_eq!(checked_total(1, 1, 1), Err(RoutingError::CapacityExceeded));
    }
}
