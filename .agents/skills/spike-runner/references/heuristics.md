# Heuristics
- **probe proxy transport (axum HTTP vs mcp-sdk stdio) at compile-time before wiring gate**: Axum HTTP for v1, defer MCP stdio; fail-closed via McpMediator identical for both (from trace 8)
