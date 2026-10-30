# CWG foundation build, test and source reverification

Status: **PASS for the recorded source/build/model scope; Draft for independent review**.

This is a fresh verification of the Protocol/Router foundation in Draft PR #1,
not acceptance of a webhook, MCP server or Codex Adapter. Results below separate
executed commands from source inspection and checks awaiting execution. Earlier
audit reports and failed attempts remain historical evidence at their recorded
revisions; no earlier PASS is substituted for this run.

## Source identity and scope

| Source | Exact commit |
|---|---|
| Requested baseline and freshly fetched main | `a409120bbafeaefd7ad7569b7d5704e2ad6122d5` |
| Existing PR #1 head freshly built before new changes | `a7aa73858a85620f4ca10f09fcf75d86186f662c` |
| New tests and status pre-fix regression | `1d90e701209ba0be7e46d1c8445016851e946f9e` |
| Final checked implementation | `3ddf7088b7780d1e97732a534c873887aa361f35` |

Main still matched the requested baseline, so there was no newer main delta to
integrate. Work uses the existing `codex/cwg-build-test-audit` branch and Draft
PR #1, based on main. Main is not modified directly. A separate worktree keeps
this foundation audit separate from other CWG branches. Only `CWG_Protocol` and
`CWG_Router` are Cargo workspace members in this branch.

Before changes, the review read README, AGENTS, SECURITY, Cargo.toml,
doc/maps.md, doc/protocol.md, doc/migration.md, doc/roadmap.md, both crate library
sources, their tests and the existing audit tools/reports. MSS and official
Makepad repositories were not changed. No Receiver, MCP Server or Codex Adapter
implementation is introduced here.

## Recorded environment and verification method

- Windows NT `10.0.26300.9711`, x64; Rust host `x86_64-pc-windows-msvc`.
- `rustc 1.97.1 (8bab26f4f 2026-07-14)`, full compiler commit
  `8bab26f4f68e0e26f0bb7960be334d5b520ea452`, LLVM `22.1.6`.
- `cargo 1.97.1 (c980f4866 2026-06-30)`.
- Existing Rust, Cargo and local dependency cache were used. No third-party
  software or toolchain was installed. No Docker, WSL, WSLC or VM was used.

CHEAPEST_VERIFICATION_PATH: a Protocol correction first uses its named
regression; the final milestone executes all seven owner-requested workspace
commands sequentially. Router additions exercise existing behavior rather than
introducing another routing implementation. The controller supplies the
process-local `CARGO_NET_OFFLINE=true` for cache-only resolution; exact Cargo
arguments remain those requested by the owner. The existing runner passes this
environment through but does not list that flag separately in its summary.

Fresh evidence attempt directories are reserved exclusively. Final runs require
committed scoped build/audit inputs. Before/after fingerprints include Rust
sources, manifests, lockfile, relevant Cargo/toolchain configuration, scripts
and selected compiler/profile environment controls. Observed drift makes the
attempt FAIL even if subprocesses exit zero. This is not a complete hermetic
host attestation or detection of transient changes restored before the final
snapshot. Logs redact workspace/home paths and hash the published bytes.

## Fresh baseline commands — actually executed

Source: `a7aa73858a85620f4ca10f09fcf75d86186f662c`.
[Full fresh summary](evidence/final-4/summary.json), overall PASS, exit 0.
The 67 passing tests are a fresh execution of the existing foundation suite,
not an inference from the old 67-test report.

