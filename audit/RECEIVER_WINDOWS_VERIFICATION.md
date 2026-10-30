# CWG Windows native verification — PR #2

Status: **SOURCE GATES PASS — Draft for independent review; deployment NOT_QUALIFIED**.
This report records observed results separately from source review and planned
verification. No SECURITY PASS or deployment approval follows from unit tests.
Historical evidence is preserved without relabelling its outcomes.

## Scope and source identity

The attached Windows Native QA brief defines this review: CWG_Security, the
CWG_GitHub receiver/API client, CWG_State and their local integration. Protocol
and Router retain the separately reviewed PR #1 contracts. This work does not
implement MCP, a Codex Adapter, outbound publishing or a public proxy.

| Source | Exact commit |
|---|---|
| Original foundation requested against main | `a409120bbafeaefd7ad7569b7d5704e2ad6122d5` |
| Integrated PR #1 security-review dependency | `a7aa73858a85620f4ca10f09fcf75d86186f662c` |
| Initial PR #2 source actually baselined | `94f5d4325831c57746c489029b1c234f3f8493f7` |
| Initial security/native regression source | `9a9f8fc7511b51b83ef61964cea1df62a02fb274` |
| Commit/fsync and first retained-access regression source | `ffa21820dcb22c5d5f39f5d6713574d3c075377b` |
| First attempted corrected workspace run | `f5f0def5618e78fa2bf05b30eda2a2dffdb584ad` |
| Qualified retained-access regression source | `260517f85998ab1aa4b9943fc2bebc2d6a607f41` |
| Second attempted corrected workspace run | `2581947ccf267caf97a44ab5c91c459c75d26f38` |
| Final verified source | `0963e4269e870d419e1f871b62a670625704b78c` |

The actual PR #2 base is `codex/cwg-build-test-audit` at the PR #1 SHA above,
not main. `git merge-base --is-ancestor a7aa73858a85620f4ca10f09fcf75d86186f662c 94f5d4325831c57746c489029b1c234f3f8493f7`
returned exit 0. Both Draft PR heads/base metadata were re-read from GitHub;
main and PR #1 were not modified.

The first baseline is not final acceptance. In particular, its 103 passing tests
did not include the later regressions which exposed defects. A later evidence
commit may add this report and logs; the checked source SHA must remain explicit.

## Platform and execution boundary

Observed host: Windows 11 Pro, build **10.0.26300.9711**, x64, on a GIGABYTE
AERO X16 1VH laptop. The saved CIM result reports version `10.0.26300`, a 64-bit
OS, and `HypervisorPresent: true`; that flag does not establish that this work
ran in a guest VM. No Docker, WSL, WSLC or VM was launched for these checks.
See [the successful platform query](evidence/receiver-baseline/platform-2.json).
The fixture volume is a fixed local NTFS disk ([native volume receipt](evidence/receiver-baseline/platform-3.json)); its original CRLF bytes are preserved.
The preceding [platform query](evidence/receiver-baseline/platform.json) returned
null OS/machine fields and is not platform proof.

The actual tool versions recorded by the runner are:

- `rustc 1.97.1 (8bab26f4f 2026-07-14)`, host `x86_64-pc-windows-msvc`;
- `cargo 1.97.1 (c980f4866 2026-06-30)`;
- LLVM `22.1.6`.

Native fixture execution requires permission to open owned loopback sockets and
create the specifically owned test files. The original default temporary parent
did not satisfy the product ancestor-ACL policy. Subsequent fixtures select a
qualified parent beneath the existing user profile through a **test-only**
setting. Its path is hashed in evidence rather than published. Product ACL
checks are unchanged; this is not a production path or network override.

No real working GitHub account or token is used for adversarial tests. API
fixtures bind owned loopback endpoints and send no Authorization or Cookie.
Production remains fixed to HTTPS `api.github.com:443`.

## Initial baseline: observed commands

All runner Cargo commands use `CARGO_NET_OFFLINE=true`. The explicit offline
test is a second execution, not another 103 distinct test cases. The full
[baseline summary](evidence/receiver-baseline/summary.json) records tool output,
source fingerprints, transcript encoding and SHA-256 values for every log.

