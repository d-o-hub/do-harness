#!/usr/bin/env bash
# check-go-vet.sh — the `vet` sensor for the Go pack: run `go vet ./...`.
# The `./...` package pattern is passed inside a script because the harness
# resolves path-looking argv arguments against the workspace and would treat
# `./...` as a missing file; the script keeps the exact `go vet ./...` command.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

exec go vet ./...
