# CWG stacked-PR source review — 2026-10-30

## Scope and starting identities

Reviewed Protocol/Router, receiver framing/reconciliation/journal boundaries and MCP framing/report persistence in the existing draft branches:

| PR | Starting HEAD |
|---|---|
| #1 | `8bece841b4d7050b68ef8780056d5167d4158ed7` |
| #2 | `a8ba353172126b56edf43ab66fb375ddf4486bcb` |
| #3 | `7931b04a7e3d1d0e9fa43086cbf1df6819e64eeb` |

This is a targeted source review, not an exhaustive security certification or independent repetition of the earlier Windows runs. Existing histories and evidence are retained. No main merge, force push, secret change, service or model call is part of this review.

## Integration blocker

PR #2 forked from older #1 commit `a7aa73858a85620f4ca10f09fcf75d86186f662c`; GitHub compare reports it diverged from the current #1 (5 missing commits at the initial observation). Its Protocol still lacks the newer string-only DeliveryStatus decoder. PR #3 forked from the old #2 baseline `94f5d4325831c57746c489029b1c234f3f8493f7`; it does not inherit the later native handle/ACL, uncertain-sync and replay fixes. A parent PR's current PASS is not inherited by a child based on its older source.

The new read-only `audit/verify-stack.mjs` gate resolves exact local SHAs and checks ancestry, rejecting absent refs, shallow history, stale dependencies and observed ref changes. It ignores Git replacement objects and process-level Git routing overrides. It does not fetch, merge, validate code equivalence, authenticate remote GitHub state or run application code. Fresh owner-controlled refs and a trusted local repository remain prerequisites; an ancestry PASS alone cannot certify runtime behavior.

```sh
node --test audit/stack.test.mjs
# Refresh the two upstream/child remote refs using your normal reviewed process first.
node audit/verify-stack.mjs /absolute/path/to/Chat_Worker_GitHub
```

The CLI exits 2 for a blocked stack; it performs no automatic repair. Merge/reconcile #1 into #2, then #2 into #3 under review, retaining MCP read APIs and every newer security fix. Recheck the final combined source. No such merge was performed here.

## Changes in this branch

Two public Router regressions cover revocation during an in-flight attempt without reopening UNKNOWN/BLOCKED work, independent neighboring completion, capacity refusal without consuming either replay key, and non-eviction after completion. Production Protocol/Router behavior and existing tests are unchanged.

## Findings patched in PR #3

- P2: a report duplicate previously returned success without fsync, allowing visible but unsynced bytes from an uncertain append to be labeled persisted. Both new writes and duplicate reconciliation now synchronize.
- P2: a matching earlier report could bypass the 4,096-record limit on replay. Enforce it during the full scan before duplicate success.
- P3: reject positional report arrays; the journal writer emits named objects only. Keep raw duplicate-key rejection.
- P3: the maximum stdio payload fit with LF but not CRLF. Separate payload budget from the two-byte terminator allowance, with fragmented-boundary regressions.

Report persistence and source validation do not authenticate a Codex caller or prove delivery. MCP protected config/report-handle access and actual client integration remain separate blockers.

## Actual verification

`node --check` on both new audit files and `node --test audit/stack.test.mjs` ran locally with Node 22.16.0 and Git 2.47.3 on Linux: **11 tests PASS, 0 failed, 0 skipped**. Tests use owned synthetic Git repositories, not model calls or production data. Exact file hashes are in `audit/evidence/review-stack-20261030.json`.

Rust/Cargo/rustfmt/Clippy and Windows-native checks for these additions: **NOT_RUN** (toolchain/Windows host unavailable here). Fourteen Rust tests were added across the three branches (2 Router, 4 HTTP reader, 6 report journal, 2 stdio framing); none is counted as executed. Historical 70/204 PASS results remain valid only for their recorded sources, not these new heads.

## Required next gates

Run the seven existing Cargo build/test checks, locked/offline repeat, `git diff --check`, new targeted tests and the ancestry gate on the reviewed combined tree. Repeat Windows filesystem/concurrency checks for MCP after adopting the current receiver boundary. Keep all PRs Draft and live delivery NOT_RUN until separately qualified.
