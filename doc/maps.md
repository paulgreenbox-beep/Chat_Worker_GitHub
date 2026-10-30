# Architecture map

```mermaid
flowchart LR
  Author[ChatGPT / Task Author] --> GH[(GitHub Issues and Comments)]
  GH --> Receiver[CWG_GitHub Webhook / Poller]
  Receiver --> Guard[CWG_Security Verification]
  Guard --> State[(CWG_State Durable Inbox)]
  State --> Router[CWG_Router Exact Binding]
  Router --> Adapter[CWG_Worker Provider Adapter]
  Adapter --> ChatA[Codex thread A]
  Adapter --> ChatB[Codex thread B]
  ChatA --> Ack[Receipt / Completion]
  ChatB --> Ack
  Ack --> State
  State --> Outbound[Authorized GitHub Publisher]
  Outbound --> GH
```

```mermaid
stateDiagram-v2
  [*] --> PENDING
  PENDING --> VERIFIED: trusted signature + API reconciliation + authorization
  PENDING --> BLOCKED: invalid / unauthorized
  VERIFIED --> DELIVERING: atomic claim
  DELIVERING --> DELIVERED: provider accepted
  DELIVERING --> UNKNOWN: deadline / uncertain outcome
  DELIVERED --> ACKNOWLEDGED: worker receipt
  ACKNOWLEDGED --> COMPLETED: verified work outcome
  UNKNOWN --> BLOCKED: owner review
```

The event loop is not a shell runner. Transport acknowledgement, agent acknowledgement, and task completion are separate facts. No claim of active delivery until an actual provider integration is tested.
