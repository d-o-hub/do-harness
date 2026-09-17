# Heuristics
- **treat the setup-node NODE_AUTH_TOKEN placeholder as unset and clear it, or the CI job silently takes the token branch and never attempts OIDC**: Writing or debugging a GitHub Actions npm trusted-publishing job that sets registry-url via actions/setup-node (from trace 3)
