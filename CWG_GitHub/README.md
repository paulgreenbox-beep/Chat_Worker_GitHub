# CWG_GitHub

Planned: receive signed GitHub webhooks; optionally poll GitHub as a recovery path; re-fetch Issues/comments over authenticated API; send approved replies. Webhook HMAC covers raw bytes, while actor authorization requires fresh API verification. Store delivery IDs, repository numeric IDs and canonical comment IDs. No network implementation yet; **NOT_IMPLEMENTED**.
