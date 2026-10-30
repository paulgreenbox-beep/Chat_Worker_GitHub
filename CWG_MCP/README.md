# CWG_MCP — local agent inbox connector

Native Rust **stdio MCP** server connected to an already verified CWG
durable inbox. No GitHub token, network listener, Codex model call, shell or
automatic execution. This component is independent of MSS.

## Contract

| Tool | Input | Output |
| --- | --- | --- |
| `cwg_binding_status` | Empty object | Pinned repo, GOAL and provider thread metadata |
| `cwg_inbox_list` | Optional `offset` and `limit` | Message IDs, GitHub source and status, no body |
| `cwg_message_get` | `message_id` | One bound message with **untrusted** task text |
| `cwg_report_progress` | `message_id`, `report_id`, `kind`, `body` | Durable local report status; **not** publication or task completion |

The server is pinned to exactly **one operator-configured binding per process**.
Model tool arguments cannot select another repository, GOAL, account or thread.
The binding must already exist in the durable journal. Reports append to a
separate bounded local `worker-reports.jsonl` with fsync and idempotent IDs.

**No `cwg_acknowledge` is exposed yet.** MCP requests cannot independently
prove that a provider accepted input or that the worker performed the task.
Reading a message never changes its delivery state. No automatic retry for
UNKNOWN outcomes is performed.

## Build and configure (Windows)

After independent security qualification of the underlying receiver, create
an owner-only state directory and reviewed receiver configuration. Register an
approved binding by running the receiver. On the reviewed Rust toolchain:

```powershell
cargo fmt --all -- --check
cargo test --workspace --all-targets --locked
cargo build --release --locked -p cwg-mcp
```

Example dedicated Codex MCP configuration:

```toml
[mcp_servers.cwg_development]
command = "C:/CWG/target/release/cwg-mcp.exe"
args = ["C:/CWG-Private/receiver.json", "development"]
```

The arguments are a protected absolute receiver config and an approved binding
ID, not caller-supplied task data. The MCP server does not require GitHub
tokens. Its owner must protect the config/state parent namespaces with ACLs.

**Important:** Codex can expose an MCP server in more than one chat. The MCP
protocol does not authenticate which Codex thread invoked it; these pinned
metadata alone do NOT establish real chat ownership. A trusted launcher or
Codex App Server integration must authorize the exact account/thread before
production use. Do not expose to untrusted same-user processes.

## Wire and operational scope

The server accepts bounded newline-delimited JSON-RPC on stdin and emits
JSON-RPC only on stdout. It supports legacy `initialize` →
`notifications/initialized` through 2025-11-25, and the stateless 2026-07-28
MCP envelope (`server/discover`, required per-request metadata).
Incoming frames are capped at 128 KiB. Duplicate JSON keys, unknown tools,
wrong bindings, bad arguments and malformed framing are refused.

An agent explicitly calls `cwg_inbox_list` and `cwg_message_get`; this
server **does not wake an idle Codex chat**. For proactive chat delivery, a
separately authenticated Codex App Server adapter is required.

Reports are local data only and are not pushed back to GitHub. An authorized
publisher and independent verification would be needed for outbound writes.

Tests are included, but new Rust compile/test, Codex interop, real Windows ACL
isolation and native GitHub delivery are **NOT_RUN** until independently
executed and recorded. No containers or virtual machines are used.

See [MCP wire specification](../doc/mcp.md).
