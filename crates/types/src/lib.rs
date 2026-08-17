//! Schema contracts and executable invariants for the do-harness.
//!
//! These types make invalid states unrepresentable: decision headers, HTN
//! planning structures, and event-sourcing contracts are enforced by the type
//! system rather than by prose in architecture documents.

#![forbid(unsafe_code)]

pub mod decision;
pub mod event;
pub mod htn;

pub use decision::DecisionHeader;
pub use event::{Command, DomainEvent, Projection};
pub use htn::{Method, Precondition, Subtask, TaskState};
