//! Connection, schema, and repository management for the do-harness local
//! libSQL store.
//!
//! Module layout:
//! - [`root`] — workspace root discovery
//! - [`migrate`] — connection and embedded migrations
//! - [`repo`] — execution tables (tasks, invariants)
//! - [`repo_exec`] — beats and error signatures
//! - [`repo_scope`] — signature lifecycle (reset, list, clear)
//! - [`repo_learn`] — learning tables (traces, heuristics, skill evals)
//! - [`repo_eval`] — eval-run history, skill bars, grader baselines
//! - [`repo_workflow`] — append-only workflow events + transactional commands

#![forbid(unsafe_code)]

pub mod error;
pub mod migrate;
pub mod repo;
pub mod repo_eval;
pub mod repo_exec;
pub mod repo_learn;
pub mod repo_metrics;
pub mod repo_scope;
pub mod repo_workflow;
pub mod root;

pub use error::{DbError, Result};

/// Re-exported so downstream crates can name connection types without
/// depending on `libsql` directly.
pub use libsql::Connection;
pub use migrate::{connect, connect_and_migrate, migrate, unix_now};
pub use repo::{
    NewTask, advance_subtask, get_task, insert_task, list_tasks, seed_invariants,
    update_task_status,
};
pub use repo_eval::{
    NewSkillEvalRun, bless_grader_baseline, get_grader_baseline, get_skill_bar,
    insert_skill_eval_run, list_skill_eval_runs, max_pass_rate, raise_skill_bar,
};
pub use repo_exec::{
    NewBeat, bump_error_signature, get_error_signature, insert_beat, list_beats,
    record_sensor_outcome,
};
pub use repo_learn::{
    NewHeuristic, NewSkillEval, NewTrace, get_trace, insert_heuristic, insert_skill_eval,
    insert_trace, list_all_skill_evals, list_heuristics, list_skill_evals, list_traces,
};
pub use repo_metrics::{SensorStat, has_ok_beat, sensor_stats};
pub use repo_scope::{clear_error_signatures, list_error_signatures, reset_error_signature};
pub use repo_workflow::{
    advance_subtask_with_event, insert_task_with_event, list_all_events,
    update_task_status_with_event,
};
pub use root::{db_path, find_harness_root};
