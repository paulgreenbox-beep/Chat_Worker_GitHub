# CWG protocol v1 (source foundation)

`cwg-message-v1`: `message_id`, `source.repository`, `source.issue_number`, `source.comment_id`, `source.author_id`, `target.provider`, `target.thread_id`, `goal_id`, `body`.

Binding key: GitHub repository identity + GOAL ID. Mutable chat titles are display metadata only. A production binding should additionally bind numeric GitHub repository ID, provider/account identity and a generation to resist renaming and stale-chat reuse.

## Implemented validation

- Schema is exactly `cwg-message-v1`.
- `message_id`: 1..128 ASCII bytes, letters/digits or `-_:`, globally unique within one Router. A future trusted ingress must allocate IDs across repositories, rather than letting comment text reserve arbitrary global IDs.
- Repository: `owner/name`; owner is 1..39 ASCII letters/digits or internal single hyphens; name is 1..100 ASCII letters/digits or `-_.`, excluding `.` and `..`. Router canonicalizes both parts to ASCII lowercase for binding and comment deduplication. It preserves the original message. Renaming/recreating repositories still requires future numeric identity/generation checks.
- `issue_number`, `comment_id`, `author_id`: nonzero `u64`. Raw JSON strings, floats (including exponential notation), negative/overflow numbers and null are rejected. A valid number does not prove the GitHub object exists or belongs to the sender.
- Provider: 1..32 ASCII bytes, a lowercase letter followed by lowercase letters, digits, `-` or `_`. Thread ID: 1..128 ASCII letters/digits or `-_:.`. Both are compared exactly; provider/account ownership is not authenticated by these strings.
- GOAL: `CWG-` or legacy `MSS-` plus 1..32 ASCII digits, at least one nonzero digit. Leading zeros are retained and significant; `CWG-1` and `CWG-001` are distinct GOALs.
- Body: 1..65,536 UTF-8 bytes, including unrestricted valid Unicode as untrusted context. Limits are bytes, not Unicode scalar counts.

`Message`, `GithubSource` and `ChatTarget` deserialization validates before returning a model. Each private wire representation rejects unknown and duplicate fields. Decode directly from raw JSON bytes into `Message`; first converting to a generic map/`serde_json::Value` loses duplicate keys and cannot be repaired by subsequent typed decoding. Reject malformed UTF-8/unpaired JSON surrogates in the JSON parser. A future receiver must also cap raw request bytes **before parsing/allocation**; this library only checks decoded field sizes. Public fields still allow direct Rust construction, so `validate` and Router admission also enforce constraints.

`InvalidMessage::{Schema, Identity, Size, Format}` classify schema, zero numeric identity, size and syntax failures respectively. Display errors do not echo message bodies. `DeliveryStatus` has exact uppercase wire names.

## Implemented in-memory routing API

`register` validates and inserts an immutable repository/GOAL → target mapping; repeated registration fails even for an identical target. It does **not** authenticate the caller or obtain owner approval. `enqueue_verified` revalidates the model and requires that exact target, then deduplicates global `message_id` and `(canonical repository, comment_id)` without partial insertion on failure. Its name describes a mandatory caller precondition, not verification performed inside Router.

Legal transitions are `VERIFIED → DELIVERING → DELIVERED → ACKNOWLEDGED → COMPLETED`, `DELIVERING → UNKNOWN`, and `UNKNOWN → BLOCKED`. Use `begin_delivery`, `mark_delivered`, `acknowledge`, `complete`, `mark_unknown` and `block_unknown` respectively. Each method only records the trusted controller's observation; it cannot verify provider acceptance, worker receipts, work evidence or owner review itself. Wrong-state and missing-message operations return `StaleOrWrongState` without mutation. UNKNOWN/BLOCKED never return to a retryable state. `PENDING` ingress verification/rejection is not represented in Router storage.

Exclusive `&mut Router` access serializes one instance. No distributed locking, crash recovery, persistence, capacity/retention limit or authenticated per-caller scope exists. A mutex test proves only external serialization of the in-memory model. Two logical thread IDs do not prove Codex-level thread isolation.

## Delivery semantics

- Github webhook `X-GitHub-Delivery` is deduplicated; canonical comment ID is independently deduplicated.
- A received webhook is not an authorized message. Re-fetch object and verify sender/permission on the current repository.
- Never interpolate message body into a command or trusted system prompt.
- The trusted chat adapter may send the body as explicitly untrusted task context after owner authorization.
- Persist claim before dispatch. On timeout after dispatch, `UNKNOWN` must be reconciled before retry.
- `DELIVERED` means provider accepted. `ACKNOWLEDGED` means worker independently confirmed. `COMPLETED` requires work evidence.
- Use separate schemas for per-message receipts and agent output; never let output modify binding policy.

## MCP versus chat delivery

CWG_MCP exposes inbox/read/ack/reply tools to an already-running agent. An MCP server alone cannot push messages into every idle chat. CWG_Worker requires a provider-specific adapter (for Codex, an authorized App Server integration), or the agent must poll its inbox.

Status: design + initial in-memory typed models; no live webhook/MCP/provider transport.
