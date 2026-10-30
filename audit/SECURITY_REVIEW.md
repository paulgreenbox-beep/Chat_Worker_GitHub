# Draft PR #1 security follow-up — 2026-10-30

## Reviewed and checked revisions

- Reviewed PR head: `ea295640b71d281eb7c8825b896ec4719a795bca`.
- Base `main`: `a409120bbafeaefd7ad7569b7d5704e2ad6122d5`; no main edits.
- Checked implementation/tooling SHA: **`252eb7aecd76611ca7489da85074569b11fe2fb1`**.
- Authoritative Cargo evidence: [final-3/summary.json](evidence/final-3/summary.json), overall **PASS**, exit **0**. Scoped source inventory, file hashes, selected environment/config fingerprints and HEAD were stable before/after all seven gates, and scoped inputs matched the committed tree.
- Subsequent changes add this report and evidence only. Historical first-pass logs remain intact; [the old report](REPORT.md) identifies its earlier checked SHA explicitly.
- Host: Windows NT 10.0.26300.9711, x86_64-pc-windows-msvc. Rust `1.97.1 (8bab26f4f 2026-07-14)`; Cargo `1.97.1 (c980f4866 2026-06-30)`; Node `v24.22.1` for audit tests. No toolchain, executable or host setting was installed/changed.

The review used separate source reviews of Protocol, Router and evidence tooling, followed by implementation review and real command execution. These are source/model checks on the recorded host, not external security certification or real Codex delivery qualification.

## Findings and fixes

| Finding at reviewed head | Priority | Resolution in current modules |
|---|---|---|
| Complete positional JSON arrays were accepted as Message/Source/Target despite an object wire schema | P2 | Map-only `Object<T>` decoding at all three levels; strict required/unknown/duplicate field handling retained. Tests cover full arrays and escaped duplicate-key aliases. |
| Field validation happened after unrestricted model string allocation | P2 hardening | `Message::from_json_slice` checks a 400 KiB raw budget before parser entry; all wire strings have bounded visitors. Generic Serde scratch and network request collection still require external limits. |
| Debug could disclose untrusted message body in logs | P3 | Message Debug reports body byte length instead of body text. Bounded parse errors retain only classification/location, not the parser's input-quoting source error. Other identifiers/application logging are not a comprehensive privacy boundary. |
| Unlimited bindings/messages/comment dedupe/body accumulation | P2 | `RouterLimits` defaults: 1,024 bindings, 4,096 retained messages, 16 MiB body bytes. Checked arithmetic and all admission checks precede collection changes. Capacity refusal never evicts replay history. |
| Global bare message IDs caused cross-repository collision denial; state APIs lacked expected route correlation | P2 hardening | Storage IDs are scoped to canonical repository. All state queries/transitions require MessageKey containing repository, GOAL, provider/thread and message ID, with exact correlation before observation/mutation. This does not authenticate the caller. |
| Stale bindings could not be disabled without discarding Router/dedupe | P2 hardening | One-way idempotent `suspend_binding` closes admission and new claims. Messages, replay history and in-flight outcome recording remain available. Re-registration/case aliases cannot reactivate it. |
| Successful Cargo exits could leave PASS after inputs changed during gates | P2 | Repeat inventory/hash/HEAD/config/environment snapshots; observed drift causes overall FAIL. Newly added build inputs and HEAD changes are tested. Before/after checks do not detect ABA restoration or pin a complete host environment. |
| Existing/partial attempt directories and fixed source-review output could be overwritten or redirected through existing links | P2 | Atomic fresh attempt reservation, parent/link checks, directory identity checks and exclusive artifact writes. Each source review gets a new named attempt. Trusted-host checks do not provide an OS boundary against a malicious process racing path operations. |
| Windows case/JSON-escaped paths could evade redaction and publication scan | P2 | Case-aware Windows redaction and escaped path scanning with regression tests. No actual credential/private-path leak was found in the prior published evidence. |
| Protocol documentation described webhook delivery-ID dedupe as current behavior | P3 | Corrected to a future ingress requirement; current message/comment replay scope is explicit. |

No additional blocking defect was found in the reviewed final Protocol/Router source. That observation is bounded by the review and tests; it is not a claim that every possible security issue is absent.

## Actual verification results

