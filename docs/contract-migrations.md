# Contract migrations

Every adoption-contract version transition is recorded here as a migration
note. A migration note answers the three questions an adopter has when their
config stops parsing or starts warning: what changed, why, and how to move.
The version rules and the contract-first workflow live in
[versioning.md](versioning.md).

## Sample note: v0 → v1 (schema-version introduced)

Version 1 introduced the explicit `schema-version` key and the `migrate`
command. It is the reference shape for future notes.

**Changed:** `do-harness.toml` now carries `schema-version = 1` as its first
line. Files without it are version 0.

**Why:** before versioning, there was no machine-readable way for an adopter
to know which binary schema their config was written for, and no way for a
newer binary to recognize an older file and migrate it deterministically.

**How to move:** run `do-harness migrate --diff` to preview and
`do-harness migrate --write` to apply. The migration declares the version and
replaces the deprecated `allow_failure` alias with `severity = "warn"`.
`migrate` refuses ambiguous blocks and refuses files from a newer binary;
`doctor` reports either skew and names the fix.

**Bump:** `CONFIG_SCHEMA_VERSION` 0 → 1.

## Template for new notes

When you bump `CONFIG_SCHEMA_VERSION`, append a note with this shape:

    **Changed:** <what the parser now accepts or rejects>
    **Why:** <the adopter-visible problem this version solves>
    **How to move:** <the exact commands or edits an adopter runs>
    **Bump:** <old version> → <new version>

Keep the note in this file and repeat it in the PR body. A bump without a
note fails the contract-first review gate; a provisional note that does not
match the landed schema is a merge-blocking discrepancy.
