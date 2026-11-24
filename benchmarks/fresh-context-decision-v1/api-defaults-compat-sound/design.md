# Design: public API timeout option (sound variant)

## Objective

Extend the public client configuration API with a timeout option while
preserving behavior for existing callers.

## Proposed contract

- `connect(host, port)` returns a Connection.
- A new optional `timeout` parameter is added; the default is the legacy
  effective default (**30 seconds**), so no caller observes a change.
- The deprecated `timeout_seconds` keyword alias remains and forwards to
  `timeout`, logging a deprecation warning; it is scheduled for removal two
  releases after the new parameter ships.

## Design decisions

### Default preservation

The new parameter defaults to the legacy 30 seconds. Existing callers are
byte-for-byte behaviorally identical.

### Deprecation window

`timeout_seconds` continues to work and emits a deprecation warning. The
removal is scheduled, documented, and never in the same release as the new
parameter.

### Compatibility tests

A fixture asserts: no-argument callers keep 30 seconds; `timeout=5` works;
`timeout_seconds=5` works with a warning; `timeout` and `timeout_seconds`
cannot both be passed (error). These tests are part of the ordinary suite.

## Open questions

- Should the deprecation warning be emitted once per process or per call?
  (Decision: once per process.)

## Files

- `config.py` — the frozen pre-implementation source snapshot.
