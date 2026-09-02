# guardian-proxy

Optional fail-closed guardian proxy — adjacent to `do-harness` dev harness, not part of it.

- **Off by default:** no `agent-governance` dep unless `agt-governance` feature is enabled.
- **Fail-closed:** `ProxyMediator::decide` returns `Deny` on invalid params or governance denial, never allow on error.
- **Transport v1:** validates `ProxyConfig { bind, upstream, agent_id }` (deny_unknown_fields) and mediator wiring; HTTP reverse-proxy (`axum`) deferred to next slice.

Run:

```bash
cargo check -p guardian-proxy
cargo check -p guardian-proxy --features agt-governance
cargo test -p guardian-proxy
cargo run -p guardian-proxy -- --config guardian-proxy.toml
```

See `plans/agt-governance-epic.md` for promotion criteria.
