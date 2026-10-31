# Chat Worker GitHub (CWG)

CWG is a standalone GitHub-backed transport from a task author to an **existing,
explicitly registered AI worker chat**. It is not part of Makepad SandStudio.

Target: Author → GitHub comment → authenticated receiver → durable inbox →
registered thread → provider acknowledgement. Task text is data, not execution
authority. No containers, VMs, model calls, automatic merge or installed service.

## Current source

| Component | Source in this branch | Qualification |
|---|---|---|
| CWG_Protocol / CWG_Router | Strict status decoding, logical routing and new cross-GOAL/suspension regressions from PR #1 | Historic foundation 70 PASS applies only to its recorded source; integrated HEAD NOT_RUN |
| CWG_Security | Bounded object-only JSON checks; Windows CNG HMAC; handle-based Windows ACL checks | Native results and exact SHA in the Windows audit below |
| CWG_State | Bounded protected file-locked/fsynced inbox; persistent replay protection | Native subprocess/fault results in the Windows audit; power-loss durability NOT_VERIFIED |
| CWG_GitHub | Foreground Windows receiver, fixed-host API reconciliation, local inbox CLI | Controlled-loopback receiver-to-inbox integration; live GitHub/Codex delivery NOT_RUN |
| CWG_MCP | Scoped stdio inbox tools and non-authoritative local reports from PR #3 | WP-01B integrated source NOT_RUN; files/spec protection not yet qualified |
| CWG_Worker | Trusted Codex App Server adapter | NOT_IMPLEMENTED |
| CWG_MCP / CWG_Worker | Planned MCP and Codex adapter | NOT_IMPLEMENTED |
| CWG_CheckBroker | Optional independent check integration | Disabled; not needed for ordinary messages |

The receiver is **not a qualified deployment**. Native Windows build, test and
security evidence, including initial failures and their fixes, is recorded in
[the Windows verification report](audit/RECEIVER_WINDOWS_VERIFICATION.md).
Results apply to the exact source SHA recorded there. This branch incorporates the additional security review
from PR #1 at `a7aa73858a85620f4ca10f09fcf75d86186f662c`, preserving its
scoped MessageKey, body budgets, binding suspension and all regression tests.
See [the preserved security review](audit/SECURITY_REVIEW.md); integrating that
review is separate from the native receiver verification.

## WP-01A foundation integration (NOT_RUN)

This isolated branch merges the current Protocol/Router Draft PR #1 source
(fbab27fe3a73103fa6193d25c484058877a0e62b) into Receiver/State Draft PR #2
(383bff0d070533a948d0ee9dfcb980fbb1775862). The merge commit has the Receiver
as first parent and the Protocol foundation as second parent. Neither existing
PR branch nor main has been changed.

- DeliveryStatus now accepts only canonical uppercase JSON strings.
- The four new Rust test files cover status wires, cross-GOAL replay,
  revocation/lifecycle and suspension interleavings.
- PR #2's native Windows protection, delivery aliases and strict persisted
  Transition decoder remain intact; prior tests and evidence are preserved.
- Foundation audit/evidence are carried by their existing Git blob bytes.

The prior 70-test foundation and 204-test receiver reports are historical
receipts for different earlier source SHAs. Full Rust/Cargo and Windows-native
checks on this new integrated HEAD are **NOT_RUN**. This remains source-only
integration: there is no qualified Codex Adapter, MCP, GitHub Publisher or
end-to-end delivery. See [WP-01A handoff](audit/WP01A_INTEGRATION.md).

## WP-01B source integration (pending independent qualification)

This branch merges PR #3 MCP source at `41e59503ba68bd9d5a7a9ca173c75aa2911d28c4` into the
qualified WP-01A integration baseline `a4a4957c932a6431ce1acbe0e98fc786e9e76cfd`. The hardened
State journal, Windows ACL, delivery aliases, strict Protocol and native
regressions are retained. The MCP-scoped `get_message` / `inbox_page`
read methods are added to the protected State transaction path.

PR #3 alone had **127 Rust tests PASS**, but also rustfmt/Clippy failures
and five reproduced MCP wire incompatibilities. These results are historical
and cannot qualify this merged commit. The MCP filesystem boundaries and
version-conformance improvements are separate source changes in this
development workstream. Until all exact-SHA Windows checks pass, WP-01B
remains **NOT_VERIFIED** and must not be treated as a trusted chat adapter.

No live GitHub-to-Codex delivery, provider identity proof, publisher,
automatic model execution, or CheckBroker is part of this integration.
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
packages are listed. The native audit records locked and offline verification.
Once the dependencies are cached, repeat with --offline. The implementation uses Windows system APIs
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
