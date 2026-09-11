//! Connection, schema, and repository management for the do-harness local
//! libSQL store.
//!
//! Module layout:
//! - [`root`] — workspace root discovery
//! - [`migrate`] — connection and embedded migrations
//! - [`repo`] — execution tables (tasks, invariants)
//! - [`repo_exec`] — beats and error signatures
//! - [`repo_scope`] — signature lifecycle (reset, list, clear)
//! - [`repo_trace`] — execution traces
//! - [`repo_heuristic`] — distilled heuristics
//! - [`repo_skill_eval`] — latest skill evaluations
//! - [`repo_eval`] — eval-run history, skill bars, grader baselines
//! - [`repo_workflow`] — append-only workflow events + transactional commands
//! - [`query`] — generic parameterized read-only helpers

#![forbid(unsafe_code)]

pub mod error;
pub mod migrate;
pub mod migrate_catalog;
pub mod query;
pub mod repo;
pub mod repo_eval;
pub mod repo_exec;
pub mod repo_heuristic;
pub mod repo_metrics;
pub mod repo_scope;
pub mod repo_skill_eval;
pub mod repo_trace;
pub mod repo_workflow;
pub mod root;

pub use error::{DbError, Result, retry_on_busy};

/// Re-exported so downstream crates can name connection types without
/// depending on `libsql` directly.
pub use libsql::Connection;
pub use migrate::{
    MigrationSkew, connect, connect_and_migrate, count_migrations, inspect_migrations, migrate,
    unix_now,
};
pub use query::count_where;
pub use repo::{NewTask, get_task, list_tasks, seed_invariants};
pub use repo_eval::{
    NewSkillEvalRun, bless_grader_baseline, get_grader_baseline, get_skill_bar,
    insert_skill_eval_run, list_skill_eval_runs, max_pass_rate, raise_skill_bar,
};
pub use repo_exec::{
    NewBeat, SensorOutcome, bump_error_signature, get_error_signature, list_beats,
    record_sensor_outcome, record_verify_batch,
};
pub use repo_heuristic::{NewHeuristic, insert_heuristic, list_heuristics};
pub use repo_metrics::{SensorStat, has_ok_beat, sensor_stats};
pub use repo_scope::{clear_error_signatures, list_error_signatures, reset_error_signature};
pub use repo_skill_eval::{
    NewSkillEval, insert_skill_eval, list_all_skill_evals, list_skill_evals,
};
pub use repo_trace::{NewTrace, get_trace, insert_trace, list_traces};
pub use repo_workflow::{
    WorkflowEventRow, advance_subtask_with_event, canonical_payload, chain_hash,
    count_tasks_without_added_event, insert_task_with_event, list_all_events,
    list_events_ascending, update_task_status_with_event,
};
pub use root::{db_path, find_harness_root};