| Check | Result | Command | Exit | Observed tests | Evidence |
|---|---|---|---:|---|---|
| Cargo metadata | PASS | `cargo metadata --format-version 1` | 0 | N/A | [log](evidence/final-4/metadata.txt) |
| Cargo fmt | PASS | `cargo fmt --all -- --check` | 0 | N/A | [log](evidence/final-4/fmt.txt) |
| Cargo check | PASS | `cargo check --workspace --all-targets` | 0 | N/A | [log](evidence/final-4/check.txt) |
| Cargo test | PASS | `cargo test --workspace --all-targets` | 0 | 67 PASS / 0 FAIL / 0 ignored | [log](evidence/final-4/test.txt) |
| Cargo clippy | PASS | `cargo clippy --workspace --all-targets -- -D warnings` | 0 | N/A | [log](evidence/final-4/clippy.txt) |
| Debug build | PASS | `cargo build --workspace` | 0 | N/A | [log](evidence/final-4/debug.txt) |
| Release build | PASS | `cargo build --workspace --release` | 0 | N/A | [log](evidence/final-4/release.txt) |

## New finding, regression and correction

**P3 — DeliveryStatus had an alternative JSON wire shape.** The derived Serde
unit-enum decoder accepted externally tagged objects such as `{"UNKNOWN":null}`
in addition to the documented uppercase status strings. This is a wire-format
ambiguity, **not an authentication bypass** and not evidence of an unauthorized
Router transition. Router methods operate on its existing guarded state model.

The new `delivery_status_accepts_only_exact_string_wire_values` test checks all
eight canonical uppercase string serializations and round trips, then tries
each null-tagged object through both raw JSON and `serde_json::Value` decoding.
At the exact committed pre-fix source above, all 16 object decoding cases were
accepted. The test actually ran and failed:

| Command | Source | Exit | Observed tests | Evidence |
|---|---|---:|---|---|
| `cargo test -p cwg-protocol --test status_wire` | `1d90e701209ba0be7e46d1c8445016851e946f9e` | 101 | 0 PASS / 1 FAIL / 0 ignored | [summary](evidence/status-before/summary.json), [transcript](evidence/status-before/test.txt) |

The correction requests string deserialization explicitly and maps only the
eight exact uppercase values. Canonical JSON serialization remains unchanged;
the alternative object representation is deliberately refused. Protocol and
migration documentation explain this tightening. The same named post-fix command actually passed on `3ddf7088b7780d1e97732a534c873887aa361f35` (exit 0, 1 PASS / 0 FAIL / 0 ignored): [summary](evidence/status-after/summary.json), [transcript](evidence/status-after/test.txt).

Two additional Router tests compose the previously separate route/replay
boundaries within one repository:

- `comment_replay_across_goals_and_threads_does_not_reserve_the_new_message_id`;
- `message_id_replay_across_goals_and_threads_does_not_reserve_the_new_comment`.

They register two valid GOAL/thread mappings, attempt replay across them, check
that the original state is unchanged, and reuse the unused ID/comment after the
rejected admission. These are additional composition checks of existing
behavior; no Router defect or runtime correction is claimed from their addition.
The final execution passed all **70 scenarios (67 retained + 3 new)**, with zero failed or ignored tests. The locked/offline run repeated the same 70 scenarios; it is not another 70 distinct tests.

## Required negative coverage

The following is a mapping of test assertions to requested categories. It does
not turn source inspection into execution evidence. Use the baseline/final
command results for the exact revision actually run.

| Protocol category | Existing named test / additional coverage |
|---|---|
| Wrong schema version | `rejects_wrong_schema_during_deserialization` |
| Empty message ID | `rejects_empty_message_identifier` |
| Invalid repository | `rejects_invalid_repository_names` |
| Empty thread ID | `rejects_empty_thread_id` |
| Invalid GOAL ID | `rejects_malformed_goal_identifiers` |
| Oversized body | `rejects_oversized_body_in_bytes`; ASCII, multibyte and escaped-body boundaries |
| Unknown JSON fields | `rejects_unknown_json_fields_at_every_level` |
| Duplicate JSON keys | `rejects_all_duplicate_json_keys_even_identical_values`; escaped field aliases |
| Invalid Unicode | `rejects_invalid_unicode_json_and_non_ascii_identity`; malformed UTF-8 and unpaired surrogates |
| Invalid numeric IDs | `rejects_invalid_numeric_identifiers`; zero, negative, fraction, exponent, overflow, string, null and Boolean |