| Проверка | Результат | Доказательство |
|---|---|---|
| Cargo metadata | PASS | `cargo metadata --format-version 1`, exit **0**, [log](evidence/final-3/metadata.txt) |
| Cargo fmt | PASS | `cargo fmt --all -- --check`, exit **0**, [log](evidence/final-3/fmt.txt) |
| Cargo check | PASS | `cargo check --workspace --all-targets`, exit **0**, [log](evidence/final-3/check.txt) |
| Cargo test | PASS | `cargo test --workspace --all-targets`, exit **0**, **67 passed / 0 failed / 0 ignored**, [log](evidence/final-3/test.txt) |
| Cargo clippy | PASS | `cargo clippy --workspace --all-targets -- -D warnings`, exit **0**, [log](evidence/final-3/clippy.txt) |
| Debug build | PASS | `cargo build --workspace`, exit **0**, [log](evidence/final-3/debug.txt) |
| Release build | PASS | `cargo build --workspace --release`, exit **0**, [log](evidence/final-3/release.txt) |
| Locked/offline test | PASS | `cargo test --workspace --all-targets --locked --offline`, exit **0**, **67 passed / 0 failed / 0 ignored**, [transcript](evidence/review-2/locked-offline.json) |
| Audit tooling tests | **14 PASS; 1 NOT_RUN** | `node --test audit/tooling.test.mjs`, exit **0**, 15 scenarios, 0 failed, 1 skipped; [transcript](evidence/review-2/node-tests.json) |
| Source/dependency/publication observations | PASS within scope | `node audit/review.mjs review-3`, exit **0**, [result](evidence/review-3/source-review.json); locked metadata, zero dependency cycles and no pattern findings |
| Evidence/test retention | PASS | All **30** published Cargo log hashes match; all **38** previous test names appear in the new successful run; [receipt](evidence/review-2/integrity.json), exit **0** |
| Git diff checks | PASS | `git diff --check`, full base and staged checks, exit **0**, [receipt](evidence/review-2/diff-check.json) |
| Security review | **ISSUES_FOUND** | Implemented-scope fixes above; remaining external trust/runtime requirements below |

The single Node NOT_RUN is the file-symlink artifact-write fixture: Windows refused file symlink creation with `EPERM`, including the run with filesystem escalation. The directory-junction rejection test **ran and passed**, as did rejection of existing regular files, partial/repeated attempt names, and exclusive write replacement. No security setting was weakened to force the skipped scenario to pass.

Node tooling tests deliberately inject synthetic command callbacks inside ignored local fixture repositories to test evidence classification. Those callbacks are **not Cargo execution evidence**. Actual Cargo results above come from the default CLI invoking installed Cargo. Fixtures are retained only under ignored `audit/.scratch/` and are not published.

Pre-fix scoped regression runs also executed: Protocol `cargo test -p cwg-protocol --test hardening_regressions` exited **101** (0 passed, 4 failed: root/source/target arrays and body Debug); Router `cargo test -p cwg-router --test security_regressions` exited **101** (0 passed, 3 failed: default binding/message capacity and cross-repository message ID collision). These refer to the initial regression patches against the reviewed implementation; the current files additionally contain scoped-key assertions. Their full pre-fix transcripts were not saved in this repository. Final assertions and complete post-fix outputs are in final-3; no old-file hash is misattributed to the initial regression patches.

## New tests and API migration

- Protocol: 13 new tests (4 regressions, 9 bounded JSON tests); now **31** tests including all previous 18. Raw-size exact/+1, escaped maximum decoded body, decoded field budgets, malformed Unicode, complete documents, object-only shapes, escaped duplicate keys and diagnostics are covered.
- Router: 16 new tests (3 regressions, 12 bounds/scope/suspension integration tests, 1 checked-add overflow unit test); now **36** tests including all previous 20. Every transition/query refuses mismatched scope; capacity failure does not reserve IDs or release replay tombstones. Suspension closes enqueue/claim but permits existing in-flight outcomes without retry.
- Total Rust: **29 added + 38 retained = 67**. The three original foundation unit-test scenarios/assertions are retained. Router tests were migrated from bare string arguments to the explicit MessageKey API without weakening expectations.
- Audit: 15 new builtin Node scenarios, including expected overall FAIL despite synthetic successful commands after source/HEAD/config/environment drift, missing test-summary refusal, exclusive paths, links/junctions, path redaction and historical report preservation.
- All Router state methods now take `&MessageKey`. Construct it from a validated message before enqueue; keep the exact repository/GOAL/target correlation. There is no compatibility fallback that silently selects by a bare ID. Same-ID messages in distinct repositories are independent; same-ID replay in one repository still conflicts.
- New Protocol API: `Message::from_json_slice`, `MAX_JSON_BYTES`, `MessageParseError`. This is a decoder, not a Webhook Receiver. serde_json **1.0.154** moved from dev-only to runtime because raw-byte guarding must precede parser allocation. Cargo.lock and registry versions/checksums did not change. Licenses remain recorded in the source-review result; no new package version was introduced.

