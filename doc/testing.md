# Verification matrix

PASS means the named check actually ran and met its assertion on the recorded source/platform. FAIL means an executed check failed. NOT_RUN means no execution evidence exists. These results cover different boundaries; compilation, unit/model checks, source review, actual delivery and release acceptance are not interchangeable.

CHEAPEST_VERIFICATION_PATH: for Protocol edits, `cargo test -p cwg-protocol --test validation`; for Router edits, `cargo test -p cwg-router`. Final acceptance runs the seven workspace gates requested by the owner in order, followed by a locked/offline test and `git diff --check`. See [audit logs](../audit/evidence/) and [report](../audit/REPORT.md).

| Owner / changed boundary | Tests / evidence | Scope |
|---|---|---|
| CWG_Protocol: schema, message ID, thread ID | `validation.rs`: wrong schema/empty identifiers, lengths and ASCII checks | Direct validation and typed raw-JSON deserialization |
| CWG_Protocol: repository, GOAL, provider | Invalid repository names, GOAL suffixes, provider grammar and boundary round trips | Syntax only; not ownership |
| CWG_Protocol: body / Unicode | Oversized ASCII/multibyte bodies, invalid UTF-8/surrogates, valid Unicode round trips | Decoded UTF-8 byte size; not raw ingress limits |
| CWG_Protocol: JSON structure | Every duplicate recognized field, unknown root/nested fields, missing fields, malformed/trailing JSON | Direct raw JSON parsing; not a previously decoded generic map |
| CWG_Protocol: numeric IDs and statuses | Zero/negative/fraction/string/null/overflow IDs, nonzero u64 bounds, all status wire names, error categories | Syntax/classification only |
| CWG_Router: bindings | Two repositories, two logical Codex chats, different/simultaneous GOALs, malformed/duplicate bindings, provider/thread mismatch | Logical exact routing; no caller authorization |
| CWG_Router: replay / case aliases | Duplicate message/comment IDs, canonical repository names, admission failure without partial insertion | Per-instance memory history only |
| CWG_Router: state transitions | All 7 reachable states × 6 operations, missing IDs, UNKNOWN replay/retry refusal, premature ACK regression | Trusted-controller observations; no actual receipts |
| CWG_Router: contention / untrusted text | Two competing claims and simultaneous GOALs under external mutex; body cannot register a GOAL | Single-process model; not distributed or Codex isolation |
| Existing unit tests | Protocol `rejects_empty_body`; Router `exact_binding_and_duplicate_refusal`, `uncertain_delivery_does_not_retry` | All retained, no assertion removed or weakened |
| Workspace | Cargo metadata/fmt/check/test/clippy/debug/release, committed Cargo.lock, dependency graph/source scan | Recorded Windows x86_64 compiler only |
| Future components | Signature/actor authorization, actual chat delivery, persistent recovery, outbound publication | NOT_RUN: components not implemented |

`serde_json` is a dev-dependency used to exercise the raw JSON format (including malformed bytes and duplicate keys); runtime dependencies stay `serde` plus the workspace path edge. Its registry version/checksum and transitive versions are pinned in Cargo.lock. `.gitattributes` keeps source line endings stable and preserves exact evidence bytes across checkouts. No new toolchain or executable is installed by the audit.

Run `node audit/verify.mjs final` to record the exact seven Cargo gates with command/exit code/test totals and redacted host paths. Existing evidence is never overwritten; use `final-2`, etc. for subsequent attempts. Node is optional audit tooling, not a Rust/runtime dependency. Run `node audit/review.mjs` for a locked/offline dependency graph and targeted publication scan. Logs are real command output with workspace/home paths replaced by `<workspace>`/`<user-home>`; summary SHA-256 values cover those published, redacted bytes. A targeted scan is not a package advisory database audit.

If a transcript contains trailing whitespace (for example a failing rustfmt diff), its `.txt` artifact contains a lossless JSON envelope with a `transcript` string. Decode that string to recover every redacted transcript byte. `transcript_sha256` covers the decoded transcript; `log_sha256` covers the published envelope. This changes the encoding only; original FAIL results and diagnostic whitespace are preserved.
