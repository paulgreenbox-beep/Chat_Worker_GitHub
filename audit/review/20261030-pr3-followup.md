# PR #3 follow-up: MCP parameter and handshake boundaries

Reviewed parent: `0dc65996110233c3004288c4ba49a1ae76b73f13`.

## Source findings and corrections

1. `notifications/initialized` advanced the legacy handshake without checking its parameters. A notification containing an array, null or malformed metadata could open the tool gate. It is now ignored without a reply or phase transition when invalid.
2. `tools/list` treated explicit null/arrays/scalars like omitted params; metadata/cursor shapes were unchecked. Omitted params and valid metadata objects remain supported; invalid containers and unsupported cursors are refused explicitly.
3. `check_phase` changed New to Modern before the first request's method parameters were admitted. A rejected list/tool envelope could prevent a later valid legacy handshake. Phase checks are now observational; adoption occurs only after a successful request result.
4. ClientInfo previously required only an object. Required name/version types and optional modern ClientInfo are now checked; `_meta` must be an object for legacy calls as well.

Eight new regression functions exercise these boundaries, including absence of an outbox write for malformed report-call metadata. All prior test bodies/assertions are preserved; three existing test-only helpers gain parent visibility for reuse. No public production authorization bypass is added. The catalog remains four tools; no ACK/complete tool, chat push, GitHub publisher, model call or network operation is introduced.

## Verification actually performed

Original lib.rs reconstruction matched Git blob `2b397b658f11d2895a48a706912509f054026cef`. Baseline identity, old-test preservation (apart from test-helper visibility), final newlines/trailing whitespace, absence of new unsafe blocks and scoped `git diff --check`: PASS. **Rust fmt/check/test/clippy/build and Codex integration: NOT_RUN**, toolchain absent. Eight additions are not eight observed passes. Formatting must be applied and checked on the target runner; historical parent results do not qualify this HEAD.

Required gates: `cargo fmt --all -- --check`; `cargo test -p cwg-mcp --lib rpc_boundary_tests --locked --offline`; complete locked/offline workspace tests/check/clippy/build and two-client qualification. Check against the official MCP lifecycle/schema, not only local synthetic fixtures. Reference: https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle and the project's selected modern schema.

## Blocking integration review

PR #3 is still built on the older receiver snapshot, not the hardened current PR #2. GitHub reports a base conflict. Do not merely retarget the PR or overwrite State to resolve it. Integrate #1 into #2 and #2 into #3 preserving strict status/transition decoders, native ACL/opened-handle checks, uncertain-fsync reconciliation and replay aliases. Then port the MCP read methods to that protected State implementation and rerun exact-head tests.

Outbox/config protection and TOCTOU: the current outbox uses metadata path checks before an ordinary OpenOptions call. This is not equivalent to the protected opened-handle ACL/ancestor mechanism in PR #2; Windows hardlink/race and hostile same-user scenarios remain unqualified. Do not deploy until shared protected-file access is reviewed and tested. No live calling Codex thread is authenticated by this patch. Full protocol conformance (including pre-initialize ping), native isolation and live delivery remain separate open gates.

Keep Draft. Main, other branches, historic receipts, dependency versions, permissions and deployment settings are untouched by this commit.
