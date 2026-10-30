# MSS → CWG migration plan

CWG is independent of MSS. Reference sources are **draft, unmerged** PRs, not validated deployable packages.

- MSS [PR #5](https://github.com/paulgreenbox-beep/makepad-sandstudio/pull/5): reuse GOAL registry design, check catalog and append-only evidence semantics, generalize names/schema to CWG.
- MSS [PR #7](https://github.com/paulgreenbox-beep/makepad-sandstudio/pull/7): reuse authorization, exact event gates, concurrency, publication separation and replay protections. Do **not** inherit disabled dispatcher as if working.
- MSS [PR #9](https://github.com/paulgreenbox-beep/makepad-sandstudio/pull/9): optional Windows broker patterns only; production isolation is BLOCKED and must not be used to authorize CWG delivery.

## Extraction stages

1. Establish independent CWG protocol, authenticated repo→goal→thread bindings and durable inbox.
2. Implement GitHub webhook HMAC + GitHub API reconciliation with negative tests.
3. Integrate Codex App Server thread association with explicit authorization, then MCP read/ack/reply fallback.
4. Add receipts, outbound GitHub publisher and end-to-end two-thread isolation tests.
5. If desired, separately qualify an optional native check broker; it is **not required for ordinary chat message transport**.

Do not delete or merge MSS PRs automatically. Do not import MSS_Core/MSS_Remote/MSS_Render/MSS_Desktop. No Docker/WSL/VM.

The CWG source audit strengthens only the two existing Rust crates: validated wire models, immutable validated bindings, canonical repository keys and the in-memory state machine. Stage 1 remains partial: authenticated bindings and durable inbox are not implemented. No MSS source/dependency or qualification claim is imported. Webhook/MCP/Codex adapter stages remain separate work; [verification evidence](../audit/REPORT.md) does not accept those stages.