The validation helper checks both direct `validate` and typed raw JSON
deserialization. Additional tests cover standalone Source/Target, object-only
Message/Source/Target, exact statuses and InvalidMessage classifications.
`Message::from_json_slice` has exact/+1 raw-budget tests, decoded string limits
and error/Debug body-redaction checks. Deserializing a generic map first loses
duplicate keys; the protocol does not claim to recover information already lost
by a caller.

| Router category | Existing named test / additional coverage |
|---|---|
| Two independent repositories | `independent_repositories_do_not_share_routes_or_comment_dedupe` |
| Two logical Codex chats | `two_logical_codex_chats_in_one_repository_remain_distinct` |
| Simultaneous GOALs | `simultaneous_goals_can_be_queued_with_external_serialization` |
| Repeated message ID | `duplicate_message_id_cannot_replace_original`; new cross-GOAL composition case |
| Repeated comment ID | `duplicate_comment_id_cannot_create_new_message`; new cross-GOAL composition case |
| Wrong thread | `wrong_thread_and_provider_are_refused_without_poisoning_dedupe` |
| Unknown binding | `unknown_repository_or_goal_binding_is_refused` |
| Missing-message delivery | `delivery_of_missing_message_is_refused_by_every_operation` |
| Retry after UNKNOWN | `unknown_outcome_remains_non_retryable_and_replay_protected` |
| Invalid state transitions | `all_reachable_states_enforce_exact_transition_matrix`; seven reachable states × six operations |

Scope/limit/suspension tests additionally check every state query/transition
against forged repository/GOAL/provider/thread, malformed keys, zero/exact/overflow
budgets, failure atomicity and one-way suspension without evicting replay history.
These are logical model tests. Two logical thread IDs and an external Mutex do
**not** establish Codex thread isolation, distributed claims or actual delivery.

The original main unit tests `rejects_empty_body`,
`exact_binding_and_duplicate_refusal` and `uncertain_delivery_does_not_retry`
retain their scenarios and assertions; Router arguments were migrated earlier
to MessageKey. No tests are removed or ignored to obtain a pass. All 67 tests
present at the start of this revalidation remain in the source.

## Architecture and source security review

| Boundary | Implemented guarantee | Remaining requirement |
|---|---|---|
| Input validation | Shared validation, strict objects, exact syntax and bounded raw JSON entry point | Generic parsers/callers must impose raw bounds; a future ingress must bound its stream before allocation |
| IDs and bindings | Validated immutable repository/GOAL/target mapping; exact scoped MessageKey | Caller authentication, owner approval and numeric GitHub/account/generation ownership are external requirements |
| Cross-repository/thread routing | Canonical repository keys, exact GOAL/provider/thread comparisons | Correlation is not per-caller authorization or real Codex ownership/isolation |
| Replay | Repository-scoped message/comment dedupe; rejected admissions do not reserve unused IDs | In-memory history disappears on restart; no webhook delivery-ID ledger or durable recovery exists here |
| Races | Exclusive mutable Router access; claim/state guards and external-Mutex model tests | No multiprocess/distributed lock or persistent atomic claim |
| State and UNKNOWN | Guarded post-VERIFIED transitions; UNKNOWN can only be closed to BLOCKED | Methods record trusted-controller observations, not verified provider/worker receipts or completed work |
| Untrusted GitHub text | Body remains data; no shell, model or network execution | Future authenticated ingress/provider must preserve the trust boundary |
| Registration authority | Invalid and repeated registration refused; body cannot change policy | Ordinary Rust APIs cannot identify the caller; no fake approval flag is added |

