//! Workstream-scope vocabulary shared by beats and error signatures.
//!
//! A scope is the workstream key both tables partition on: `task:<id>` for a
//! task, `branch:<name>` for a branch, or `global` for unscoped beats. The
//! key strings are the stored wire format ([`BeatScope::key`],
//! [`std::fmt::Display`], and serde all emit them), so parsing is the single
//! choke point where a malformed key fails closed instead of silently
//! widening a lookup to another workstream's beats.

use serde::{Deserialize, Serialize, de, ser};
use std::str::FromStr;
use thiserror::Error;

/// Wire key for unscoped beats.
pub const GLOBAL_SCOPE_KEY: &str = "global";

/// Prefix of a task scope key (`task:<id>`).
const TASK_SCOPE_PREFIX: &str = "task:";

/// Prefix of a branch scope key (`branch:<name>`).
const BRANCH_SCOPE_PREFIX: &str = "branch:";

/// Shapes [`BeatScope::from_str`] accepts.
const EXPECTED_SCOPE_SHAPES: &str = "task:<id>|branch:<name>|global";

/// Error returned when a scope key is not one of the three stored shapes.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unknown beat scope '{value}': expected {EXPECTED_SCOPE_SHAPES}")]
pub struct BeatScopeParseError {
    /// The unrecognized key.
    pub value: String,
}

/// Error returned when a beat's `task_id` column disagrees with its scope key.
///
/// The two gate lookups read different columns — the advance gate filters on
/// `task_id` while reuse and strikes filter on `scope` — so a mismatched pair
/// would satisfy one lookup and not the other. Writers must pick one
/// workstream, not two.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error(
    "beat scope '{scope}' disagrees with task id {task_id:?}: a task scope requires the matching id, branch/global scopes require none"
)]
pub struct BeatScopeMismatch {
    /// The scope key that was supplied.
    pub scope: String,
    /// The `task_id` column that was supplied alongside it.
    pub task_id: Option<i64>,
}

/// Workstream scope for persisted beats and error signatures.
///
/// ```
/// # use std::str::FromStr;
/// # use do_harness_types::BeatScope;
/// let scope = BeatScope::from_str("task:7").expect("wire key");
/// assert_eq!(scope, BeatScope::Task(7));
/// assert_eq!(scope.key(), "task:7");
/// assert_eq!(scope.task_id(), Some(7));
/// assert!(BeatScope::from_str("team:7").is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeatScope {
    /// Explicit task id (`task:<id>`).
    Task(i64),
    /// Branch name (`branch:<name>`).
    Branch(String),
    /// Unscoped beats.
    Global,
}

impl BeatScope {
    /// Stored key for this scope (`task:<id>`, `branch:<name>`, `global`).
    #[must_use]
    pub fn key(&self) -> String {
        match self {
            Self::Task(id) => format!("{TASK_SCOPE_PREFIX}{id}"),
            Self::Branch(name) => format!("{BRANCH_SCOPE_PREFIX}{name}"),
            Self::Global => GLOBAL_SCOPE_KEY.to_owned(),
        }
    }

    /// Task id encoded by this scope, if it is a task scope.
    #[must_use]
    pub const fn task_id(&self) -> Option<i64> {
        match self {
            Self::Task(id) => Some(*id),
            Self::Branch(_) | Self::Global => None,
        }
    }

    /// Derives the scope for a beat from its `task_id` column when no
    /// explicit scope was supplied: a task beat belongs to `task:<id>`,
    /// anything else is unscoped.
    #[must_use]
    pub const fn scope_for_task(task_id: Option<i64>) -> Self {
        match task_id {
            Some(id) => Self::Task(id),
            None => Self::Global,
        }
    }

    /// Rejects a `task_id` column that disagrees with an explicit scope, so
    /// the `task_id` and `scope` lookups can never disagree.
    ///
    /// # Errors
    ///
    /// Returns [`BeatScopeMismatch`] when a task scope names a different id
    /// than the column carries, or when a branch/global scope carries any id.
    pub fn check_consistency(&self, task_id: Option<i64>) -> Result<(), BeatScopeMismatch> {
        let matches = match self {
            Self::Task(id) => task_id == Some(*id),
            Self::Branch(_) | Self::Global => task_id.is_none(),
        };
        if matches {
            Ok(())
        } else {
            Err(BeatScopeMismatch {
                scope: self.key(),
                task_id,
            })
        }
    }
}

impl std::fmt::Display for BeatScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.key())
    }
}

