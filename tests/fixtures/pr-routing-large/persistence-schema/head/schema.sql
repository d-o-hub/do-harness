CREATE TABLE account (
    id INTEGER PRIMARY KEY,
    email TEXT NOT NULL,
    slug TEXT NOT NULL DEFAULT (hex(id)),
    created_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE session (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL REFERENCES account(id),
    expires_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_01 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_02 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_03 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_04 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_05 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_06 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_07 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_08 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_09 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_10 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_11 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_12 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_13 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_14 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_15 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_16 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_17 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_18 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_19 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_20 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_21 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_22 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_23 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_24 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_25 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_26 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_27 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_28 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_29 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_30 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_31 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_32 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_33 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_34 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_35 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_36 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_37 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_38 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_39 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE audit_40 (
    id INTEGER PRIMARY KEY,
    account_id INTEGER NOT NULL,
    payload TEXT NOT NULL,
    recorded_at INTEGER NOT NULL DEFAULT 0
);