| Check | Command | Exit | Observed result | Evidence |
|---|---|---:|---|---|
| Metadata | `cargo metadata --format-version 1` | 0 | PASS | [metadata](evidence/receiver-baseline/metadata.txt) |
| Format | `cargo fmt --all -- --check` | 1 | FAIL | [format diff](evidence/receiver-baseline/fmt.txt) |
| Check | `cargo check --workspace --all-targets --locked` | 0 | PASS | [check](evidence/receiver-baseline/check.txt) |
| Tests | `cargo test --workspace --all-targets --locked` | 0 | 103 PASS, 0 FAIL, 0 ignored | [tests](evidence/receiver-baseline/test.txt) |
| Clippy | `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | PASS | [Clippy](evidence/receiver-baseline/clippy.txt) |
| Debug build | `cargo build --workspace --locked` | 0 | PASS | [debug](evidence/receiver-baseline/debug.txt) |
| Release build | `cargo build --workspace --release --locked` | 0 | PASS | [release](evidence/receiver-baseline/release.txt) |
| Offline tests | `cargo test --workspace --all-targets --locked --offline` | 0 | 103 PASS, 0 FAIL, 0 ignored | [offline tests](evidence/receiver-baseline/offline-test.txt) |
| Diff whitespace | `git diff --check` | 0 | PASS | [diff check](evidence/receiver-baseline/diff-check.txt) |

## Regressions: observed failures before their fixes

Commands below are the exact recorded subprocess commands, not proposed checks.
Every phase has its own immutable attempt directory and summary. These results
must remain FAIL even if a later attempt passes.

| Phase / source | Command | Exit | Observed counts | Interpretation |
|---|---|---:|---|---|
| [Security / `9a9f8fc`](evidence/receiver-regression-security/summary.json) | `cargo test -p cwg-security --locked --offline cng_ -- --nocapture` | 101 | 6 PASS / 2 FAIL | CNG cleanup and compound failures were misclassified. |
| [State / `9a9f8fc`](evidence/receiver-regression-state/summary.json) | `cargo test -p cwg-state --test durable --locked --offline security_regression_ -- --nocapture` | 101 | 0 PASS / 4 FAIL | Durable aliases, cross-source alias correlation, record shape and private-root enforcement failed. |
| [GitHub / `9a9f8fc`](evidence/receiver-regression-github/summary.json) | `cargo test -p cwg-github --lib --locked --offline audit_ -- --nocapture` | 101 | 25 PASS / 3 FAIL | Config/Event/API typed positional arrays bypassed the required object shape. |
| [HTTP / `9a9f8fc`](evidence/receiver-regression-http/summary.json) | `cargo test -p cwg-github --lib --locked --offline http_security_regression_ -- --nocapture` | 101 | 0 PASS / 4 FAIL | Empty Host accepted; absent/malformed signature and unsupported event were checked after body reads. |
| [WinHTTP / `9a9f8fc`](evidence/receiver-regression-winhttp/summary.json) | `cargo test -p cwg-github --bin cwg-github --locked --offline audit_ -- --nocapture` | 101 | 6 PASS / 5 FAIL | Missing explicit TLS/retry policy, ignored close failure, observed retry default 5, and an accepted truncated response. |
| [Commit/fsync / `ffa2182`](evidence/receiver-regression-commit/summary.json) | `cargo test -p cwg-state --lib --locked --offline commit_tests -- --nocapture` | 101 | 1 PASS / 2 FAIL | A visible append with failed sync could later receive no-op success without a successful reconciliation sync. |
| [Retained access / `ffa2182`](evidence/receiver-regression-retained/summary.json) | `cargo test -p cwg-state --test retained_access --locked --offline security_regression_ -- --nocapture` | 101 | 0 PASS / 2 FAIL | **INCONCLUSIVE for sharing:** fixture setup failed with PermissionDenied before the intended assertions. |
| [Retained access, escalated / `ffa2182`](evidence/receiver-regression-retained-2/summary.json) | `cargo test -p cwg-state --test retained_access --locked --offline security_regression_ -- --nocapture` | 101 | 0 PASS / 2 FAIL | **INCONCLUSIVE for sharing:** the after-drop positive control failed with `ACL_UNTRUSTED_ACCESS` on the fixture ancestor. |
| [Retained access, qualified parent / `260517f`](evidence/receiver-regression-retained-3/summary.json) | `cargo test -p cwg-state --test retained_access --locked --offline security_regression_ -- --nocapture` | 101 | 0 PASS / 2 FAIL | Both intended refusal assertions failed with usable fixtures; this is the retained writer/mapping regression evidence. |
| [Workspace / `f5f0def`](evidence/receiver-regression-workspace/summary.json) | `cargo test --workspace --all-targets --locked --offline` | 4294967295 (`0xffffffff`) | No complete summary | **INTERRUPTED**, not a completed suite. Restricted loopback fixtures blocked; the controller terminated only the owned test process. The old signed fixture also encountered an unsuitable inherited ACL. |
| [Workspace, second attempt / `2581947`](evidence/receiver-regression-workspace-2/summary.json) | `cargo test --workspace --all-targets --locked --offline` | 101 | Tests NOT_RUN — compilation failed | Including the native fixture helper in the GitHub library tests violated its existing `#![forbid(unsafe_code)]`. The helper import was replaced with safe, test-only system-tool fixture setup; the lint remains unchanged. |

The two inconclusive retained-access attempts are neither sharing bypass proof
nor successful protection tests. The qualified third attempt supplies separate,
actual failure evidence. None of these attempts grants a final PASS to changed
source. The interrupted workspace output contains individual results but cannot
be summed into a complete test count.

The qualified retained-access source can be reconstructed from `ffa2182` using
the lossless UTF-8 [patch envelope](retained-fixture-repro.json). The controller
verified `git worktree add --detach target/retained-reconstruct
ffa21820dcb22c5d5f39f5d6713574d3c075377b`, followed by applying the decoded patch
with `git am --committer-date-is-author-date`; the resulting commit was exactly
`260517f85998ab1aa4b9943fc2bebc2d6a607f41`. This reconstruction changes fixture
preparation, not production ACL policy. It created no extra GitHub branch/tag.
Decode the envelope's `patch` string to a fresh owned temporary file before
reproducing the command; its decoded SHA-256 is
`e60211aaa8ef5e09be5c58ff81efcbbf7f0bd97014b1f51b656f5125cec4d3b0`.
The JSON envelope retains legitimate format-patch context spaces without
weakening repository whitespace checks.

## Later attempts and fixture corrections

All unsuccessful attempts remain FAIL. Cargo normally stops after a failed target, so counts from interrupted/early-failed runs describe completed targets only. Diagnostic runs with `scoped_inputs_match_head=false` recorded exact worktree hashes and are not final commit acceptance.