## Remaining boundaries

The changes do not establish caller authentication, owner approval, GitHub actor permission or numeric object ownership. Anyone already holding unrestricted in-process Router access and a complete matching key is still a trusted-controller caller, not an authenticated external principal. No fake `approved` flag or synthetic identity mechanism was added.

Repository numeric identity/account/generation binding, automatic stale-thread detection, durable restart/crash recovery, distributed claims, fair per-tenant scheduling and real receipt/work evidence remain future requirements. Finite budgets are not exact allocator quotas. Suspension is not provider cancellation. Generic Serde callers must impose their own raw bound, and a network receiver must bound data before collecting a slice. A fresh Router loses replay history and is not a recovery policy.

Audit provenance now rejects observed net drift and existing output destinations. It does not detect temporary input changes restored before the after-snapshot, include every possible generated/include dependency, pin all registry/tool binaries, or stop a hostile same-user process racing filesystem checks. External Cargo configs/wrappers and selected environment controls are fingerprinted with values kept out of publication; that is not hermetic build proof. The source scan is targeted, not proof against every possible secret or an advisory database audit.

**NOT_RUN:** the file-symlink fixture above; Webhook/HMAC/actor reconciliation; MCP; Codex Adapter; real delivery/receipts/completion; actual Codex thread/account isolation; persistent recovery/distributed execution; other platforms/MSRV; clean-machine/bit-for-bit builds; dependency advisory database scan and runtime/release acceptance. Webhook, MCP and Codex Adapter were not implemented. MSS/Makepad were not changed. No Docker/WSL/WSLC/VM or interactive-desktop test was used. PR remains Draft with automatic merge disabled.

## Reproduction

Use the recorded compiler and source SHA, then run `node audit/verify.mjs final-N` with a fresh name, `node --test audit/tooling.test.mjs`, `cargo test --workspace --all-targets --locked --offline`, and `node audit/review.mjs review-N` with a fresh name. Failed/partial attempts are preserved rather than reused. Review the overall summary status and failures, not just individual Cargo exit codes. See [test matrix](../doc/testing.md) for the implemented boundary and evidence encoding.

## Changed files in this follow-up

The final publication scan is also recorded in `audit/evidence/review-3/source-review.json` (created after the list was generated).

- `.gitattributes`
- `.gitignore`
- `audit/evidence/final-3/check.txt`
- `audit/evidence/final-3/clippy.txt`
- `audit/evidence/final-3/debug.txt`
- `audit/evidence/final-3/fmt.txt`
- `audit/evidence/final-3/metadata.txt`
- `audit/evidence/final-3/release.txt`
- `audit/evidence/final-3/summary.json`
- `audit/evidence/final-3/test.txt`
- `audit/evidence/review-2/diff-check.json`
- `audit/evidence/review-2/integrity.json`
- `audit/evidence/review-2/locked-offline.json`
- `audit/evidence/review-2/node-tests.json`
- `audit/evidence/review-2/source-review.json`
- `audit/REPORT.md`
- `audit/review.mjs`
- `audit/SECURITY_REVIEW.md`
- `audit/support.mjs`
- `audit/tooling.test.mjs`
- `audit/verify.mjs`
- `CWG_Protocol/Cargo.toml`
- `CWG_Protocol/src/lib.rs`
- `CWG_Protocol/tests/hardening_regressions.rs`
- `CWG_Protocol/tests/raw_json.rs`
- `CWG_Router/src/lib.rs`
- `CWG_Router/tests/audit_regressions.rs`
- `CWG_Router/tests/routing.rs`
- `CWG_Router/tests/security_bounds_and_scope.rs`
- `CWG_Router/tests/security_regressions.rs`
- `doc/maps.md`
- `doc/migration.md`
- `doc/protocol.md`
- `doc/roadmap.md`
- `doc/testing.md`
- `README.md`
- `SECURITY.md`
