# Design: public API timeout option (flawed variant)

## Objective

Extend the public client configuration API with a timeout option while
preserving behavior for existing callers.

## Proposed contract

- `connect(host, port)` returns a Connection.
- A new optional `timeout` parameter is added; the default is **5 seconds**
  (previously the effective default was 30 seconds).
- The deprecated `timeout_seconds` keyword alias is **removed**; callers
  that still pass it fail with TypeError.

## Design decisions

### Default change

The default timeout changes from 30 to 5 seconds. The rationale recorded in
the issue is that 5 seconds is the safer default for new callers. Existing
callers that relied on the 30-second default now time out earlier.

### Deprecated alias

`timeout_seconds` is removed in the same release the new parameter is
introduced, so the API surface has exactly one keyword. No deprecation
window is kept.

### Compatibility tests

Existing tests cover the new parameter only; no test asserts the legacy
default behavior or the alias.

## Open questions

- Should the deprecation warning be emitted once per process or per call?

## Files

- `config.py` — the frozen pre-implementation source snapshot.
