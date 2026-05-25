---
name: webhook
description: Configure and operate the HTTP webhook connector through puffer serve.
allowed-tools:
  - Bash
argument-hint: "[webhook connector task]"
arguments: target
user-invocable: true
disable-model-invocation: false
---

Use the `webhook` connector when the user wants an HTTP endpoint that forwards
posted messages into Puffer through `puffer serve`.

Target: $target

Configuration lives in `.puffer/connectors.toml` or `~/.puffer/connectors.toml`
under `[connectors.webhook]`. The connector requires a bind address and may
use an optional bearer token.

```toml
[connectors.webhook]
bind_address = "127.0.0.1:8080"
path = "/puffer"
auth_token = "..."
```

Callers send JSON to `POST {path}` with `conversation_id`, `message`, and an
optional `user_id`. Run `puffer serve` after configuring the connector. The
connector handles inbound webhook messages directly through the connector
runtime; it is not a workflow trigger yet and should not be used in
`WorkflowCreate` connection triggers until a typed subscriber or connector
protocol stream exists.