| Attempt | Source | Observed result / command | Completed test counts |
|---|---|---|---|
| [receiver-diagnostic-winhttp-1](evidence/receiver-diagnostic-winhttp-1/summary.json) | `5fec3c391476de756202a70d341a018ad921900b` plus recorded worktree hashes | PASS; `cargo test -p cwg-github --bin cwg-github --locked --offline windows::tests::audit_native_exact_response_boundary_and_missing_length -- --exact --nocapture`; exit 0 | 1 PASS / 0 FAIL / 0 ignored |
| [receiver-diagnostic-winhttp-2](evidence/receiver-diagnostic-winhttp-2/summary.json) | `5fec3c391476de756202a70d341a018ad921900b` plus recorded worktree hashes | PASS; `cargo test -p cwg-github --bin cwg-github --locked --offline windows::tests::audit_native_exact_response_boundary_and_missing_length -- --exact --nocapture`; exit 0 | 1 PASS / 0 FAIL / 0 ignored |
| [receiver-diagnostic-winhttp-3](evidence/receiver-diagnostic-winhttp-3/summary.json) | `5fec3c391476de756202a70d341a018ad921900b` plus recorded worktree hashes | FAIL; `cargo test -p cwg-github --bin cwg-github --locked --offline -- --nocapture`; exit 101 | 17 PASS / 2 FAIL / 0 ignored |
| [receiver-final](evidence/receiver-final/summary.json) | `f20edeb96dcc60033de2dd5ba01e3e273003c464` | FAIL; Nine sequential gates; every exact command/exit is in the linked summary | 55 PASS / 0 FAIL / 0 ignored; 201 PASS / 0 FAIL / 0 ignored |
| [receiver-final-2](evidence/receiver-final-2/summary.json) | `b7befe6f78c679b634f0065fb058519860cb0dac` | FAIL; Nine sequential gates; every exact command/exit is in the linked summary | 202 PASS / 0 FAIL / 0 ignored; 75 PASS / 1 FAIL / 0 ignored |
| [receiver-final-3](evidence/receiver-final-3/summary.json) | `430f39abcb7475ee5333d535df74c174e826a35e` | PASS; Nine sequential gates; every exact command/exit is in the linked summary | 204 PASS / 0 FAIL / 0 ignored; 204 PASS / 0 FAIL / 0 ignored |
| [receiver-regression-delayed](evidence/receiver-regression-delayed/summary.json) | `87bf108a71b2fc549e6db01765f3b932254a30d8` | FAIL; `cargo test -p cwg-github --bin cwg-github --locked --offline audit_fixture_accepts_headers -- --nocapture`; exit 101 | 0 PASS / 1 FAIL / 0 ignored |
| [receiver-regression-fixture](evidence/receiver-regression-fixture/summary.json) | `16dfb71cd55b23bbf785b608ed3456f1fd4d9a24` | FAIL; `cargo test -p cwg-state --test windows_paths --locked --offline fixture_fence_ -- --nocapture`; exit 101 | 0 PASS / 2 FAIL / 0 ignored |
| [receiver-regression-pool](evidence/receiver-regression-pool/summary.json) | `bb2fb58c2dcd30a06417106ab4ad62e62a607a2e` | FAIL; `cargo test -p cwg-github --bin cwg-github --locked --offline audit_sessions_refuse_global -- --nocapture`; exit 101 | 0 PASS / 1 FAIL / 0 ignored |
| [receiver-regression-server](evidence/receiver-regression-server/summary.json) | `3cba27597cb32e599c59141829365d23e068553e` | FAIL; `cargo test -p cwg-github --bin cwg-github --locked --offline audit_fixture_refuses_an_interrupted -- --nocapture`; exit 101 | 0 PASS / 1 FAIL / 0 ignored |
| [receiver-regression-server-2](evidence/receiver-regression-server-2/summary.json) | `12c4979b32384582aaee8eb21235930475a78faa` | FAIL; `cargo test -p cwg-github --bin cwg-github --locked --offline audit_fixture_ -- --nocapture`; exit 101 | 2 PASS / 1 FAIL / 0 ignored |
| [receiver-regression-workspace-3](evidence/receiver-regression-workspace-3/summary.json) | `d86913ba1b0120d84741f590462cb0dbb8129aa0` | FAIL; `cargo test --workspace --all-targets --locked --offline`; exit 101 | 73 PASS / 1 FAIL / 0 ignored |
| [receiver-regression-workspace-4](evidence/receiver-regression-workspace-4/summary.json) | `4cd0b11dc4c8009be93699f179c97630a4cf105a` | FAIL; `cargo test --workspace --all-targets --locked --offline`; exit 101 | 73 PASS / 1 FAIL / 0 ignored |
| [receiver-regression-workspace-5](evidence/receiver-regression-workspace-5/summary.json) | `5fec3c391476de756202a70d341a018ad921900b` | FAIL; `cargo test --workspace --all-targets --locked --offline --no-fail-fast`; exit 101 | 196 PASS / 2 FAIL / 0 ignored |
| [receiver-regression-workspace-6](evidence/receiver-regression-workspace-6/summary.json) | `ad68ea79cb7a41bf831c7c71009d3330fac1db91` | FAIL; `cargo test --workspace --all-targets --locked --offline --no-fail-fast`; exit 101 | 198 PASS / 2 FAIL / 0 ignored |
| [receiver-regression-workspace-7](evidence/receiver-regression-workspace-7/summary.json) | `43f05420ff3686b1518340eacf3dcd411c714ac5` | FAIL; `cargo test --workspace --all-targets --locked --offline --no-fail-fast`; exit 101 | 199 PASS / 1 FAIL / 0 ignored |
| [receiver-regression-workspace-8](evidence/receiver-regression-workspace-8/summary.json) | `2a48df292547b3fce051ce8d71bb2634f25f0950` | PASS; `cargo test --workspace --all-targets --locked --offline --no-fail-fast`; exit 0 | 200 PASS / 0 FAIL / 0 ignored |
| [receiver-regression-workspace-9](evidence/receiver-regression-workspace-9/summary.json) | `2a48df292547b3fce051ce8d71bb2634f25f0950` | PASS; `cargo test --workspace --all-targets --locked --offline --no-fail-fast`; exit 0 | 200 PASS / 0 FAIL / 0 ignored |

The first final attempt at `f20edeb96dcc60033de2dd5ba01e3e273003c464` had a native binary exit `0xc0000409` (3221226505); 55 tests had completed before that process failure. It is **UNEXPLAINED_NATIVE_PROCESS_FAILURE**, not proof of a buffer overflow or an OS bug. Its immediately later 201-test offline PASS is a separate result and does not erase it. No dump/fast-fail subcode was collected.

