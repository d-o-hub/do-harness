# Design: label whitespace normalization (routine low-risk control)

## Objective

Normalize label trimming in the public text helper: strip surrounding
whitespace from label inputs before they are returned.

## Proposed contract

- `strip_label(value)` returns `value` with leading and trailing whitespace
  removed.
- `strip_label(None)` returns `None` (unchanged).

## Design decisions

This is a routine, mechanical change: one helper, one behavior, no state, no
schema, no authorization surface, and no compatibility surface beyond the
documented None passthrough. There is no consequential design choice to
review; the ordinary workflow's change-and-verify path covers it.

## Open questions

- None.

## Files

- `textutil.py` — the frozen pre-implementation source snapshot.
