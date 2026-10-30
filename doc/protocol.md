# CWG protocol v1 (source foundation)

`cwg-message-v1`: `message_id`, `source.repository`, `source.issue_number`, `source.comment_id`, `source.author_id`, `target.provider`, `target.thread_id`, `goal_id`, `body`.

Binding key: GitHub repository identity + GOAL ID. Mutable chat titles are display metadata only. A production binding should additionally bind numeric GitHub repository ID, provider/account identity and a generation to resist renaming and stale-chat reuse.

## Implemented validation

- Schema is exactly `cwg-message-v1`.
- `message_id`: 1..128 ASCII bytes, letters/digits or `-_:`, unique within its canonical repository. The same ID in another repository has independent storage and cannot reserve this repository's namespace.
- Repository: `owner/name`; owner is 1..39 ASCII letters/digits or internal single hyphens; name is 1..100 ASCII letters/digits or `-_.`, excluding `.` and `..`. Router canonicalizes both parts to ASCII lowercase for binding and comment deduplication. It preserves the original message. Renaming/recreating repositories still requires future numeric identity/generation checks.
- `issue_number`, `comment_id`, `author_id`: nonzero `u64`. Raw JSON strings, floats (including exponential notation), negative/overflow numbers and null are rejected. A valid number does not prove the GitHub object exists or belongs to the sender.
- Provider: 1..32 ASCII bytes, a lowercase letter followed by lowercase letters, digits, `-` or `_`. Thread ID: 1..128 ASCII letters/digits or `-_:.`. Both are compared exactly; provider/account ownership is not authenticated by these strings.
- GOAL: `CWG-` or legacy `MSS-` plus 1..32 ASCII digits, at least one nonzero digit. Leading zeros are retained and significant; `CWG-1` and `CWG-001` are distinct GOALs.
- Body: 1..65,536 UTF-8 bytes, including unrestricted valid Unicode as untrusted context. Limits are bytes, not Unicode scalar counts.

`Message`, `GithubSource` and `ChatTarget` deserialization requires objects and validates before returning a model. Positional arrays are rejected even when their values/order would match the fields. Each private wire representation rejects unknown and duplicate fields. Use `Message::from_json_slice(&[u8])` for untrusted JSON: documents exceeding `MAX_JSON_BYTES` (400 KiB) are rejected before parsing, and `MessageParseError` reports only TooLarge or InvalidJson line/column without copying input into the error. The allowance accommodates a maximum-size body expressed with JSON Unicode escapes plus normal envelope fields; excess whitespace can still exceed the physical document limit.

Decoded wire strings are bounded by byte length before a visitor copies borrowed text. Parser-owned scratch allocation for escaped strings is controlled by the raw-slice limit, not by generic Serde visitors alone. Future ingress must also limit the network request before collecting it into a slice. Generic `Deserialize` remains format-independent and requires callers to impose that raw bound. First converting JSON to a generic map/`serde_json::Value` loses duplicate keys and cannot be repaired by subsequent typed decoding. Malformed UTF-8/unpaired JSON surrogates are rejected by the JSON parser. Public fields still allow direct Rust construction, so `validate` and Router admission also enforce constraints. Debug output for Message includes body length rather than body text.

`InvalidMessage::{Schema, Identity, Size, Format}` classify schema, zero numeric identity, size and syntax failures respectively. Display errors do not echo message bodies. `DeliveryStatus` has exact uppercase wire names.

## Implemented in-memory routing API

`register` validates and inserts an immutable repository/GOAL → target mapping; repeated registration fails even for an identical target. It does **not** authenticate the caller or obtain owner approval. `enqueue_verified` revalidates the model and requires an active exact binding, then deduplicates `(canonical repository, message_id)` and `(canonical repository, comment_id)` without partial insertion on failure. Its name describes a mandatory caller precondition, not verification performed inside Router.

State queries and mutations now require `&MessageKey { repository, goal_id, target, message_id }`, normally constructed with `MessageKey::from(&message)` before moving the message into Router. A bare ID is no longer accepted. Each operation checks the expected GOAL/provider/thread against the selected stored message and refuses mismatches. A caller who knows another route's complete matching key is not denied on identity grounds: knowing or constructing a key does not authenticate its caller. This is a deliberate API migration in the source foundation; wire fields/version are unchanged.

`Router::default()` enforces `RouterLimits` of 1,024 bindings, 4,096 retained messages and 16 MiB cumulative body bytes. A trusted controller may choose different limits with `Router::with_limits`; zero is a valid deny-admission budget. Checked arithmetic and all capacity checks precede insertion, with `CapacityExceeded` on exhaustion. There is no automatic eviction, so terminal messages and dedupe records remain counted. Limits bound the admitted data, not every allocator/runtime overhead or per-tenant scheduling.

`suspend_binding(&BindingKey)` is a one-way, idempotent trusted-controller action: it blocks new enqueue/claims for that binding. Re-registration (including repository case aliases) cannot reactivate it. Existing message states and replay history are preserved. In-flight acceptance/UNKNOWN/receipt recording remains possible under the exact key after suspension, because suspension cannot undo a dispatched action. There is no automatic retry or resume path.

Legal transitions are `VERIFIED → DELIVERING → DELIVERED → ACKNOWLEDGED → COMPLETED`, `DELIVERING → UNKNOWN`, and `UNKNOWN → BLOCKED`. Use `begin_delivery`, `mark_delivered`, `acknowledge`, `complete`, `mark_unknown` and `block_unknown` respectively. Each method only records the trusted controller's observation; it cannot verify provider acceptance, worker receipts, work evidence or owner review itself. Wrong-state and missing-message operations return `StaleOrWrongState` without mutation. UNKNOWN/BLOCKED never return to a retryable state. `PENDING` ingress verification/rejection is not represented in Router storage.

Exclusive `&mut Router` access serializes one instance. No distributed locking, crash recovery, persistence, automatic retention cleanup or authenticated per-caller scope exists. A mutex test proves only external serialization of the in-memory model. Two logical thread IDs do not prove Codex-level thread isolation.

## Delivery semantics

- Future ingress must deduplicate GitHub webhook `X-GitHub-Delivery`; current Router independently deduplicates canonical repository/comment identities only.
- A received webhook is not an authorized message. Re-fetch object and verify sender/permission on the current repository.
- Never interpolate message body into a command or trusted system prompt.
- The trusted chat adapter may send the body as explicitly untrusted task context after owner authorization.
- Persist claim before dispatch. On timeout after dispatch, `UNKNOWN` must be reconciled before retry.
- `DELIVERED` means provider accepted. `ACKNOWLEDGED` means worker independently confirmed. `COMPLETED` requires work evidence.
- Use separate schemas for per-message receipts and agent output; never let output modify binding policy.

## MCP versus chat delivery

CWG_MCP exposes inbox/read/ack/reply tools to an already-running agent. An MCP server alone cannot push messages into every idle chat. CWG_Worker requires a provider-specific adapter (for Codex, an authorized App Server integration), or the agent must poll its inbox.

Status: design + initial in-memory typed models; no live webhook/MCP/provider transport.
