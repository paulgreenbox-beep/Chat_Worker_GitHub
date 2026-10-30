# Receiver review follow-up — 2026-10-30

Starting PR #2 source: `a8ba353172126b56edf43ab66fb375ddf4486bcb`.

Reviewed the current created-comment reconciliation, exact-byte request reader, replay aliases, durable transaction synchronization and the opened-handle security boundary. No production Receiver/Security/State code was changed in this follow-up. Earlier 204-test Windows evidence is retained for its exact recorded implementation and is not claimed for this new HEAD.

## New tests

`CWG_GitHub/tests/http_fragmentation_review.rs` adds four public-interface cases:

1. 1/2/3/7/64-byte short reads preserve exact UTF-8/escaped JSON body bytes for later HMAC verification.
2. Exact MAX_EVENT_BYTES body accepted; MAX+1 refused before trying to read a body.
3. Identical mixed-case duplicate Host/event/delivery headers refused rather than normalized away.
4. Content-Length consumes only the declared bytes, leaving subsequent bytes unread; the server still closes after one request, not pipelining.

These tests do not use a real GitHub credential, public webhook endpoint, TLS proxy, model or native access-control proof. They are **NOT_RUN** here because Rust/Cargo are unavailable.

## Dependency issue — still open

At review start, this branch was based on #1 commit `a7aa73858a85620f4ca10f09fcf75d86186f662c`, not the newer `8bece841b4d7050b68ef8780056d5167d4158ed7`. GitHub compare reported 5 missing commits and divergent history. Integrate current #1 through a reviewed nonrewriting merge before assuming that its strict DeliveryStatus decoder/new tests are present. The read-only ancestry gate and 11 executed Node/Git tests were added in PR #1 under `audit/verify-stack.mjs` and `audit/stack.test.mjs`; they are not copied into this stale branch as substitute proof.

PR #3 is separately based on an older version of this receiver and does not contain these newer native ACL and synchronization fixes. Review the combined final source, not merely each PR independently.

## Additional source observation

`CWG_State::Transition` still uses derived Serde enum deserialization, unlike #1's stricter DeliveryStatus. If journal transitions are intended to have a string-only wire representation, add an object-vs-string regression and a narrowly reviewed decoder change. This observation is not a proven authorization bypass and was not changed in this follow-up.

## Required verification

```sh
cargo test -p cwg-github --test http_fragmentation_review --locked --offline
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked --offline
cargo test --workspace --all-targets --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
git diff --check
```

Repeat the previously required metadata/debug/release gates and native Windows suites on the final reconciled source. Current added Rust tests, compilation, rustfmt and Clippy: **NOT_RUN**. No automatic merge, workflow enablement, production configuration or secret change occurred. Keep Draft.
