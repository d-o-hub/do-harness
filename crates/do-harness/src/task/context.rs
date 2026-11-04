//! `do-harness task context`: a bounded, read-only resume projection.
//!
//! The projection is assembled from existing facts only: the task record, the
//! method catalog, task-scoped beats, task-scoped traces and error signatures,
//! the workspace diff, and skill frontmatter. It never runs a sensor, never
//! advances a task, and never writes the database or an evidence artifact.
//!
//! Every stored string (titles, trace text, signature messages) is untrusted
//! data: it is rendered as data and is never executed or interpolated into a
//! command. Commands in `next_action` are generated from the task id and method
//! policy, never from stored content.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::report::Format;

mod render;

/// Wire-schema version of the context projection.
pub const CONTEXT_SCHEMA_VERSION: u32 = 1;

/// Default `--max-bytes` when the caller does not bound the result.
pub const DEFAULT_MAX_BYTES: usize = 8192;

const MAX_FAILURES: usize = 10;
const MAX_TRACES: usize = 10;
const MAX_CHANGED_PATHS: usize = 20;
const MAX_SKILLS: usize = 5;
const MAX_FIELD_CHARS: usize = 240;

/// One bounded section plus what the budget dropped.
#[derive(Debug, Clone, Serialize)]
pub struct Bounded<T> {
    /// Retained items, in the section's stable order.
    pub items: Vec<T>,
    /// Items the byte budget or the per-section cap omitted.
    pub omitted: usize,
    /// Whether any item was omitted.
    pub truncated: bool,
}

impl<T> Bounded<T> {
    fn new(items: Vec<T>, total: usize) -> Self {
        let omitted = total.saturating_sub(items.len());
        Self {
            items,
            omitted,
            truncated: omitted > 0,
        }
    }

    fn shrink(&mut self) -> bool {
        if self.items.pop().is_some() {
            self.omitted += 1;
            self.truncated = true;
            true
        } else {
            false
        }
    }
}

/// Which command inputs the projection was built from.
#[derive(Debug, Clone, Serialize)]
pub struct Budget {
    /// `--max-bytes` the caller requested.
    pub max_bytes: usize,
}

/// State-database availability, reported without creating or migrating.
#[derive(Debug, Clone, Serialize)]
pub struct Database {
    /// `ok`, `missing`, `uninitialized`, `incompatible`, or `future`.
    pub state: &'static str,
    /// Fixed reason code.
    pub reason: &'static str,
    /// Repository-relative database path.
    pub path: String,
}

/// Task identity and lifecycle fields.
#[derive(Debug, Clone, Serialize)]
pub struct TaskIdentity {
    /// Primary key.
    pub id: i64,
    /// Stored title (untrusted data, rendered as text only).
    pub title: String,
    /// Lifecycle state.
    pub status: &'static str,
    /// Parent task id, when this is a subtask.
    pub parent_id: Option<i64>,
    /// HTN method name, when recorded.
    pub method: Option<String>,
    /// Current subtask index.
    pub subtask_index: i64,
    /// Recorded precondition guard.
    pub precondition: Option<String>,
}

/// The current method step, or why it cannot be resolved.
#[derive(Debug, Clone, Serialize)]
pub struct StepView {
    /// `current`, `closed`, `complete`, `no_method`, `method_missing`,
    /// `invalid_index`, or `policy_unreadable`.
    pub state: &'static str,
    /// Resolved method name.
    pub method: Option<String>,
    /// Stored subtask index.
    pub index: i64,
    /// Current subtask name, when in range.
    pub name: Option<String>,
    /// Sensor gating this step, when declared.
    pub sensor: Option<String>,
    /// Total subtasks in the method, when resolved.
    pub total_subtasks: Option<usize>,
    /// Subtasks after the current one.
    pub remaining_subtasks: Option<usize>,
    /// Fixed reason code.
    pub reason: &'static str,
}

