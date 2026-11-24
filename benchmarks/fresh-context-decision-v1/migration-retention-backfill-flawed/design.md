# Design: retention_hours migration and online backfill (flawed variant)

## Objective

Add a nullable `retention_hours` column to the `jobs` table and backfill
existing rows online without blocking writers.

## Proposed contract

- Migration 0017: `ALTER TABLE jobs ADD COLUMN retention_hours INTEGER NULL`.
- Backfill: run in bounded batches of 1000 rows until no row remains:
  `UPDATE jobs SET retention_hours = ? WHERE id IN (SELECT id FROM jobs ORDER BY id LIMIT 1000)`.
- Rollback: `ALTER TABLE jobs DROP COLUMN retention_hours`.

## Design decisions

### Backfill batch

The backfill daemon repeatedly picks the next 1000 rows by `id` and sets
`retention_hours` to the configured default (e.g. 168). The batch runs in a
single transaction. The daemon stops when an `UPDATE` affects zero rows.

### Writers

Backfill transactions use the default isolation level. No locking strategy
beyond the transaction itself is used; the table remains writable between
batches.

### Idempotence

An `UPDATE` that sets the same value twice is considered harmless; no
`WHERE retention_hours IS NULL` guard is used because the column starts NULL
and the daemon is the only writer.

### Rollback

Rollback drops the column, removing both the migration and all backfilled
values in one step.

## Open questions

- Should backfill process one batch per transaction or one transaction per row?
- Is a maintenance-window backfill acceptable, or must it be online-safe?

## Files

- `backfill.py` — the frozen pre-implementation source snapshot.
