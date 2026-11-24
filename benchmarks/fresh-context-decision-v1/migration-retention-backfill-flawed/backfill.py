"""Frozen pre-implementation snapshot for the flawed backfill design."""

import sqlite3

BATCH_SIZE = 1000
DEFAULT_RETENTION_HOURS = 168


def migrate_add_column(conn):
    conn.execute(
        "ALTER TABLE jobs ADD COLUMN retention_hours INTEGER NULL"
    )
    conn.commit()


def backfill(conn):
    while True:
        cursor = conn.execute(
            "UPDATE jobs SET retention_hours = ? "
            "WHERE id IN (SELECT id FROM jobs ORDER BY id LIMIT ?)",
            (DEFAULT_RETENTION_HOURS, BATCH_SIZE),
        )
        conn.commit()
        if cursor.rowcount == 0:
            break


def rollback(conn):
    conn.execute("ALTER TABLE jobs DROP COLUMN retention_hours")
    conn.commit()
