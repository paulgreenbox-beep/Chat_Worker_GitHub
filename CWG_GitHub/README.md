# CWG_GitHub

Windows-native foreground `cwg-github` binary: strict loopback webhook receiver,
current GitHub source/actor reconciliation, explicit config-bound route, durable
inbox enqueue and local inbox inspection. No shell or model invocation.

See [receiver guide](../doc/receiver.md) for configuration, limits, trust boundaries,
tests and commands. See the [native Windows audit](../audit/RECEIVER_WINDOWS_VERIFICATION.md)
for executed checks, exact source SHA and remaining limits.
Actual Codex delivery, MCP and outbound comments are not implemented here.
