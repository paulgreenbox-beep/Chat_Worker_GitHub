# PR #2 follow-up: canonical persisted transitions

Reviewed parent: `7233ad941c36a0c9ee0a72dd3dde75d6a4814cf9`.

## Finding and correction

The public `Transition` enum still derived Serde Deserialize for an externally tagged unit enum. That admits object/null representations in addition to the uppercase string written by the journal serializer, the same class of wire ambiguity previously fixed for DeliveryStatus in PR #1. This is a format-hardening issue, not evidence of an authentication bypass. The new `transition.rs` implements a string-only visitor; `cwg_state::Transition` remains re-exported at its original public path. All six variant names and writer bytes are unchanged. No stored record is silently normalized or rewritten.

Four new pure parser regressions cover canonical round trips, all six object/null aliases both directly and inside Record/Event, other invalid JSON shapes, and no event application after a parse failure. Existing commit fault tests are byte-identical. Filesystem/ACL, transaction, HMAC, API and FFI code is unchanged.

## Verification and acceptance

Reconstructed original lib.rs was checked against Git blob `cbf07d53ed8b06e4c5826540f44e2123b3ff66fc` before editing. Scoped source checks: baseline identity, old-test preservation, UTF-8/final newline/trailing whitespace and `git diff --check` PASS. **Rust fmt/check/test/clippy/build and new native tests: NOT_RUN**, because no Rust toolchain is available here; none was installed. New tests are added, not declared passing. The earlier Windows 204-test report remains historical evidence for its own SHA, not qualification of this update. Non-JSON Serde formats remain NOT_VERIFIED.

Required gates on Windows: `cargo fmt --all -- --check`; `cargo test -p cwg-state --lib journal_wire_tests --locked --offline`; full locked/offline workspace tests/check/clippy/build, followed by existing native receiver/journal regressions. Confirm that an invalid persisted transition is refused without deleting/truncating records. A failing regression before the fix has not been executed here and must not be invented.

## Stack blockers

This branch still requires deliberate integration of the current PR #1 and its strict DeliveryStatus decoder. The MCP child PR uses an older State snapshot and must incorporate this correction together with the native journal/ACL changes, not merely rename its base ref. Conflicts must be resolved without dropping native safety checks or old tests. Keep Draft. Main, dependency versions, secrets, model calls and MSS remain untouched.
