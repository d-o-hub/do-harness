//! Schema contracts and executable invariants for the do-harness.
//!
//! These types make invalid states unrepresentable: decision headers, HTN
//! planning structures, event-sourcing contracts, and persisted execution
//! records are enforced by the type system rather than by prose in
//! architecture documents.

#![forbid(unsafe_code)]

pub mod beat;
pub mod decision;
pub mod error_signature;
pub mod eval_run;
pub mod event;
pub mod heuristic;
pub mod htn;
pub mod skill_eval;
pub mod task;
pub mod trace;
pub mod workflow;

pub use beat::Beat;
pub use decision::DecisionHeader;
pub use error_signature::ErrorSignature;
pub use eval_run::{GraderBaseline, SkillEvalRun};
pub use event::{Command, DomainEvent, Projection};
pub use heuristic::Heuristic;
pub use htn::{Method, Precondition, Subtask, TaskState, TaskStateParseError};
pub use skill_eval::SkillEval;
pub use task::TaskRecord;
pub use trace::Trace;
pub use workflow::{
    AddTask, AdvanceTask, CompleteTask, FailTask, TaskAdded, TaskAdvanced, TaskBoard,
    TaskCompleted, TaskFailed, WorkflowEvent,
};
