# CWG protocol v1 (proposal)

`cwg-message-v1`: `message_id`, `source.repository`, `source.issue_number`, `source.comment_id`, `source.author_id`, `target.provider`, `target.thread_id`, `goal_id`, `body`.

Binding key: GitHub repository identity + GOAL ID. Mutable chat titles are display metadata only. A production binding should additionally bind numeric GitHub repository ID, provider/account identity and a generation to resist renaming and stale-chat reuse.

## Delivery semantics

- Github webhook `X-GitHub-Delivery` is deduplicated; canonical comment ID is independently deduplicated.
- A received webhook is not an authorized message. Re-fetch object and verify sender/permission on the current repository.
- Never interpolate message body into a command or trusted system prompt.
- The trusted chat adapter may send the body as explicitly untrusted task context after owner authorization.
- Persist claim before dispatch. On timeout after dispatch, `UNKNOWN` must be reconciled before retry.
- `DELIVERED` means provider accepted. `ACKNOWLEDGED` means worker independently confirmed. `COMPLETED` requires work evidence.
- Use separate schemas for per-message receipts and agent output; never let output modify binding policy.

## MCP versus chat delivery

CWG_MCP exposes inbox/read/ack/reply tools to an already-running agent. An MCP server alone cannot push messages into every idle chat. CWG_Worker requires a provider-specific adapter (for Codex, an authorized App Server integration), or the agent must poll its inbox.

Status: design + initial in-memory typed models; no live webhook/MCP/provider transport.
