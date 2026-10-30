# CWG_Security

Bounded two-pass JSON validation retains duplicate-key detection, including
ignored GitHub extension objects. Windows CNG supplies HMAC-SHA256; a fixed-work
comparison checks the supplied MAC. No home-grown SHA/HMAC implementation.
Non-Windows signature verification explicitly refuses. Signature authentication
is separate from GitHub API author/repository reconciliation in CWG_GitHub.

See [receiver guide](../doc/receiver.md). New tests and native qualification are
NOT_RUN; source availability is not an approval to deploy.
