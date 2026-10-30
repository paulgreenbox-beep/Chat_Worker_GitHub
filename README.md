# Chat Worker GitHub (CWG)

**CWG** is a standalone, GitHub-backed message bridge between a human/AI task author and an **existing, explicitly registered AI worker chat**. It is not part of Makepad SandStudio.

**Target flow:** Author → GitHub event/comment → authenticated CWG receiver → verified message → thread binding → provider adapter → acknowledgement/reply.

## Scope and status

- This repository begins as a **source-only foundation**, not a running webhook service, installed MCP server, or active Codex App Server bridge.
- GitHub is the durable transport/audit journal. GitHub Issue text is **untrusted data**, never execution authority.
- The same registered conversation should receive subsequent task updates; the display name of a chat is not its trusted identity.
- No automatic model calls, job execution, persistent background worker, secrets, containers or VMs are provisioned.
- Reuse and adapt concepts from [MSS GOAL PR #5](https://github.com/paulgreenbox-beep/makepad-sandstudio/pull/5), [dispatcher PR #7](https://github.com/paulgreenbox-beep/makepad-sandstudio/pull/7), and [broker PR #9](https://github.com/paulgreenbox-beep/makepad-sandstudio/pull/9), **not their approval/qualification claims**. In particular broker native isolation remains blocked.

## Modules

| Module | Responsibility |
|---|---|
| `CWG_Protocol` | Versioned message, binding, delivery and receipt schemas |
| `CWG_GitHub` | Webhook ingestion, API reconciliation and outbound comments |
| `CWG_Security` | Webhook signature, sender authorization, replay and scope checks |
| `CWG_Router` | Target repository + goal + registered thread routing |
| `CWG_MCP` | Agent-side tools for inbox, acknowledge and reply |
| `CWG_Worker` | Provider-specific chat adapters; Codex first |
| `CWG_State` | Durable message queue, idempotency and recovery |
| `CWG_CheckBroker` | Optional independent source-check interface; disabled until independently qualified |

See [architecture map](doc/maps.md), [delivery protocol](doc/protocol.md), [migration plan](doc/migration.md), [security requirements](SECURITY.md), and [agent rules](AGENTS.md).

## MVP acceptance

1. A GitHub event from an authorized repository and actor becomes a verified, deduplicated immutable message.
2. An owner-approved mapping binds the destination to a **stable provider thread ID**, not a mutable chat title.
3. One delivery to one registered thread is acknowledged and correlated to the original GitHub message.
4. Interrupted or uncertain dispatch is not blindly replayed; messages transition to `UNKNOWN` until reconciled.
5. Reply/comment publication requires separate GitHub write authorization.
6. A different repo/user/thread cannot impersonate another binding.
7. Independent unit and integration tests accompany each implemented boundary; no invented PASS.

## Current implementation

The initial Rust source provides typed protocols, bounded validation and an in-memory routing/queue model. Network webhook listener, GitHub authentication, persistent database, MCP transport, Codex App Server connection, native check execution and actual delivery are **NOT_IMPLEMENTED**.

## Development

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
```

These are proposed local commands, **not claimed to have passed** on a Windows host.