Native diagnostics then captured `complete=false`, zero request bytes and WinHTTP 12152; earlier diagnostics also captured 12030. These errors alone do not prove a TCP reset. Windows accepted sockets inherit the listening socket properties, while receive timeouts apply to blocking reads. The fixtures had nonblocking listeners and failed to reset accepted sockets. `WouldBlock` was treated as EOF; writes could also stop early. A dedicated 50 ms delayed-header native TCP regression failed before the fix, independently of WinHTTP. Accepted fixture sockets now explicitly reset blocking mode before bounded reads/writes. Empty/incomplete connections do not consume the intended fixture transaction or receive success.

The old fixture also failed a controlled empty-connection-then-valid-request regression; this is now PASS. Fixture worker cleanup now preserves the original failure without starting a second panic during unwinding. This repairs a plausible abort path, but does **not** forensically attribute the historical fast-fail without a subcode/stack. Fixture limits are at most eight accepts, an accept deadline and per-socket timeouts; they are not a strict global four-second wall-clock guarantee.

The private-pool A/B at `2a48df292547b3fce051ce8d71bb2634f25f0950` passed 200 tests in each arm. That PASS/PASS does not establish pooling as the cause. Disabling cross-session pooling was subsequently added as separate recommended transport-state hardening, with a failing policy regression beforehand. The diagnostic environment switch was removed from the client; final code always applies the checked production setting. No retry, fallback or test serialization was introduced.

Two test-only filesystem helper defects also had recorded pre-fix FAILs: creating a junction path before checking the owned-root fence, and mutating ACLs through a hardlink alias. Ownership/reparse checks now precede creation; the same opened file handle verifies a single link before ACL mutation. Mapping cleanup attempts both releases before assertions. No actual call altered an unowned host path. Strict Clippy findings were fixed by ordering the test module, using byte-string syntax and simplifying fixture enum names; the unsafe-free library policy was preserved.

## Corrections present in the source under review

These source-review conclusions are paired with the executed final gate table below. No original test assertions were removed to obtain a
pass, and error-path tests remain in the suite.

| Priority | Defect and correction | Regression / verification boundary |
|---|---|---|
| P1 | A new delivery ID for an existing comment was not durably recorded. Persistent `DeliveryAlias` records now retain that correlation and reject alias reuse for another source. | State reopen and cross-source alias tests; original FAIL recorded. |
| P1 | State relied on an owner-private directory assumption. Real owner/DACL, opened-handle and local-filesystem checks now guard config and journal access. | Actual unprotected ACL regression; native config/ACL fixtures. |
| P1 | Existing write handles or writable mappings survive DACL tightening. Leaf sharing policy refuses other writers while the guard lives; checked ancestor handles pin names against rename/delete. | Qualified retained-access FAIL; corrected-source native regressions PASS in the final suite. Fresh private provisioning remains required. |
| P1 | No-op reconciliation could acknowledge an unsynchronized visible journal prefix. Every successful transaction now synchronizes observed bytes, including a duplicate/read/no-op. | Injected append/fsync regression: 1 PASS / 2 FAIL before fix. Physical power-loss remains NOT_VERIFIED. |
| P2 | Serde could interpret named security models as positional arrays. Private map-only typed boundaries now cover Config, Route, every nested GitHub identity, Binding/Pending and journal records. | Nested-array negatives; generic `Value` arrays remain valid JSON and are not globally forbidden. |
| P2 | HTTP envelope checks happened after body read and empty Host was accepted. Validate framing/event/delivery/signature first; apply a shared deadline across header/body, including post-read checks. | Four captured HTTP FAILs; native slow/interrupted stream tests and header/body bounds. |
| P2 | WinHTTP depended on host TLS/retry defaults. Native tests set explicit TLS 1.2 and one connection attempt, check setter success, and query the retry count. | Captured policy/default FAILs. SECURE_PROTOCOLS is set-only: its unsupported query is tested as a refusal, not TLS readback or a production handshake. |
| P2 | WinHTTP accepted a truncated response. Declared response length is queried, bounded and matched exactly; query failures other than absent header refuse. | Actual native truncated-transfer FAIL plus boundary/query-error tests. |
| P2 | Native cleanup failures were lost or confused with operation failure. CNG/WinHTTP return explicit cleanup or compound classes, with one close attempt and no blind retry. | Captured CNG and WinHTTP FAILs; native owned-handle accounting plus injected failure branches. |
| P3 | Test setup inherited unsuitable ACLs or could wait indefinitely for a denied loopback connection. Owned private fixtures, explicit test parent and bounded fixture accepts replace those assumptions. | Initial unsuccessful attempts preserved; corrected fixture controls PASS in the final suite. |

## Twenty required GitHub negative cases

“Pure” means in-process structured inputs or an injected API reply. “Native”
means actual Windows system APIs or owned local TCP/filesystem resources. The final suite executed these tests on the recorded SHA, with zero failures or ignored tests. Mock API reconciliation and native transport
tests compose different boundaries; they are not twenty live GitHub tests.

