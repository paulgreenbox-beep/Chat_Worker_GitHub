# Chat Worker GitHub (CWG)

CWG is a standalone GitHub-backed transport from a task author to an **existing,
explicitly registered AI worker chat**. It is not part of Makepad SandStudio.

Target: Author → GitHub comment → authenticated receiver → durable inbox →
registered thread → provider acknowledgement. Task text is data, not execution
authority. No containers, VMs, model calls, automatic merge or installed service.

## Current source

| Component | Source in this branch | Qualification |
|---|---|---|
| CWG_Protocol / CWG_Router | Existing validated contracts and routing | Prior audit at its recorded SHA; unchanged here |
| CWG_Security | Bounded duplicate-key JSON checks; Windows CNG HMAC | New Rust/native tests NOT_RUN |
| CWG_State | Bounded file-locked/fsynced persistent inbox; replay protection | New Rust/native tests NOT_RUN |
| CWG_GitHub | Foreground Windows receiver, fixed-host API reconciliation, local inbox CLI | Build/live webhook E2E NOT_RUN |
| CWG_MCP / CWG_Worker | Planned MCP and Codex adapter | NOT_IMPLEMENTED |
| CWG_CheckBroker | Optional independent check integration | Disabled; not needed for ordinary messages |

The new receiver is implementation source, **not a qualified release**. The
editing environment lacked Rust and Windows. No claim is made that its new tests
passed or that this branch currently compiles. The earlier 38-test audit applies
only to its recorded Protocol/Router source, not to these additions.

## Receiver workflow

An owner configures an exact numeric repository/owner/Issue → account/thread
binding and authorized numeric senders. A created comment must pass raw HMAC,
live GitHub reconciliation and current author permission checks before entering
the persistent queue. Changing comment text cannot change the destination.
Repeated deliveries do not create duplicate messages. The queue preserves
non-retryable interrupted delivery state and refuses torn/corrupt records.

See [receiver installation, API and security guide](doc/receiver.md) and the
[inert Windows example](examples/receiver.windows.example.json). No webhook,
proxy, credentials, background process or chat connection is installed by this
repository. Local inbox inspection is not delivery to Codex.

## Development

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --locked
```

The lock retains existing external versions/checksums; only new local workspace
packages are listed. Cargo lock validation remains NOT_RUN. Once the dependencies
are cached, repeat with --offline. The implementation uses Windows system APIs
instead of adding new external crypto/network packages.

Read [AGENTS.md](AGENTS.md), [SECURITY.md](SECURITY.md), the baseline
[protocol](doc/protocol.md), [architecture map](doc/maps.md), and
[migration plan](doc/migration.md). Those baseline documents describe the earlier
foundation; [doc/receiver.md](doc/receiver.md) specifies the additions and their
remaining trust/verification gaps. Historical [audit](audit/REPORT.md) and tests
are preserved, not relabelled as current acceptance.

## End-to-end acceptance still open

A successful durable enqueue does not prove provider acceptance, worker receipt
or completed work. Actual Codex connection/account/thread isolation, MCP caller
authorization and outbound replies need separate implementation and approval.
Do not claim end-to-end readiness until those paths have been tested on two
real registered chats with explicit owner authorization.
