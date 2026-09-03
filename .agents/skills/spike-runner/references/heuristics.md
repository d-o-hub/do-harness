# Heuristics
- **probe proxy transport (axum HTTP vs mcp-sdk stdio) at compile-time before wiring gate**: Axum HTTP for v1, defer MCP stdio; fail-closed via McpMediator identical for both (from trace 8)
- **send User-Agent (and Accept) headers on external API probes**: unauthenticated crates.io API returns 403 without User-Agent; always set `-A` identifier plus `Accept: application/json` on crates.io / api.github.com spikes (from trace 10)
