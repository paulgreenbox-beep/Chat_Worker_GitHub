# CWG_State

Bounded, append-only, file-locked local journal. Replays existing CWG_Router
contracts. Immutable numeric-source/account/generation bindings, source comment
and delivery dedupe, guarded transitions, fsync-before-success and fail-closed
corruption handling. All operations assume a trusted controller and protected
local storage; this is not caller authentication or a distributed database.

See [receiver guide](../doc/receiver.md) for capacity limits, crash uncertainty,
owner-only directory prerequisites and NOT_RUN Windows qualification.