README, SECURITY and doc/maps.md distinguish the implemented post-VERIFIED
model from design-only PENDING/signature/API/authentication edges. The migration
plan leaves authenticated binding and durability incomplete. Roadmap milestones
are not declared accepted by a diagram or by model-test success. No additional
P0/P1/P2 source defect was identified by this architecture/dependency review;
the new status shape finding is classified P3 and corrected. A separate reviewer checked the new Protocol implementation without authoring it, and independently matched the seven final and five supplemental log hashes. No additional P0/P1/P2 was identified in the reviewed JSON/source scope.

## Workspace, dependencies and publication

The workspace has two crates, both Edition 2024. Its local edge is Router →
Protocol; Protocol uses serde and serde_json. The existing runtime serde_json
dependency is justified by checking a raw-byte bound before JSON parsing.
Cargo.lock pins registry versions/checksums. No new dependency or version is
needed for this revalidation. Actual locked/offline metadata reported zero cycles and no source/publication-pattern findings: [review result](evidence/foundation-review-4/source-review.json). Cargo.lock and both crate manifests are unchanged against the starting PR head.

Both owned libraries enforce `#![forbid(unsafe_code)]`; source inspection found
no unsafe construct in the owned Rust files. This is not a certification that
every transitive dependency contains no unsafe. No unused direct dependency was
identified from the source; an external unused-dependency scanner was not
installed. A targeted credential/private-path scan is not proof against every
possible secret. Do not publish unrelated worktrees, temporary fixtures, token
values or personal host paths with the report.

Locked/cache-only builds show repeatable dependency selection on this host,
not bit-for-bit binaries, an independently provisioned machine, a supported MSRV
or other-platform compatibility. The exact compiler version is recorded rather
than claiming a toolchain pin which the source does not provide.

## Final corrected-source verification

Final implementation SHA: `3ddf7088b7780d1e97732a534c873887aa361f35`. [Seven-gate summary](evidence/final-5/summary.json): overall PASS, exit 0, scoped inputs matched committed HEAD and before/after fingerprints had no recorded drift.

