# MCP source review and corrections — 2026-10-30

Starting PR #3 HEAD: `7931b04a7e3d1d0e9fa43086cbf1df6819e64eeb`.
Changed implementation: commits `7a7705a4007a4cebb62164e2ea5b27afab3a306c` (outbox) and `b5a6d7a3642e11fc1bb0c1384a8c93c1811d11cd` (stdio). This document does not certify those commits as compiled or Windows-tested.

## Corrected findings

### P2: visible is not necessarily persisted

Previously an identical report found in `worker-reports.jsonl` immediately returned `Ok(false)`, which the MCP tool projected as `persisted:true`. If a preceding append wrote all bytes but sync_all failed, a repeat could observe those cached bytes and claim durability without another successful synchronization.

All matching repeats now synchronize before success, under the existing file lock. A failure remains REPORT_COMMIT_UNKNOWN. No record is appended a second time, no torn record is repaired/truncated, and the complete journal is validated before returning an idempotent result. The new fault-I/O regression models an actual full-write/failed-sync result, a failed duplicate reconciliation and a later successful reconciliation. The seam tests persistence control flow only, not physical power-loss durability.

### P2: duplicate request bypassed replay capacity

The 4,096-report cap was checked only after the early duplicate return. An externally enlarged journal could therefore be accepted on a matching old ID. The cap is now checked for every replayed record before any duplicate success. Exactly-at-capacity reconciliation remains possible; new append is refused.

### P3: alternate journal representation

The writer emits named JSON objects, but derived struct deserialization can accept positional arrays. Replay now requires an object and retains the raw duplicate-key decoder. Existing valid object records remain compatible. Duplicate-key, positional-array, conflicting-identity, repeated stored-ID and torn-tail cases have regressions.

### P3: CRLF frame boundary

The frame reader allowed only one terminator byte before stripping CRLF. A MAX_RPC_BYTES payload worked with LF but failed with CRLF. Wire buffering now allows two terminator bytes, followed by an independent payload-size check. New tests cover capacities 1/7/8192, split CRLF, exact limit, limit+1 and preservation of the next frame.

## Test accounting

All existing tests/assertions are retained. Added **8 Rust tests**: 6 report-journal cases and 2 stdio framing cases. No external dependency, tool schema, dispatcher policy, credential or model call was added.

Rust compilation, rustfmt, Clippy, all existing and new Rust tests: **NOT_RUN** in this review environment (rustc/Cargo absent). This change must not inherit prior branch #1/#2 PASS counts. The separate read-only PR-stack audit tool in PR #1 was actually run with 11 passing Node/Git tests; that is not MCP runtime evidence.

## Remaining merge/deployment blockers

1. This branch still descends from old #2 source `94f5d4325831c57746c489029b1c234f3f8493f7`. Reconcile the latest #1/#2 security and State changes before deployment. Retain MCP `get_message`/`inbox_page` additions when resolving State, not the old pre-hardening journal implementation.
2. `Server::open` config access and the report journal still use path prechecks rather than the newer #2 guarded handle/owner/DACL boundary. Adopting #2 alone does not automatically protect the separate worker-reports file. Fresh protected provisioning, opened-handle and reparse/hardlink/concurrent-writer tests are still required. No claim of same-user/process isolation is made.
3. A configured binding is not authenticated Codex thread identity. Real client scope/launch authorization, provider delivery and outbound GitHub publication remain NOT_RUN/NOT_VERIFIED.
4. The legacy `validate_list_params` helper currently accepts nonobject params by falling through its optional-object check. This is a separate JSON-RPC conformance item, not an observed cross-chat authorization bypass. Validate/reject such frames and notification shapes in the next bounded protocol review.
5. Both advertised MCP protocol eras require validation against pinned official specifications and a real client; no compatibility certificate is supplied by these source changes.

## Required checks on the final combined source

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked --offline
cargo test -p cwg-mcp --all-targets --locked --offline
cargo test --workspace --all-targets --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo build --workspace --locked --offline
cargo build --workspace --release --locked --offline
git diff --check
```

Run the reviewed PR-stack ancestry gate after refreshing refs. Then qualify guarded config/report I/O on Windows and the real two-chat client scenario separately. Keep Draft; no merge or activation was performed.