| # | Scenario | Test and coverage type |
|---:|---|---|
| 1 | Unauthorized sender | Pure `unauthorized_sender_causes_no_api_requests`; native receiver E2E rejects an independently signed unauthorized actor before API access. |
| 2 | Forged repository ID | Pure recreated-repository and API repository-ID negatives; route chosen by configured numeric ID. |
| 3 | Repository name/ID mismatch | Pure API repository name/owner/ID negatives and source-owner transfer refusal. |
| 4 | Forged comment ID | Pure API comment zero/ID reconciliation negatives. |
| 5 | Webhook/API author mismatch | Pure comment author ID/login/type and permission identity negatives. |
| 6 | Revoked author permission | Pure revoked permission and none/read/triage/unknown permission replies; no fallback to webhook authority. |
| 7 | Repeat same comment | Native receiver E2E returns duplicate without another inbox entry; durable reopen tests. |
| 8 | Different delivery IDs, same comment | Durable alias and cross-source alias tests, including reopen; numeric comment identity is canonical. |
| 9 | Invalid Content-Length | Pure HTTP empty/zero/signed/overflow/truncation bounds; native WinHTTP response-length and truncated-transfer tests. Request and response are distinct boundaries. |
| 10 | Duplicate HTTP headers | Pure case-insensitive duplicate length/signature rejection. |
| 11 | TE/CL ambiguity | Pure `ambiguous_framing_refused`, also rejects content encoding and Expect. |
| 12 | HTTP limits exceeded | Pure header overflow and declared body MAX+1 refusal; native API response MAX/boundary tests. |
| 13 | Slow/interrupted transmission | Native loopback slow header/body and interrupted-body tests, shared deadline. |
| 14 | API timeout | Real WinHTTP request to delayed owned loopback fixture, followed by owned-handle cleanup assertions. |
| 15 | GitHub 401/403/404/429/5xx | Native denied-status/auth-challenge fixture for 401/403/404/429/500/503. Both added status cases passed in the final native run. |
| 16 | Redirect elsewhere | Native redirect trap ensures the second owned loopback endpoint is not contacted. Fixed production host, redirects and automatic auth disabled. No external attack endpoint used. |
| 17 | Unknown GitHub event | Pure rejection before body read; receiver handles only `issue_comment` / created comments. |
| 18 | Edited/deleted comment | Pure action/body/timestamp reconciliation failures, per-endpoint missing-reply propagation; native 404 transport refusal. Actual GitHub edit/delete integration NOT_RUN. |
| 19 | Closed/locked Issue | Pure state/locked/PR and issue identity negatives. |
| 20 | Wrong registered recipient | Direct State `put_verified` wrong thread/provider/GOAL tests; neighbor binding cannot read/transition a message; native E2E neighbor inbox remains empty. This is logical binding isolation, not Codex thread isolation. |

## Native FFI, durable State and local integration review

**CNG:** HMAC uses exact raw bytes and bounded `u32` lengths with SHA-256/HMAC
provider flags. Tests include GitHub's public vector, RFC 4231 vectors, binary
and empty input, tamper and signature format cases. Actual open/hash failures
use Windows calls; impossible-to-reliably-force cleanup failures use a private
injected API seam. Counters distinguish owned providers and successful closes;
they are not a global process-handle leak proof. Failed cleanup refuses digest
success, preserves compound failures, and is not retried blindly.

**JSON:** Raw size bounds precede parsing; a recursive pass rejects duplicate
keys including escaped aliases and ignored extension subtrees. Typed models
separately reject positional arrays and invalid numeric representations. Tests
cover malformed UTF-8, lone surrogates, Unicode/escaping, trailing documents and
recursion bounds. Extension fields in GitHub responses remain allowed, but
duplicate keys do not become trusted by being ignored. Static errors do not echo
untrusted payloads.

**WinHTTP:** Production destination is hard-coded HTTPS GitHub, paths come from
validated configured identities, and token syntax rejects header injection.
Payload URLs are compared as data and never used as destinations. Synchronous
handles are closed child-before-parent on normal/error returns, with a Drop
fallback for unexpected unwinding. Native tests inspect the real session policy,
exercise GET/status/redirect/timeout/size/truncation handling, and account for
only handles created by the owned request. Deadline checks refuse late results;
they do not promise hard scheduler preemption or an exact wall-clock cutoff.
TLS certificate-chain/revocation behavior and an actual GitHub TLS connection
are not qualified by cleartext loopback fixtures.
The separate repository/Issue/comment/permission API reads are not an atomic
GitHub snapshot: remote permissions or content can change after a successful
check. No claim of instantaneous global revocation follows from reconciliation.

**State:** A fresh guarded handle is opened for each transaction and retained
through load, lock, validation, append and sync. Store does not hold a journal
guard for its whole lifetime. Full stored Binding, generation, numeric source,
GOAL and exact target are checked; comment IDs form deterministic repository-
scoped message IDs. Replayed journal operations use Router's scoped MessageKey.
Limits are 1,024 messages, 20,000 records, 512 KiB per record and 32 MiB per
journal. Torn/corrupt/duplicate/out-of-sequence data fails closed without repair
or truncation. Tests cover concurrent claims, durable aliases, reopen, partial
append, fsync failure and a non-retryable interrupted DELIVERING/UNKNOWN path.
Owned native subprocess tests cover competing processes, OS locks, committed
claim after process termination and partial append after process exit.
This is a single-host, single-journal claim boundary, not a distributed lock.
At journal capacity, further outcome records may be refused even for an existing
DELIVERING claim. The durable claim remains non-retryable; guaranteed outcome
progress, reserved completion capacity and reviewed compaction are not provided.
The current receiver never begins chat dispatch.

**ACL/path boundary:** Actual opened owners/DACLs and filesystem metadata are
inspected; unsupported ACL forms, untrusted access, reparse points, hard links,
alternate streams, traversal, network/removable roots and ambiguous paths refuse.
Checks do not silently modify production ACLs or grant privileges. SYSTEM,
Administrators and the configured owner are trusted; supported OS ancestors may
also trust the Windows-resolved TrustedInstaller principal. Previously granted
directory/administrative rights are not revoked by changing a DACL. Fresh private
provisioning is an explicit precondition, not an isolation sandbox against the
same user, administrators or a compromised OS. The final suite passed native private/unprotected ACL, retained writer/mapping, real junction, hardlink and guarded rename controls.

**Local receiver E2E:** The test calls production `receive_one` through an owned
loopback HTTP listener, real CNG and the real WinHTTP request core using a private
`cfg(test)` loopback client without credentials, then commits to a real private
journal and reads it back. Assertions cover acceptance, duplicate, unauthorized
actor, bad MAC, exact target, empty neighbor inbox, journal reopen and duplicate
after reopen. The fixture client is compiled out of production. The final run passed this controlled receiver-to-inbox path;
it does not prove whole-service startup, secrets provisioning, external GitHub,
public reverse proxy, provider acceptance or worker execution.