/// One sensor's task-scoped freshness verdict.
#[derive(Debug, Clone, Serialize)]
pub struct SensorFreshness {
    /// Sensor name from the method policy.
    pub name: String,
    /// `current`, `red`, `stale`, or `missing`.
    pub state: &'static str,
    /// `current`, `failed`, `inputs_changed`, `no_beat`, `unknown_sensor`, or
    /// `inputs_unresolved`.
    pub reason: &'static str,
}

/// Gating criteria derived from the method and task-scoped evidence.
#[derive(Debug, Clone, Serialize)]
pub struct Gating {
    /// Sensors the next transition requires.
    pub required_sensors: Vec<String>,
    /// Per-sensor freshness, in method order.
    pub sensors: Vec<SensorFreshness>,
    /// Aggregate state: `green`, `red`, `stale`, `missing`,
    /// `not_applicable`, or `unavailable`.
    pub state: &'static str,
    /// Fixed aggregate reason code.
    pub reason: &'static str,
}

/// Product acceptance criteria, which `TaskRecord` does not carry.
#[derive(Debug, Clone, Serialize)]
pub struct AcceptanceCriteria {
    /// Always `false` until a typed source exists.
    pub available: bool,
    /// Fixed reason code.
    pub reason: &'static str,
    /// Human-readable explanation.
    pub note: &'static str,
}

/// Evidence summary with a recovery command when one applies.
#[derive(Debug, Clone, Serialize)]
pub struct Evidence {
    /// Same vocabulary as [`Gating::state`].
    pub state: &'static str,
    /// Same vocabulary as [`Gating::reason`].
    pub reason: &'static str,
    /// Re-record command when the remedy is a `verify` run.
    pub recovery: Option<String>,
}

/// One unresolved task-scoped failure signature.
#[derive(Debug, Clone, Serialize)]
pub struct FailureItem {
    /// Signature key (for example `sensor:test`).
    pub signature: String,
    /// Consecutive attempt count.
    pub attempt_count: i64,
    /// Last diagnostic message, clipped and control-character-free.
    pub message: Option<String>,
    /// Unix timestamp of first recording.
    pub created_at: i64,
}

/// One task-scoped trace.
#[derive(Debug, Clone, Serialize)]
pub struct TraceItem {
    /// Trace id.
    pub id: i64,
    /// Command that ran, clipped. Untrusted data; never executed.
    pub command: Option<String>,
    /// Captured failure output, clipped. Untrusted data.
    pub error_diff: Option<String>,
    /// Recorded resolution steps, clipped. Untrusted data.
    pub resolution_steps: Option<String>,
    /// Unix timestamp of creation.
    pub created_at: i64,
}

/// One changed workspace path.
#[derive(Debug, Clone, Serialize)]
pub struct ChangedPath {
    /// Repository-relative path.
    pub path: String,
    /// `added`, `modified`, `deleted`, `renamed`, or `untracked`.
    pub kind: &'static str,
}

/// Branch/workspace context, explicitly separated from task-scoped facts.
#[derive(Debug, Clone, Serialize)]
pub struct Workspace {
    /// Stored git revision, when available.
    pub git_revision: Option<String>,
    /// Current branch, when attached.
    pub branch: Option<String>,
    /// Always `workspace`: these paths are not attributed to the task.
    pub scope: &'static str,
    /// Bounded changed paths.
    pub changed: Bounded<ChangedPath>,
    /// True when git state could not be determined.
    pub discovery_failed: bool,
    /// Discovery diagnostic, when it failed.
    pub discovery_error: Option<String>,
}

/// One concrete next command derived from observable state.
#[derive(Debug, Clone, Serialize)]
pub struct NextAction {
    /// `verify`, `advance`, `done`, `none`, `configure_method`, `fix_method`,
    /// `fix_state`, `fix_policy`, or `unavailable`.
    pub kind: &'static str,
    /// Command to run, generated from the task id and policy.
    pub command: Option<String>,
    /// Fixed reason code.
    pub reason: &'static str,
}

/// One metadata-only skill candidate.
#[derive(Debug, Clone, Serialize)]
pub struct SkillItem {
    /// Skill name.
    pub name: String,
    /// Repository-relative `SKILL.md` path.
    pub path: String,
    /// Rank score.
    pub score: f64,
    /// Short description from frontmatter, when present.
    pub short_description: Option<String>,
}

