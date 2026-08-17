//! Connection, schema, and repository management for the do-harness local
//! libSQL store.
//!
//! Module layout:
//! - [`root`] — workspace root discovery
//! - [`migrate`] — connection and embedded migrations
//! - [`repo`] — execution tables (tasks, beats, error signatures)
//! - [`repo_learn`] — learning tables (traces, heuristics, skill evals)

#![forbid(unsafe_code)]

pub mod migrate;
pub mod repo;
pub mod repo_exec;
pub mod repo_learn;
pub mod root;

pub use migrate::{connect, connect_and_migrate, migrate, unix_now};
pub use repo::{
    NewTask, advance_subtask, get_task, insert_task, list_tasks, seed_invariants,
    update_task_status,
};
pub use repo_exec::{NewBeat, bump_error_signature, get_error_signature, insert_beat, list_beats};
pub use repo_learn::{
    NewHeuristic, NewSkillEval, NewTrace, get_trace, insert_heuristic, insert_skill_eval,
    insert_trace, list_heuristics, list_skill_evals, list_traces,
};
pub use root::{db_path, find_harness_root};
