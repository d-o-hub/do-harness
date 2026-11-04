//! State-database availability checks for the context projection.

use std::path::Path;

use anyhow::Result;

use super::workspace::{ranked_skills, workspace_view};
use super::{
    AcceptanceCriteria, Bounded, Budget, CONTEXT_SCHEMA_VERSION, ContextDocument, Database,
    Evidence, NextAction, Unknown,
};

/// Classifies the state database without creating or migrating it.
pub(super) async fn inspect_database(root: &Path) -> Database {
    let path = do_harness_db::db_path(root);
    let relative = path
        .strip_prefix(root)
        .map_or_else(|_| path.display().to_string(), |p| p.display().to_string());
    if !path.exists() {
        return Database {
            state: "missing",
            reason: "database_missing",
            path: relative,
        };
    }
    let Ok(conn) = do_harness_db::connect_read_only(&path).await else {
        return Database {
            state: "incompatible",
            reason: "database_unreadable",
            path: relative,
        };
    };
    let Ok(skew) = do_harness_db::inspect_migrations(&conn).await else {
        return Database {
            state: "incompatible",
            reason: "database_unreadable",
            path: relative,
        };
    };
    if skew.legacy {
        return Database {
            state: "incompatible",
            reason: "database_legacy",
            path: relative,
        };
    }
    if skew.is_future() {
        return Database {
            state: "future",
            reason: "database_newer_than_binary",
            path: relative,
        };
    }
    if skew.applied_max.is_none() {
        return Database {
            state: "uninitialized",
            reason: "database_uninitialized",
            path: relative,
        };
    }
    Database {
        state: "ok",
        reason: "current",
        path: relative,
    }
}

/// Builds a document when no task can be read.
pub(super) async fn unavailable(
    root: &Path,
    database: Database,
    max_bytes: usize,
) -> Result<ContextDocument> {
    let reason = database.reason;
    let mut unknowns = vec![Unknown {
        section: "task",
        reason,
    }];
    unknowns.push(Unknown {
        section: "acceptance_criteria",
        reason: "not_recorded",
    });
    let (skills, paths, catalog_error) = ranked_skills(root, "");
    if catalog_error.is_some() {
        unknowns.push(Unknown {
            section: "skills",
            reason: "catalog_unreadable",
        });
    }
    let workspace = workspace_view(root);
    if workspace.discovery_failed {
        unknowns.push(Unknown {
            section: "workspace",
            reason: "discovery_failed",
        });
    }
    Ok(ContextDocument {
        schema_version: CONTEXT_SCHEMA_VERSION,
        budget: Budget { max_bytes },
        database,
        task: None,
        step: None,
        gating: None,
        acceptance_criteria: AcceptanceCriteria {
            available: false,
            reason: "not_recorded",
            note: "TaskRecord has no acceptance-criteria field; none are inferred from the title or a passing sensor.",
        },
        evidence: Evidence {
            state: "unavailable",
            reason,
            recovery: None,
        },
        failures: Bounded::new(Vec::new(), 0),
        traces: Bounded::new(Vec::new(), 0),
        workspace,
        next_action: NextAction {
            kind: "unavailable",
            command: None,
            reason,
        },
        skills,
        paths,
        unknowns,
    })
}
