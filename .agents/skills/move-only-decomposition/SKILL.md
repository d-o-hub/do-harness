---
name: move-only-decomposition
description: >
  Decompose a file that is near the line ceiling without changing behaviour, and
  prove the move. Use when a loc/threshold sensor warns at or above the
  decomposition band, when `do-harness split` refuses because the file is under
  the hard ceiling, when an inline test module is what makes a file large, or
  when reviewing a refactor whose diff is almost entirely moved code. Triggers:
  "loc warning", "450 lines", "500-line ceiling", "extract module", "move code",
  "move-only diff", "split refuses".
license: MIT
---

# Move-Only Decomposition

A file in the warning band is a *feedforward* signal: decompose deliberately
before it reaches the hard ceiling, where the change stops being reviewable and
starts being urgent.

## Step 1 — find the band, then ignore the mechanical tool

```bash
do-harness loc --warn      # files at or above the decomposition threshold
```

`do-harness split` only acts on files that already exceed the hard ceiling; for a
file inside the warning band it answers "already under the ceiling" and does
nothing. In the band you choose the seam yourself — that is the point, because a
line-count-driven split produces a module nobody wants.

## Step 2 — pick a cohesive seam

Best seams, in order of how little they disturb reviewers:

1. **The inline `#[cfg(test)] mod tests`.** It is already a unit, and moving it
   changes no production code:

   ```rust
   #[cfg(test)]
   #[path = "<stem>_tests.rs"]
   mod tests;
   ```

   Keep the module *name* (`tests`) so test identities (`module::tests::case`)
   survive; the `#[path]` attribute — not a rename — is what the layout change is
   for. Child modules reach the parent's private items, so moved tests keep using
   the fixtures they already had.
2. **A behaviour group.** A set of functions that already share a subject
   (database commands, one language's scaffolding) becomes a submodule; keep the
   public path stable with a re-export:

   ```rust
   mod db;
   pub use db::{InitDbOpts, init_db, maintenance, seed};
   ```
3. **Constants that belong to one behaviour**, moved next to it.

Never split purely by line count, and never mix a rename or a behaviour fix into
the move — the proof below cannot distinguish your improvement from a mistake.

## Step 3 — keep the mechanics honest

- Fix relative paths: `include_str!("../templates/x")` becomes
  `include_str!("../../templates/x")` when the code moves one directory deeper.
- A moved item keeps its visibility unless the new path needs less: prefer
  `pub(super)`/`pub use` over widening everything.
- Delete the old declaration: two `mod` declarations for one file body is a
  compile error, and a stale inline copy is worse.

## Step 4 — prove the move

Raw line comparison is useless: `cargo fmt` re-wraps whatever it likes. Compare
**token streams with all whitespace collapsed**, then let the compiler and tests
speak:

```python
import re, sys
def tokens(path): return " ".join(open(path, encoding="utf-8").read().split())
old = re.search(r"mod tests \{\n(.*)\n\}$", open(src).read(), re.S).group(1)
print("tokens preserved =", tokens_body(old) == tokens(dest))
```

Then: `cargo fmt`, the affected test target, `cargo clippy -- -D warnings`, and
`do-harness loc --warn` to show the band cleared. A dropped or re-ordered line
surfaces as a failed assertion or an unused-import warning; anything else means
the move was not move-only.

## Anti-patterns

- **Splitting by line count alone.** Choosing the largest block instead of a
  cohesive one produces a module with no reason to exist and reviewers who cannot
  see the seam.
- **Proving the move by diffing text line by line.** `cargo fmt` re-wraps, so
  line comparison reports false differences and hides real ones; collapse
  whitespace first and confirm with the test suite.
- **Folding a rename or a fix into the move.** The move stops being provable and
  the review stops being cheap.
- **Leaving the inline copy behind.** The file keeps its size and the new module
  is dead code.

## Gotchas

- A file whose inline test module is most of its length is a one-step fix: the
  production code never moves.
- Re-exports keep call sites unchanged, which is what makes a large-looking diff
  safe to review.
