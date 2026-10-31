# WP-01B — MCP + hardened Receiver/State development

Status: **DEVELOPMENT SOURCE ONLY — NOT_RUN; NO-GO for transport or merge**.

First parent WP-01A: `a4a4957c932a6431ce1acbe0e98fc786e9e76cfd`

Second parent PR #3: `41e59503ba68bd9d5a7a9ca173c75aa2911d28c4`

WP-01A was independently qualified: seven Cargo gates, 220 workspace Rust tests.
PR #3 was independently evaluated: 127 Rust tests PASS; fmt FAIL (127
fragments / 12 files), Clippy FAIL, MCP probes 55 PASS / 5 FAIL.
Those historical results do not qualify any newly integrated SHA.

Integration decisions:
- Preserve the first-parent hardened State, its Windows ACL opened handles,
  delivery aliases, strict Transition decoding, read/no-op sync barriers,
  Protocol string-only DeliveryStatus, Router regressions and evidence.
- Port `get_message`/`inbox_page` using existing protected `Store::transact`.
- Add MCP workspace member/package to `Cargo.toml` and `Cargo.lock` without
  upgrading or changing externally pinned registry packages.
- Retain PR #3's MCP stdio, report-journal code and original tests.
- Retain all original historical audit blobs; do not generate new PASS.

Open independent qualification:
- Full locked/offline Cargo metadata/fmt/check/test/clippy/debug/release.
- Nine preserved WP-01A target cases plus MCP tests and native coverage.
- MCP modern cache fields, unsupported version code/data and versioned ping.
- MCP config/outbox Windows protected handles, ACL, reparse and writer tests.
- Scoped read pagination/no ACK and report journal recovery/output budgets.
- Node audit regressions; one symlink fixture historically NOT_RUN: EPERM.

Out of scope: real webhook, provider model, account/thread attestation,
durable revocation, journal compaction, GitHub publisher, CheckBroker.

## WP-01B follow-up source changes — NOT_RUN

- Add `open_private_reports_journal` as a fixed-leaf held Windows handle
  using the existing Security ACL policy, without new external packages.
- Replace MCP config metadata-before-open with `read_private_config`.
- Replace report journal symlink-before-open with protected native handles.
- Add State `scoped_reads.rs` native pagination/no-ACK regressions.
- Add MCP versioned wire tests, native protected-file tests and output limit.
- Implement modern `ttlMs`/`cacheScope`, structured unsupported-version
  `-32022` and corrected modern/legacy `ping`; fix Clippy nested conditional.
- The incoming 127 PR #3 tests and all 220 WP-01A tests are historical
  counts; expected new count must be discovered by native test execution.

**Every Cargo gate, native protection and MCP conformance result on this
new combined source is NOT_RUN.** Existing published evidence remains
untouched. No publisher, model, thread authorization or real delivery.
