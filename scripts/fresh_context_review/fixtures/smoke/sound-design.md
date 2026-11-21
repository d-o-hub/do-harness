# Design: additive nullable column backfill migration (synthetic fixture)

Status: frozen for fresh-context review (this document is the review input,
not a proposal under active change).

## Objective

Add an optional `retention_hours` column to the `jobs` table and populate it
for historical rows without a table rewrite or a lock that blocks writers.

## Intended behavior

1. Schema migration `0016_add_retention_hours` adds
   `retention_hours INTEGER` (nullable).
2. Backfill runs in bounded batches (1000 rows per batch) during a
   maintenance window, idempotently (`WHERE retention_hours IS NULL`).
3. Writers are never blocked: the column is nullable, so insert paths in
   this release continue to work without supplying the value.

## Explicit constraints

- The migration must be repeatable and safe to run on a partially backfilled
  database (idempotent, no destructive step).
- Existing reads and writes of `jobs` rows must remain compatible; no
  existing index changes are in scope.
- A rollback plan must leave the database at the pre-migration schema with
  no data loss.

## Proposed contract

```
migration 0016: ADD COLUMN retention_hours INTEGER NULL
backfill: UPDATE jobs SET retention_hours = :default
          WHERE retention_hours IS NULL LIMIT 1000
rollback: none required beyond dropping the column (data loss acceptable only
          for the new column itself)
```

## Relevant paths

- Module: `crates/db/migrations/0016_add_retention_hours.sql`
- Tests: `crates/db/tests/migrations.rs`
- Design: this document (pinned revision)

## Open questions

- Should the default retention value be derived from a per-job class or be a
  single constant?
- Is the maintenance-window backfill acceptable, or must backfill be
  online-safe under concurrent writes?