Independent source review found no additional P0/P1 in current GitHub/HTTP/
WinHTTP reconciliation paths. The direct State wrong-target test requested by
that review is now present. The qualified final suites passed retained-access/fsync and native integration controls. Security review remains **ISSUES_FOUND / addressed within scope; no known unfixed P0/P1 found by the final source reviews**, not SECURITY PASS or deployment acceptance.

## Final qualifying gates

Final verified source: `0963e4269e870d419e1f871b62a670625704b78c`. [Full summary](evidence/receiver-final-4/summary.json).
All build inputs matched HEAD before/after; raw fixture fingerprints matched the committed fixture; source, environment and external-input drift lists were empty. All commands used cached dependencies with `CARGO_NET_OFFLINE=true`. The test-only parent selected a newly owned private child beneath the existing profile. No production policy override was active.

| Check | Result | Command / evidence | Exit | Tests | Log SHA-256 |
|---|---|---|---:|---|---|
| Cargo metadata | PASS | `cargo metadata --format-version 1`; [log](evidence/receiver-final-4/metadata.txt) | 0 | N/A | `61900fdf7fd8fa62e33353fc02a3962554b216d9610da95684fb726cd7561266` |
| Cargo fmt | PASS | `cargo fmt --all -- --check`; [log](evidence/receiver-final-4/fmt.txt) | 0 | N/A | `138bee985428435bd69d0e378a4a318a9ab578a243b10f17746e739602c5aa0d` |
| Cargo check | PASS | `cargo check --workspace --all-targets --locked`; [log](evidence/receiver-final-4/check.txt) | 0 | N/A | `5706a4f34d56fb97fc1527701399930e94d3a908733957f784072e7f16ea7b5c` |
| Cargo test | PASS | `cargo test --workspace --all-targets --locked`; [log](evidence/receiver-final-4/test.txt) | 0 | 204 PASS / 0 FAIL / 0 ignored | `f5a9f5f97f75dcabe4e4ad636cd6a9d7c3f8128769b29d443d099a6c28605543` |
| Cargo clippy | PASS | `cargo clippy --workspace --all-targets --locked -- -D warnings`; [log](evidence/receiver-final-4/clippy.txt) | 0 | N/A | `dea6686dbefc84de74c8a2550adbd0480377c3b4339515070879e5e25d719b7f` |
| Debug build | PASS | `cargo build --workspace --locked`; [log](evidence/receiver-final-4/debug.txt) | 0 | N/A | `9ef75fa27f4ec998931505e3e899a575baa478bc7d53df62665e8bc9af1d8c1b` |
| Release build | PASS | `cargo build --workspace --release --locked`; [log](evidence/receiver-final-4/release.txt) | 0 | N/A | `46e2bf510c5cf39f2b56bfd5df1b999613b460739914c57939b111936ad2d40d` |
| Locked offline tests | PASS | `cargo test --workspace --all-targets --locked --offline`; [log](evidence/receiver-final-4/offline-test.txt) | 0 | 204 PASS / 0 FAIL / 0 ignored | `2803c60d3f468e426dd4176b45619a129e384f7f6d7f9bd11c55d774f2bbecea` |
| Diff check | PASS | `git diff --check`; [log](evidence/receiver-final-4/diff-check.txt) | 0 | N/A | `06295aaf402ff039ca4149210174f93718015f2ac5d30749d2cc9bd9dac5c5a1` |
| Security review | ISSUES_FOUND / fixes reviewed | Independent source/FFI review and native controls below; no known open P0/P1 found | N/A | Not inferred from unit tests alone | See source review and evidence manifest |

Both final-3 and final-4 passed all nine gates, with 204/0/0 normal and 204/0/0 offline each. These are repeated executions of the same scenarios, not 816 distinct tests. The final-4 breakdown is Protocol 31, Router 36, Security 24, State 35 and GitHub 78 (55 library + 23 binary), total **204**. All 103 baseline Windows scenarios remain; 101 tests were added.

A separately captured [native receipt](evidence/receiver-native-receipt/summary.json) at this same SHA ran `cargo test -p cwg-github --bin cwg-github --locked --offline native_receiver_end_to_end -- --nocapture`, exit 0, 1 PASS / 0 FAIL / 0 ignored. Actual stdout: `NATIVE_E2E: loopback HTTP + BCrypt + WinHTTP + protected fsync journal; accepted=1 duplicate=2 refused=2 neighbor=0`. This includes reopen/readback, not a real Codex chat connection.

Ready for subsequent independent review of Draft PR #2. **Merge/deployment readiness is not granted.** Actual GitHub/TLS/proxy/Codex qualification and power-loss guarantees remain open. The historical native process failure described below is preserved and its exact fast-fail subcode/stack remains NOT_VERIFIED. No known unfixed P0/P1 was identified in the reviewed source scope.

## Dependency, publication and independent source review

`Cargo.lock` retains all external versions/checksums. The only lockfile change against the initial PR #2 source adds the local State → Security dependency for actual ACL enforcement. Locked offline metadata reports no dependency cycles; all five workspace crates use Edition 2024. No new registry dependency, toolchain or third-party software was installed. Protocol/Router source and tests are byte-identical to PR #1.

The [final automated source observations](evidence/review-5/source-review.json) remain ISSUES_FOUND rather than a false security PASS: five intentional unsafe-source sites (system CNG/WinHTTP/ACL wrappers, fixed-work MAC comparison and test-only native fixtures), plus two drive-path candidates. The latter are the disabled synthetic `CWG-Private` config and documented usage example, not personal host paths. No credential-pattern candidate was found. Independent FFI review checked ABI types, buffer/SID/ACE bounds, ownership and error cleanup. Final reviews found no additional P0/P1; accepted residual P3 limits are RAII token/LocalFree cleanup status not propagated, trusted Rust callers of `GuardedFile.file_mut`, and no formal hardware timing proof of MAC comparison.