impl FromStr for BeatScope {
    type Err = BeatScopeParseError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        if raw == GLOBAL_SCOPE_KEY {
            return Ok(Self::Global);
        }
        if let Some(id) = raw.strip_prefix(TASK_SCOPE_PREFIX) {
            if let Ok(task_id) = id.parse::<i64>() {
                return Ok(Self::Task(task_id));
            }
        }
        if let Some(name) = raw.strip_prefix(BRANCH_SCOPE_PREFIX) {
            if !name.is_empty() {
                return Ok(Self::Branch(name.to_owned()));
            }
        }
        Err(BeatScopeParseError {
            value: raw.to_owned(),
        })
    }
}

impl Serialize for BeatScope {
    /// Serializes as the stored key string, keeping the JSON wire format
    /// identical to the former free-form scope column.
    fn serialize<S: ser::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.key())
    }
}

impl<'de> Deserialize<'de> for BeatScope {
    /// Parses the stored key string, failing closed on unknown shapes.
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::from_str(&raw).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// Task id used by the round-trip and consistency cases.
    const TASK_ID: i64 = 7;

    /// Task ids used by the mismatch cases.
    const SAMPLE_TASK_ID: i64 = 3;
    const OTHER_TASK_ID: i64 = 4;

    /// Every variant round-trips through its key, `Display`, `FromStr`, and
    /// serde without changing the stored strings.
    #[test]
    fn scope_keys_roundtrip_through_every_boundary() {
        for scope in [
            BeatScope::Task(TASK_ID),
            BeatScope::Branch("feat/x".to_owned()),
            BeatScope::Global,
        ] {
            assert_eq!(BeatScope::from_str(&scope.key()).expect("key"), scope);
            assert_eq!(scope.to_string(), scope.key());
            assert_eq!(
                scope.task_id(),
                BeatScope::from_str(&scope.key()).expect("key").task_id()
            );
            let json = serde_json::to_string(&scope).expect("serialize");
            assert_eq!(json, format!("\"{}\"", scope.key()));
            let parsed: BeatScope = serde_json::from_str(&json).expect("parse");
            assert_eq!(parsed, scope);
        }
        assert_eq!(BeatScope::Task(TASK_ID).key(), "task:7");
        assert_eq!(BeatScope::Global.key(), "global");
    }

    /// Malformed keys fail closed at every boundary, never widening to
    /// another workstream.
    #[test]
    fn malformed_scope_keys_are_rejected() {
        for raw in [
            "", "team:7", "task:", "task:abc", "branch:", "TASK:7", "Global",
        ] {
            assert!(BeatScope::from_str(raw).is_err(), "{raw} must not parse");
            assert!(serde_json::from_str::<BeatScope>(&format!("\"{raw}\"")).is_err());
        }
        let err = BeatScope::from_str("team:7").expect_err("unknown prefix");
        assert_eq!(err.value, "team:7");
        assert!(err.to_string().contains(EXPECTED_SCOPE_SHAPES));
    }

    /// A missing scope still deserializes as unscoped, preserving the old
    /// `default = "global"` row shape.
    #[test]
    fn missing_scope_deserializes_as_global() {
        #[derive(Deserialize)]
        struct Row {
            #[serde(default = "default_scope")]
            scope: BeatScope,
        }
        fn default_scope() -> BeatScope {
            BeatScope::Global
        }
        let row: Row = serde_json::from_str("{}").expect("missing scope");
        assert_eq!(row.scope, BeatScope::Global);
    }

    /// The derived scope agrees with an explicit one, and mismatched
    /// `task_id`/`scope` pairs are rejected before they can split the two
    /// gate lookups.
    #[test]
    fn scope_consistency_pins_task_id_to_scope() {
        assert_eq!(
            BeatScope::scope_for_task(Some(SAMPLE_TASK_ID)),
            BeatScope::Task(SAMPLE_TASK_ID)
        );
        assert_eq!(BeatScope::scope_for_task(None), BeatScope::Global);

        assert!(
            BeatScope::Task(SAMPLE_TASK_ID)
                .check_consistency(Some(SAMPLE_TASK_ID))
                .is_ok()
        );
        assert!(BeatScope::Global.check_consistency(None).is_ok());
        assert!(
            BeatScope::Branch("main".to_owned())
                .check_consistency(None)
                .is_ok()
        );
        assert!(
            BeatScope::Task(SAMPLE_TASK_ID)
                .check_consistency(Some(OTHER_TASK_ID))
                .is_err()
        );
        assert!(
            BeatScope::Task(SAMPLE_TASK_ID)
                .check_consistency(None)
                .is_err()
        );
        assert!(
            BeatScope::Global
                .check_consistency(Some(SAMPLE_TASK_ID))
                .is_err()
        );
        assert!(
            BeatScope::Branch("main".to_owned())
                .check_consistency(Some(SAMPLE_TASK_ID))
                .is_err()
        );
    }
}
