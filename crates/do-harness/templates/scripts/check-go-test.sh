#!/usr/bin/env bash
# check-go-test.sh — the `test` sensor for the Go pack: run `go test ./...`.
# The `./...` package pattern is passed inside a script because the harness
# resolves path-looking argv arguments against the workspace and would treat
# `./...` as a missing file; the script keeps the exact `go test ./...` command.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

exec go test ./...
