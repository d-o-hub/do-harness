# Design: retention_hours migration and online backfill (sound variant)

## Objective

Add a nullable `retention_hours` column to the `jobs` table and backfill
existing rows online without blocking writers.

## Proposed contract

- Migration 0017: `ALTER TABLE jobs ADD COLUMN retention_hours INTEGER NULL`.
- Backfill: run in bounded batches of 1000 rows until no NULL row remains:
  `UPDATE jobs SET retention_hours = ? WHERE retention_hours IS NULL AND id IN (SELECT id FROM jobs ORDER BY id LIMIT 1000)`.
- Rollback: additive only; the column is never dropped.

## Design decisions

### Backfill batch

The backfill daemon repeatedly claims the next 1000 rows whose
`retention_hours` is still NULL, sets the configured default, commits, and
stops when an `UPDATE` affects zero rows. The `WHERE retention_hours IS
NULL` guard makes the loop idempotent: already-set rows are never
reselectected, so the daemon terminates exactly when no NULL row remains.

### Writers

Each batch is a single autocommit boundary; between batches the table
remains writable. The select is bounded by `LIMIT`, so writer traffic is
never blocked for more than one batch.

### Idempotence

The guarded UPDATE is idempotent by construction: running it twice changes
nothing after the first completion.

### Rollback

Rollback is additive: `retention_hours` remains nullable and backfilled
values are preserved. There is no destructive step. A re-run of the
migration is a no-op, and a re-run of the backfill only touches still-NULL
rows.

## Open questions

- Should the backfill claim batches with an advisory lock or a lease column?
  (Decision: a lease column is out of scope; advisory lock at the daemon
  level is sufficient for a single daemon.)

## Files

- `backfill.py` — the frozen pre-implementation source snapshot.
