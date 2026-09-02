//! Embedded migration catalog for the agent-state store.

/// A single versioned schema migration.
pub struct Migration {
    /// Schema version for ordering.
    pub version: i64,
    /// Human-readable migration name.
    pub name: &'static str,
    /// SQL executed for this migration.
    pub sql: &'static str,
}

/// Embedded migration catalog, ordered by ascending version.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "init",
        sql: include_str!("../migrations/0001_init.sql"),
    },
    Migration {
        version: 2,
        name: "invariants",
        sql: include_str!("../migrations/0002_invariants.sql"),
    },
    Migration {
        version: 3,
        name: "persist",
        sql: include_str!("../migrations/0003_persist.sql"),
    },
    Migration {
        version: 4,
        name: "scope",
        sql: include_str!("../migrations/0004_scope.sql"),
    },
    Migration {
        version: 5,
        name: "beat_sensor",
        sql: include_str!("../migrations/0005_beat_sensor.sql"),
    },
    Migration {
        version: 6,
        name: "eval_latest",
        sql: include_str!("../migrations/0006_eval_latest.sql"),
    },
    Migration {
        version: 7,
        name: "fk_strike_index",
        sql: include_str!("../migrations/0007_fk_strike_index.sql"),
    },
    Migration {
        version: 8,
        name: "eval_history_and_baselines",
        sql: include_str!("../migrations/0008_eval_history_and_baselines.sql"),
    },
    Migration {
        version: 9,
        name: "workflow_events",
        sql: include_str!("../migrations/0009_workflow_events.sql"),
    },
    Migration {
        version: 10,
        name: "workflow_event_chain",
        sql: include_str!("../migrations/0010_workflow_event_chain.sql"),
    },
];
