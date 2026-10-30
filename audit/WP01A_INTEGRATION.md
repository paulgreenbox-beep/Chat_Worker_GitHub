# WP-01A: Protocol/Router into Receiver/State

**Status: SOURCE INTEGRATED, NATIVE VERIFICATION NOT_RUN, NOT ACCEPTED FOR MERGE.**

## Exact parents

- First parent: PR #2 Receiver/State: 383bff0d070533a948d0ee9dfcb980fbb1775862
- Second parent: PR #1 Protocol/Router: fbab27fe3a73103fa6193d25c484058877a0e62b
- Common ancestor: a7aa73858a85620f4ca10f09fcf75d86186f662c
- Historical foundation tested input: 34132daff82066dbe76bf89e3ec4f6b8e81084d9
- Historical Receiver tested input: 0963e4269e870d419e1f871b62a670625704b78c
- Isolated branch: codex/wp-01a-foundation-receiver-integration

The merge commit is a new untested SHA. The historical 70 foundation and
204 receiver PASS observations do not qualify it. Neither parent branch nor
main was modified. No existing PR was merged.

## Source reconciliation

Three-way review found exactly two paths modified differently on both sides:

1. .gitattributes: PR #2 receiver fixtures, retained native Windows
   receipt bytes and PR #1 foundation platform CRLF receipt rule are combined.
2. README.md: PR #2 Receiver/State description is retained, with a section
   describing PR #1 strict status and additional regressions, plus NOT_RUN.

All other paths changed only in one parent relative to their merge-base:
PR #2's files and native protections remain in place; PR #1's strict
DeliveryStatus decoder, status_wire, cross_goal_replay, review_lifecycle,
suspension_interleavings, audit tooling and exact evidence blobs are included.
Production CWG_Router/src/lib.rs was identical in both parents.
doc/migration.md contains a historical-scope clarification; no former
test assertions, journal bytes or registry dependency checksums are removed.

## Windows test-worker handoff

Check out the **exact integrated HEAD** from the isolated branch; record
full SHA, tool versions, stdout/stderr, exit codes and actual test counts.
Use the existing native Windows x64 MSVC toolchain and cache; do not install
software, run a provider model, expose a real webhook or write to main.

```powershell
git rev-parse HEAD
git diff --check
cargo metadata --locked --offline --format-version 1
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked --offline
cargo test --workspace --all-targets --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo build --workspace --locked --offline
cargo build --workspace --release --locked --offline
cargo test -p cwg-protocol --test status_wire --locked --offline
cargo test -p cwg-router --test cross_goal_replay --locked --offline
cargo test -p cwg-router --test review_lifecycle --locked --offline
cargo test -p cwg-router --test suspension_interleavings --locked --offline
cargo test -p cwg-state --lib journal_wire_tests --locked --offline
cargo test -p cwg-github --test http_fragmentation_review --locked --offline
cargo test -p cwg-state --test native_process --locked --offline
cargo test -p cwg-state --test windows_paths --locked --offline
cargo test -p cwg-state --test retained_access --locked --offline
node --test audit/tooling.test.mjs audit/stack.test.mjs
```

Run native CNG, WinHTTP and receiver-to-inbox fixture tests included in the
workspace test suite. Verify PR #1 full HEAD ancestry against the integration
commit. The two-edge verify-stack.mjs gate intentionally remains blocked
until PR #3 is integrated; record that as NOT_APPLICABLE for WP-01A.

Classify each independent result PASS / FAIL / NOT_RUN. Do not treat the
historical 70 or 204 tests as results from this new SHA. Record any Windows
symlink fixture EPERM as NOT_RUN (not PASS), and retain all failures.

## Scope and blockers

Source integration does not authorize production. Independent exact-HEAD
Cargo/rustfmt/clippy/build/native execution and code review remain required.
Out of scope: PR #3, MCP integration or hardening, Codex Adapter, Publisher,
real delivery, secrets, services, revocation/compaction, check broker and
any merge or auto-merge of existing PRs.
