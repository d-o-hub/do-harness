#!/usr/bin/env bash
# check-go-staticcheck.sh — the optional `staticcheck` sensor for the Go pack:
# run `staticcheck ./...`. The `./...` package pattern is passed inside a
# script because the harness resolves path-looking argv arguments against the
# workspace and would treat `./...` as a missing file.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

exec staticcheck ./...
