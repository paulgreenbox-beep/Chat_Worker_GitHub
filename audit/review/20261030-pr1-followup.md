# PR #1 follow-up: suspension and multi-route correlation

Reviewed parent: `5625e2f1e0fae21e0badd8f4ba5d304906928fe4`.

This source review adds three public-interface regression tests, not a new transport or authorization mechanism:

- A suspended binding retains legitimate already-in-flight Delivered/Unknown outcomes; a neighboring GOAL still completes independently.
- Refusing admission to a suspended binding reserves neither its message/comment identities nor the final message/body budget. A valid neighboring route can still admit the same previously refused identity.
- A forged cross-thread MessageKey cannot close another thread's UNKNOWN outcome or affect its neighbor.

Production Protocol/Router files and all previous tests are unchanged. No new P0/P1 claim is made; this is targeted logical coverage, not a full security certification.

## Actual verification in this follow-up

Changed-file UTF-8/final-newline/trailing-whitespace checks and scoped `git diff --check`: PASS. Test bodies were source-reviewed against the published public Router API. Rust, Cargo, rustfmt and Clippy are absent in the editing environment: **all new Rust execution, formatting and build gates are NOT_RUN**. No toolchain was installed. Earlier 70-test results apply only to their recorded source, not this new HEAD. The three additions are not counted as passing tests.

Required before acceptance: `cargo fmt --all -- --check`; `cargo test -p cwg-router --test suspension_interleavings --locked --offline`; complete workspace check/test/clippy/build gates on the resulting exact SHA. Preserve any failures and report missing prerequisites.

PR #2 and #3 were observed based on older dependency snapshots and with merge conflicts. Integrate #1 into #2, resolve without dropping tests, then integrate #2 into #3 and rerun on the combined tree. Do not copy a parent's PASS status to a child branch. This change neither merges nor rebases another branch. Keep Draft; main, MSS, secrets and background execution are untouched.
