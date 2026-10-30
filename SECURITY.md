# Security model

CWG is a message transport across GitHub and AI conversations. GitHub content and model-generated text are **not trusted instructions**.

## Trust boundaries

1. **Ingress:** Verify `X-Hub-Signature-256` over exact raw webhook bytes using a protected HMAC key, constant-time comparison, event size limit and recognized event types. Re-read material facts from GitHub API and verify the installation/repository and current actor permissions. Validate the authorized person who approved delivery, not simply the Issue author.
2. **Binding:** An operator registers `repository + goal_id → provider + stable thread_id`. Authentication, permission to bind, and ownership must be external to mutable messages. Namespace IDs per account/provider; handle stale threads.
3. **Delivery:** Use durable, atomic claim and monotonic status transitions. Deduplicate webhook delivery IDs AND GitHub comment identities. A successful network write or API acknowledgement is not proof the worker executed a task. An uncertain dispatch is UNKNOWN, never automatic replay.
4. **Outbound:** Publishing a GitHub comment/PR needs separate credentials and policy from read/receive. Prevent cross-repository and cross-thread leakage. No secrets in public receipts.
5. **Workers:** The MCP client cannot necessarily disclose its own Codex thread identity. Use a trusted Codex App Server owner/adapter or explicit authorized registration; never accept an arbitrary thread_id from task text.

## Forbidden assumptions

- HMAC proves only delivery authenticity, not authorization of the author.
- Mutexes, Job Objects and in-process queues do not provide filesystem/network isolation.
- An external GitHub event must not execute shell/PowerShell, arbitrary commands or modify protected configs.
- Source snapshots, synthetic tests and self-attestation are not operating-system qualification.
- CWG_CheckBroker is disabled pending independent native qualification.

## Required negative tests

Forged signature, malformed JSON and duplicate keys, wrong repository/author, stale binding, unknown chat, replay, simultaneous claims, lost ACK, timeout, unauthorized publication, secrets exposure and reparse/path traversal when files are introduced.