| Проверка | Результат | Доказательство |
|---|---|---|
| Cargo metadata | PASS | `cargo metadata --format-version 1`, exit 0; [log](evidence/final-5/metadata.txt); SHA-256 `95145d6d940325da76e172e89cdbb7b83374dc7aa47167ad3d48bb94482c6036` |
| Cargo fmt | PASS | `cargo fmt --all -- --check`, exit 0; [log](evidence/final-5/fmt.txt); SHA-256 `5cdce237defd66423dbb2fbadd1e34cd93f1c84babff4222ba476cc7a5fcc0fe` |
| Cargo check | PASS | `cargo check --workspace --all-targets`, exit 0; [log](evidence/final-5/check.txt); SHA-256 `4cdf89675764ee226d959967376d9f3c45cc341cbdf520f68cea57e87993ad6a` |
| Cargo test | PASS | `cargo test --workspace --all-targets`, exit 0; **70 PASS / 0 FAIL / 0 ignored**; [log](evidence/final-5/test.txt); SHA-256 `6470d52d61fced6aae26d18f4377f5f429fd2d39b167f950591c3af03a6e333d` |
| Cargo clippy | PASS | `cargo clippy --workspace --all-targets -- -D warnings`, exit 0; [log](evidence/final-5/clippy.txt); SHA-256 `fa8aa1fea7a23f4f68b7313109c66de5fbe39e6c3781a2c599272faffb2db678` |
| Debug build | PASS | `cargo build --workspace`, exit 0; [log](evidence/final-5/debug.txt); SHA-256 `aa9c633bd76cbb8042950b2490263a503e08f3b8cce47981f8f92f7235de9c41` |
| Release build | PASS | `cargo build --workspace --release`, exit 0; [log](evidence/final-5/release.txt); SHA-256 `5a7ed9128d62e047914e55cc38e6abb12d81bedb3671ba5b9381a681397f04c6` |
| Security review | **ISSUES_FOUND — P3 wire ambiguity corrected** | Source review, actual pre-/post-fix regression and independent review; no known additional P0/P1 in this scope. No blanket SECURITY PASS. |
| Locked/offline confirmation | PASS | `cargo test --workspace --all-targets --locked --offline`, exit 0; 70 PASS / 0 FAIL / 0 ignored; [log](evidence/foundation-extra/locked-offline.txt); SHA-256 `66dfc81aa6ae63cce44e41cc3e63617b080ea93c238d6a2fcfce68480832daa6` |
| Audit tooling tests | **14 PASS / 1 NOT_RUN** | `node --test audit/tooling.test.mjs`, exit 0; 14 PASS / 0 FAIL / 1 NOT_RUN; [log](evidence/foundation-extra/node-tests.txt); SHA-256 `d67d12f7d77d06a10ee0ad38e1be3f75e134aa3a183e00680e79cde457eb854f` |
| Diff check | PASS | `git diff --check`, exit 0; [log](evidence/foundation-extra/diff-check.txt); SHA-256 `06295aaf402ff039ca4149210174f93718015f2ac5d30749d2cc9bd9dac5c5a1` |
| Diff check | PASS | `git diff --check a409120bbafeaefd7ad7569b7d5704e2ad6122d5 HEAD`, exit 0; [log](evidence/foundation-extra/base-diff-check.txt); SHA-256 `4724369809518e1c00ae18c76de675db8995f07f7a5b6ecea45cf43f4b7fc569` |
| Native Windows version | PASS | `cmd /c ver`, exit 0; [log](evidence/foundation-extra/platform.txt); SHA-256 `556bf6790a01b33be814616aa28d6e0f372dea404eb4dd1e852b96c92270abed` |
| Dependency/source/publication observations | PASS within scan scope | `audit/review.mjs::runReview({phase: "review-4"})` invoked through Node, exit 0; locked offline metadata, zero cycles and zero pattern findings. [Result](evidence/foundation-review-4/source-review.json) |
| Test retention | PASS | All 67 starting test names occur in the 70-test successful run; all 7 previous integration test files and both unit-test modules are byte-identical to the starting PR head. [Receipt](evidence/foundation-extra/retention.json) |

Rust and Cargo versions above were re-read by the final runner; Node is `v24.22.1`. The Node suite has 15 scenarios: 14 actually ran and passed, while the file-symlink fixture is **NOT_RUN** because Windows returned `EPERM`. The directory-junction test ran and passed. No privilege/developer-mode/security setting was changed. Node tests use explicitly labelled simulated Cargo callbacks to test the evidence runner; they are not Rust execution evidence.

The initial main unit scenarios remain intact. Protocol now has 32 Rust tests and Router 38. No test was removed, weakened or ignored to obtain these Rust PASS results. The additional repeat of all 70 tests used `--locked --offline`; no dependency download or new software installation was needed.

## Evidence hashes and historical boundaries

| Evidence | Saved file SHA-256 |
|---|---|
| [evidence/final-4/summary.json](evidence/final-4/summary.json) | `758165698b0f25d69a28bdf84b92c6a3e6c9ec8c275e9ac3fe3f04267328ff57` |
| [evidence/status-before/summary.json](evidence/status-before/summary.json) | `7cb633e8c5c35c05dc4d0ec1ea6201f0ae29510c689b65c0fbb7f9f76b0a06a1` |
| [evidence/status-after/summary.json](evidence/status-after/summary.json) | `de361e6036d34af93a7f7b9d1a1f46abf6a505ff3d9864d40ac533cd72e76bcd` |
| [evidence/final-5/summary.json](evidence/final-5/summary.json) | `d4a8e8cf5dbb0f1c9fa967a2c85b347b9c065fc0534ea368ddac18dda96e5754` |
| [evidence/foundation-extra/summary.json](evidence/foundation-extra/summary.json) | `c62794903042aa380b355f7d3ea3cc3685b8588a5f5bd37df45fa9fdf2ee1565` |
| [evidence/foundation-extra/retention.json](evidence/foundation-extra/retention.json) | `eb47bbde0632c2a4f7dc7dd5cffc09c892b98d7ac3dbfb3f41dd1baafb960985` |
| [evidence/foundation-review-4/source-review.json](evidence/foundation-review-4/source-review.json) | `7adf5fcb5ddbc293da66c9aa04d51facc7fdadc4e98479b9a1896171c2612176` |