/// A section that is unavailable, and why.
#[derive(Debug, Clone, Serialize)]
pub struct Unknown {
    /// Section name.
    pub section: &'static str,
    /// Fixed reason code.
    pub reason: &'static str,
}

/// The complete `task context --format json` document.
#[derive(Debug, Clone, Serialize)]
pub struct ContextDocument {
    /// Wire-schema version.
    pub schema_version: u32,
    /// Requested byte budget.
    pub budget: Budget,
    /// Database availability.
    pub database: Database,
    /// Task identity, absent when the database is unavailable.
    pub task: Option<TaskIdentity>,
    /// Current step, absent when the task cannot be read.
    pub step: Option<StepView>,
    /// Gating criteria, absent when the task cannot be read.
    pub gating: Option<Gating>,
    /// Product acceptance criteria (always unavailable today).
    pub acceptance_criteria: AcceptanceCriteria,
    /// Freshness summary.
    pub evidence: Evidence,
    /// Unresolved task-scoped failures.
    pub failures: Bounded<FailureItem>,
    /// Task-scoped traces, newest first.
    pub traces: Bounded<TraceItem>,
    /// Branch/workspace context.
    pub workspace: Workspace,
    /// One next command.
    pub next_action: NextAction,
    /// Metadata-only skill shortlist.
    pub skills: Bounded<SkillItem>,
    /// Paths to read next (skill entry points only).
    pub paths: Bounded<String>,
    /// Explicit unknowns.
    pub unknowns: Vec<Unknown>,
}

impl ContextDocument {
    /// Drops one item from the lowest-priority non-empty section.
    fn shrink_one(&mut self) -> bool {
        self.skills.shrink()
            || self.paths.shrink()
            || self.traces.shrink()
            || self.workspace.changed.shrink()
            || self.failures.shrink()
    }
}

/// Runs `task context` and prints one bounded result.
///
/// # Errors
///
/// Returns an error when the task id does not exist, when the result cannot be
/// serialized, or after printing a diagnostic for a missing or incompatible
/// state database.
pub async fn run(root: &Path, id: i64, max_bytes: usize, format: Format) -> Result<()> {
    let mut document = build(root, id, max_bytes).await?;
    let rendered = fit(&mut document, max_bytes, format)?;
    println!("{rendered}");
    if document.database.state == "ok" {
        Ok(())
    } else {
        anyhow::bail!(
            "task context unavailable: {} ({})",
            document.database.state,
            document.database.reason
        )
    }
}

/// Builds the projection without rendering or mutating anything.
///
/// # Errors
///
/// Returns an error when the task id does not exist or a read fails.
pub(crate) async fn build(root: &Path, id: i64, max_bytes: usize) -> Result<ContextDocument> {
    let database = inspect_database(root).await;
    if database.state != "ok" {
        return unavailable(root, database, max_bytes).await;
    }
    let conn = do_harness_db::connect_read_only(do_harness_db::db_path(root)).await?;
    let task = do_harness_db::get_task(&conn, id)
        .await?
        .with_context(|| format!("task {id} not found"))?;
    assemble(root, &conn, &database, &task, id, max_bytes).await
}

/// Renders, then shrinks the lowest-priority section until the result fits.
///
/// The shrink order is fixed, so identical inputs and limits produce identical
/// output. Identity, blocking reasons, and the next action are never dropped;
/// a budget smaller than that floor is exceeded rather than silently losing
/// them.
///
/// # Errors
///
/// Returns an error when rendering fails.
pub(crate) fn fit(
    document: &mut ContextDocument,
    max_bytes: usize,
    format: Format,
) -> Result<String> {
    let mut rendered = render::document(document, format)?;
    while rendered.len() > max_bytes && document.shrink_one() {
        rendered = render::document(document, format)?;
    }
    Ok(rendered)
}

mod actions;
mod assemble;
mod db;
mod workspace;

use assemble::assemble;
use db::{inspect_database, unavailable};

#[cfg(test)]
mod tests;