A final [publication inventory](evidence/receiver-final-4/publication-manifest.json) lists changed files and SHA-256 values, excluding itself. Test-only executables/fixtures are not service or caller-authentication implementations.

## Evidence integrity and provenance limits

Each summary contains source SHA, before/after input inventory and hashes,
commands, exit codes, observed test counts, bounded redacted transcript hashes
and saved-log hashes. A JSON-envelope log has a different file hash from its
decoded transcript hash; use the appropriate field. The report was finalized from actual controller-run command outputs and independent source reviews. Run evidence is not fabricated from source assertions.

| Evidence summary | SHA-256 of saved summary |
|---|---|
| [receiver-baseline](evidence/receiver-baseline/summary.json) | `bf710ff3fc195a5ec07c19235a4b8e4e51b6bcfc5682eea3a6753ca2ebd9002d` |
| [receiver-diagnostic-winhttp-1](evidence/receiver-diagnostic-winhttp-1/summary.json) | `9602c4c4933d97a113c27abdfa3ab1465f1d2816b403f084cdcc5b961141259b` |
| [receiver-diagnostic-winhttp-2](evidence/receiver-diagnostic-winhttp-2/summary.json) | `19c9b52c4449451425f06da43644ef209587f63a3d88a8896bc22ec0ab045762` |
| [receiver-diagnostic-winhttp-3](evidence/receiver-diagnostic-winhttp-3/summary.json) | `1d6e470776bd370d81c58cab4bac4b0cd4e052f5ee49763bb3b4f0148637becc` |
| [receiver-final](evidence/receiver-final/summary.json) | `c7dd9d150a32c5acbd6b5c1c73eae3e8390f901b351b8ec9227d521868158daa` |
| [receiver-final-2](evidence/receiver-final-2/summary.json) | `8bdaefc3ec62e605c541d6c0816c5ed43d27d96749533b3b07e7927d712d82d4` |
| [receiver-final-3](evidence/receiver-final-3/summary.json) | `e0a373ebe13c24cc7dd3c1d9f8aa4597a596ebe6bf027feefbe7c887e1bb463f` |
| [receiver-final-4](evidence/receiver-final-4/summary.json) | `d1a9b5e0afc87aecec4069760c1c330d75201a37a5e7b35d22484b182def6414` |
| [receiver-native-receipt](evidence/receiver-native-receipt/summary.json) | `31007c31d13bbaa5f57cb9929e75aba1b71420316a5da62f41939d685bbddff6` |
| [receiver-regression-commit](evidence/receiver-regression-commit/summary.json) | `2bcce7cc3c2cb6e1e1e002db97cfa0acbac970d07503a8eeaf29fd46c22f1366` |
| [receiver-regression-delayed](evidence/receiver-regression-delayed/summary.json) | `68f89d07cff0a38f61d0ca565b143af5cd56dd3053ba6b3a3d31a19ba59462e1` |
| [receiver-regression-fixture](evidence/receiver-regression-fixture/summary.json) | `a610beed1ca2447d68f7245cf25c2bbc508f7d29bd5c4692919f6341aec526b0` |
| [receiver-regression-github](evidence/receiver-regression-github/summary.json) | `c41fc83698424dac82df81a59bd1ef991e6910f54683a58c66d32c3dbf9a179f` |
| [receiver-regression-http](evidence/receiver-regression-http/summary.json) | `86438d1441ffa1d4a6140dcfc83746effebf5277f059f351b0c9cc8b3b2eb1c4` |
| [receiver-regression-pool](evidence/receiver-regression-pool/summary.json) | `94cb6fc9d0a0e72f5ecee2e470774f0104aa8794aba5d8fbbdc08fe37cb9e2e6` |
| [receiver-regression-retained](evidence/receiver-regression-retained/summary.json) | `7292d83c3652a3132e8701bfe1f1067b0d2713fbb9461cf5f6351622b67b45f0` |
| [receiver-regression-retained-2](evidence/receiver-regression-retained-2/summary.json) | `ef71fa2f3c186cc94399dd438f6c772fb54e3cb46783cd4fce0b6f03898f6c21` |
| [receiver-regression-retained-3](evidence/receiver-regression-retained-3/summary.json) | `e431a62bcf33d795db2ca3dbe891ce045cd0fe0b3f39b23b714919263fe4e9d7` |
| [receiver-regression-security](evidence/receiver-regression-security/summary.json) | `83e881e54c18d40e722be673c253f4b624ecf174a5579c61d3767c9ce89d04c3` |
| [receiver-regression-server](evidence/receiver-regression-server/summary.json) | `5ffa3c3e0e64fbe88dc0219ef3135b798e6bbb0702a707f3cdd4bcb57c9a0368` |
| [receiver-regression-server-2](evidence/receiver-regression-server-2/summary.json) | `c7dc1ee0e2f28c3946e2917fb8e7dae66ccd47a22b7cca71c0d6979fff5b21f9` |
| [receiver-regression-state](evidence/receiver-regression-state/summary.json) | `452d02f84751e2018360f399061c3baea6ad33983b5eb54a2800bc99a8345a04` |
| [receiver-regression-winhttp](evidence/receiver-regression-winhttp/summary.json) | `86a8d5e8e53a19e270d8cfe7bae3ebad7066a1377cc3b6f2b292da6773c042b2` |
| [receiver-regression-workspace](evidence/receiver-regression-workspace/summary.json) | `93a4908cd841557a088f0fd3509df2a87af21c8ca15578b04983e1c2cf7deb49` |
| [receiver-regression-workspace-2](evidence/receiver-regression-workspace-2/summary.json) | `cebb2b35f53b17af9a4732ec490e3105cd4a8d263b6a31c415054fa66038de0b` |
| [receiver-regression-workspace-3](evidence/receiver-regression-workspace-3/summary.json) | `9d00cefe6a0defd88b638c387e120774eb6fc286fa7194f129161a5ebf10f466` |
| [receiver-regression-workspace-4](evidence/receiver-regression-workspace-4/summary.json) | `b8d4f7dea7ba444a6dd5cde60b8499c0a27eb5a25630cda75eb3c3fe9b7436b4` |
| [receiver-regression-workspace-5](evidence/receiver-regression-workspace-5/summary.json) | `f114bfa7c04d01767212e2e659542eb086d0d33715a5dcb5a2b6261d152979d2` |
| [receiver-regression-workspace-6](evidence/receiver-regression-workspace-6/summary.json) | `d26f9671568f5241ef8efbf9402f593be2d506a66456f672dafc2628de0fc37e` |
| [receiver-regression-workspace-7](evidence/receiver-regression-workspace-7/summary.json) | `d6e053e49c87f14ba60c3337baf0969dcb720ebd4d9618be631f3e2f97ebefdf` |
| [receiver-regression-workspace-8](evidence/receiver-regression-workspace-8/summary.json) | `aabc65dfca6ef35c25d46a94e3f9f6c8a2124eae44c76bb7b0ad63126a4e8d6c` |
| [receiver-regression-workspace-9](evidence/receiver-regression-workspace-9/summary.json) | `e71557d69953919180367ec53c06656f90df9d0ae498c4be307d9a5bfbf78c8b` |

