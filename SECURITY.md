# Security model

CWG is a message transport across GitHub and AI conversations. GitHub content and model-generated text are **not trusted instructions**.

## Trust boundaries

1. **Ingress:** Verify `X-Hub-Signature-256` over exact raw webhook bytes using a protected HMAC key, constant-time comparison, event size limit and recognized event types. Re-read material facts from GitHub API and verify the installation/repository and current actor permissions. Validate the authorized person who approved delivery, not simply the Issue author.
2. **Binding:** An operator registers `repository + goal_id → provider + stable thread_id`. Authentication, permission to bind, and ownership must be external to mutable messages. Namespace IDs per account/provider; handle stale threads.
3. **Delivery:** Use durable, atomic claim and monotonic status transitions. Deduplicate webhook delivery IDs AND GitHub comment identities. A successful network write or API acknowledgement is not proof the worker executed a task. An uncertain dispatch is UNKNOWN, never automatic replay.
4. **Outbound:** Publishing a GitHub comment/PR needs separate credentials and policy from read/receive. Prevent cross-repository and cross-thread leakage. No secrets in public receipts.
5. **Workers:** The MCP client cannot necessarily disclose its own Codex thread identity. Use a trusted Codex App Server owner/adapter or explicit authorized registration; never accept an arbitrary thread_id from task text.

## Forbidden assumptions

- HMAC proves only delivery authenticity, not authorization of the author.
- Mutexes, Job Objects and in-process queues do not provide filesystem/network isolation.
- An external GitHub event must not execute shell/PowerShell, arbitrary commands or modify protected configs.
- Source snapshots, synthetic tests and self-attestation are not operating-system qualification.
- CWG_CheckBroker is disabled pending independent native qualification.

## Required negative tests

Forged signature, malformed JSON and duplicate keys, wrong repository/author, stale binding, unknown chat, replay, simultaneous claims, lost ACK, timeout, unauthorized publication, secrets exposure and reparse/path traversal when files are introduced.

## Implemented boundary and remaining risks

Current source implements structural message/binding validation, validated typed deserialization, raw-JSON unknown/duplicate-field rejection, canonical repository-name routing, in-memory deduplication and guarded state transitions. See [test matrix](doc/testing.md) and [source audit](audit/REPORT.md). Original unit tests remain present.

The ingress, binding owner authorization, GitHub actor authorization, numeric repository/account/generation binding, signatures, permission reconciliation, persistence, distributed atomic claims, provider receipts and outbound authorization described above remain requirements, not implemented guarantees. `register`, `enqueue_verified` and status methods are ordinary public Rust APIs with trusted-controller preconditions. They cannot refuse a caller based on identity because no caller authentication exists. Do not expose them directly to untrusted clients or invent an `approved: true` flag as authorization.

Public numeric IDs are syntactically validated but can still be spoofed by an untrusted caller. Global message IDs must be allocated by a trusted ingress to avoid cross-repository collision denial. Repository-name normalization closes case aliases, not rename/recreation attacks. The memory queue is unbounded and loses replay history on process restart. Raw JSON must be bounded before decoding; field validation occurs after allocation. Generic map/Value decoding discards duplicate keys, so future ingress must parse directly into the typed model. No body text is interpreted by Router as registration policy or executable code.

Tests with `Mutex<Router>` and two logical Codex thread IDs cover only local model behavior. Live GitHub delivery, real caller authorization, Codex thread isolation, crash/restart recovery and release acceptance remain **NOT_RUN**. No source-only result qualifies CWG for operation.