Each summary records exact source identity, actual commands/exits/counts and log
hashes. JSON-envelope transcripts preserve diagnostic whitespace; their file hash
differs from the hash of the decoded transcript. Historical reports/logs are not
rewritten. In particular, the previous security report explicitly states that
some earlier follow-up pre-fix transcripts were not retained; this report does
not invent or reattribute those missing artifacts.

## Remaining NOT_RUN and limitations

- Webhook/HMAC/GitHub actor reconciliation, MCP, Codex Adapter, actual delivery,
  worker receipts/completion and outbound authorization: **NOT_IMPLEMENTED /
  NOT_RUN** in this foundation branch.
- Real caller identity, owner-approved registration, numeric repository/account/
  generation binding and stale-chat detection: external requirements, not
  authenticated by syntactically valid IDs or a complete MessageKey.
- Persistent restart recovery, distributed claims and true Codex thread
  isolation: **NOT_RUN**, with no implementation in this workspace.
- Other Serde formats, clean-machine, bit-for-bit binary reproducibility, MSRV and other platforms:
  **NOT_RUN / not established**.
- Complete transitive unsafe review and package advisory database audit:
  **NOT_RUN**; no additional scanning software is installed.
- Production/runtime readiness: **NOT_VERIFIED**. Source/model/build results
  do not prove that an existing Codex chat received or executed any message.

The Draft PR remains for independent review. Main is unchanged, no automatic
merge is enabled, and this audit does not authorize release or real delivery.

## Changed files in this revalidation

The implementation/test/tool changes and documentation updates against the starting PR head are:

- `CWG_Protocol/src/lib.rs`
- `CWG_Protocol/tests/status_wire.rs`
- `CWG_Router/tests/cross_goal_replay.rs`
- `audit/verify.mjs`
- `doc/protocol.md`
- `doc/migration.md`
- `README.md`
- `doc/testing.md`
- `audit/FOUNDATION_REVERIFICATION.md`
- Fresh evidence under `audit/evidence/final-4/`, `status-before/`, `status-after/`, `final-5/`, `foundation-extra/`, `foundation-review-4/` and `foundation-review-5/`. Individual paths and hashes are in the [publication inventory](evidence/foundation-extra/publication-manifest.json).

Earlier reports and evidence are preserved. Later report/evidence-only commits
must distinguish their publication HEAD from the exact implementation SHA that
actually underwent Cargo verification.

## Reproduce this scope

Use the recorded implementation SHA and Rust/Cargo versions. Set process-local `CARGO_NET_OFFLINE=true`, run `node audit/verify.mjs final-N` with a fresh attempt name, then `cargo test --workspace --all-targets --locked --offline`, `node --test audit/tooling.test.mjs` and `git diff --check`. Review overall status, source drift and actual test summaries; do not reuse a prior attempt directory.

The completed publication set was also checked by `audit/review.mjs::runReview({phase: "review-5"})` through Node: exit 0, zero findings/cycles and stable committed scoped inputs. See the [final scan receipt](evidence/foundation-review-5/source-review.json).

Source-review captures retain their actual logical phase names `review-4`/`review-5` in JSON. Their unchanged bytes are archived under `foundation-review-4`/`foundation-review-5` to avoid colliding with receipts from other CWG branches; the captures are not reruns or renamed source identities.