Saved reconstruction-envelope SHA-256:
`6b152b4da4516955303a23021471489238301dd0da5ba82a521e3760b1b66e7f`.

Before/after fingerprints detect net drift, not changes restored between
snapshots (ABA). Scope includes Rust/manifests/lock, Cargo/toolchain configuration,
wrappers/build scripts and audit executables; raw fixture hashes are additionally
recorded by the current runner. This is not complete provenance for every
generated/include input, mutable registry/tool binary, external wrapper or host
configuration. Whitelisted compiler/profile environment values are hashed rather
than dumped. These limits remain applicable even after successful final gates.

## Explicit NOT_RUN / NOT_VERIFIED

- **NOT_RUN:** real GitHub webhook subscription or delivery, real API account/
  permission changes, external redirect target, GitHub TLS handshake/certificate
  chain/revocation qualification and public reverse proxy.
- **NOT_RUN:** full installed-service startup, production credentials or secret
  provisioning, deployment/firewall changes, background daemon or model calls.
- **NOT_IMPLEMENTED / NOT_RUN:** MCP, Codex App Server Adapter, actual account/
  thread ownership and two-real-chat isolation, provider receipts, worker
  execution, outbound replies or publishing authorization.
- **NOT_VERIFIED:** physical power-loss durability or storage-device guarantees.
  Process-kill/restart tests and injected fsync failures do not establish this.
- **NOT_VERIFIED:** isolation from a hostile same-user process, administrators,
  SYSTEM, compromised kernel/filesystem or historically granted directory rights.
- **NOT_VERIFIED:** general production readiness. A durable inbox record is not
  evidence that Codex received, acknowledged or executed a task.

## Changed files in this audit

- `.gitattributes`
- `CWG_GitHub/README.md`
- `CWG_GitHub/src/http.rs`
- `CWG_GitHub/src/lib.rs`
- `CWG_GitHub/src/main.rs`
- `CWG_GitHub/src/native_tests.rs`
- `CWG_GitHub/src/tests.rs`
- `CWG_GitHub/src/windows.rs`
- `CWG_Security/src/lib.rs`
- `CWG_Security/src/windows_acl.rs`
- `CWG_Security/src/windows_cng.rs`
- `CWG_Security/tests/json_limits.rs`
- `CWG_State/Cargo.toml`
- `CWG_State/src/bin/cwg-state-fixture.rs`
- `CWG_State/src/lib.rs`
- `CWG_State/tests/durable.rs`
- `CWG_State/tests/native_process.rs`
- `CWG_State/tests/retained_access.rs`
- `CWG_State/tests/support/windows_fixture.rs`
- `CWG_State/tests/windows_paths.rs`
- `Cargo.lock`
- `README.md`
- `SECURITY.md`
- `audit/RECEIVER_WINDOWS_VERIFICATION.md`
- `audit/receiver-verify.mjs`
- `audit/retained-fixture-repro.json`
- `doc/receiver.md`

All per-attempt logs and summaries under `audit/evidence/receiver-*` and the source-review receipts are listed individually in the publication inventory. Subsequent report/evidence-only commits do not change the checked Rust, manifests, lockfile, configuration or executable audit inputs. The final verified source SHA above deliberately identifies the tested implementation rather than pretending an evidence-only commit was compiled.

## Primary review references

- [Microsoft accept: inherited socket properties](https://learn.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-accept)
- [Microsoft SO_RCVTIMEO: blocking receives](https://learn.microsoft.com/en-us/windows/win32/winsock/sol-socket-socket-options)
- [Rust 1.97.1 Windows socket implementation](https://raw.githubusercontent.com/rust-lang/rust/1.97.1/library/std/src/sys/net/connection/socket/windows.rs)
- [Microsoft WinHTTP options and option support matrix](https://learn.microsoft.com/en-us/windows/win32/winhttp/option-flags)
- [Microsoft WinHTTP concurrency](https://learn.microsoft.com/en-us/windows/win32/winhttp/concurrency-in-winhttp)
- [Microsoft CreateFile sharing contract](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew)
- [Microsoft LockFileEx and mapped-view limitations](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-lockfileex)
- [Microsoft fast-fail classification](https://learn.microsoft.com/en-us/cpp/intrinsics/fastfail?view=msvc-170)
