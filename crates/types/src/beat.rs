//! Read model for the `beats` table: execution heartbeat/step tracking.
//!
//! `status` and `beat_type` are a closed vocabulary, not free-form labels:
//! gates compare the enums, so a typo fails at compile time instead of
//! silently failing a gate at runtime. The serde and [`std::fmt::Display`]
//! spellings are the historical column strings, so stored rows and JSON are
//! unchanged.

use serde::{Deserialize, Serialize};
use std::str::FromStr;
use thiserror::Error;

use crate::scope::BeatScope;

/// Wire names [`BeatStatus::from_str`] accepts.
const EXPECTED_BEAT_STATUSES: &str = "ok|failed|warn";

/// Wire names [`BeatType::from_str`] accepts.
const EXPECTED_BEAT_TYPES: &str = "sensor|step";

/// Error returned when a stored or supplied beat status is not one of the
/// three gate outcomes.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unknown beat status '{value}': expected {EXPECTED_BEAT_STATUSES}")]
pub struct BeatStatusParseError {
    /// The unrecognized value.
    pub value: String,
}

/// Error returned when a stored or supplied beat type is not a known kind.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unknown beat type '{value}': expected {EXPECTED_BEAT_TYPES}")]
pub struct BeatTypeParseError {
    /// The unrecognized value.
    pub value: String,
}

/// Outcome of a beat: the only three labels gates understand.
///
/// ```
/// # use std::str::FromStr;
/// # use do_harness_types::BeatStatus;
/// let status = BeatStatus::from_str("ok").expect("wire name");
/// assert_eq!(status, BeatStatus::Ok);
/// assert_eq!(status.as_str(), "ok");
/// assert!(status.is_ok());
/// assert!(BeatStatus::from_str("passed").is_err());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BeatStatus {
    /// The sensor passed; gates treat this as satisfied.
    Ok,
    /// The sensor failed; gates treat this as unsatisfied.
    Failed,
    /// The sensor failed but is advisory; recorded, never gating.
    Warn,
}

impl BeatStatus {
    /// Stored wire name shared by the database, gates, and reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Failed => "failed",
            Self::Warn => "warn",
        }
    }

    /// Whether this status satisfies a sensor gate.
    #[must_use]
    pub const fn is_ok(self) -> bool {
        matches!(self, Self::Ok)
    }

    /// Maps a sensor execution onto its recorded status: a pass records
    /// `ok`, an advisory failure records `warn`, any other failure `failed`.
    #[must_use]
    pub const fn for_outcome(ok: bool, allow_failure: bool) -> Self {
        if ok {
            Self::Ok
        } else if allow_failure {
            Self::Warn
        } else {
            Self::Failed
        }
    }
}

impl std::fmt::Display for BeatStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for BeatStatus {
    type Err = BeatStatusParseError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "ok" => Ok(Self::Ok),
            "failed" => Ok(Self::Failed),
            "warn" => Ok(Self::Warn),
            _ => Err(BeatStatusParseError {
                value: raw.to_owned(),
            }),
        }
    }
}

/// Kind of a beat: sensor runs gate tasks, steps do not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BeatType {
    /// A sensor execution; the only kind gates and metrics read.
    Sensor,
    /// A manual or lifecycle step; invisible to gates and metrics.
    Step,
}

impl BeatType {
    /// Stored wire name shared by the database, gates, and reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sensor => "sensor",
            Self::Step => "step",
        }
    }
}

impl std::fmt::Display for BeatType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for BeatType {
    type Err = BeatTypeParseError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "sensor" => Ok(Self::Sensor),
            "step" => Ok(Self::Step),
            _ => Err(BeatTypeParseError {
                value: raw.to_owned(),
            }),
        }
    }
}

/// A single execution beat (heartbeat or step) recorded for a task.
///
/// Mirrors the `beats` table; `sensor_name` names the sensor that produced a
/// sensor beat so advance gates can check the specific sensor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Beat {
    /// Primary key.
    pub id: i64,
    /// Owning task id, when the beat belongs to a task.
    pub task_id: Option<i64>,
    /// Workstream scope: `task:<id>`, `branch:<name>`, or `global`.
    #[serde(default = "default_scope")]
    pub scope: BeatScope,
    /// Beat kind.
    pub beat_type: BeatType,
    /// Outcome label.
    pub status: BeatStatus,
    /// Exit code of the sensor that produced this beat.
    pub sensor_exit_code: Option<i32>,
    /// Name of the sensor that produced this beat, when it is a sensor beat.
    pub sensor_name: Option<String>,
    /// Unix timestamp when the beat started.
    pub started_at: i64,
    /// Unix timestamp when the beat completed, when known.
    pub completed_at: Option<i64>,
}

impl Beat {
    /// Whether this beat is a run of `sensor` (any outcome).
    #[must_use]
    pub fn is_sensor_beat_for(&self, sensor: &str) -> bool {
        self.beat_type == BeatType::Sensor && self.sensor_name.as_deref() == Some(sensor)
    }

    /// Whether this beat carries a passing outcome.
    #[must_use]
    pub const fn is_ok(&self) -> bool {
        self.status.is_ok()
    }
}

fn default_scope() -> BeatScope {
    BeatScope::Global
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// The serde, `Display`, and `FromStr` spellings are one vocabulary equal
    /// to the historical column strings; anything else fails closed.
    #[test]
    fn status_and_type_names_match_the_stored_strings() {
        for (status, wire) in [
            (BeatStatus::Ok, "ok"),
            (BeatStatus::Failed, "failed"),
            (BeatStatus::Warn, "warn"),
        ] {
            assert_eq!(status.as_str(), wire);
            assert_eq!(status.to_string(), wire);
            assert_eq!(wire.parse::<BeatStatus>().expect("from_str"), status);
            let json = serde_json::to_string(&status).expect("serialize");
            assert_eq!(json, format!("\"{wire}\""));
            assert_eq!(
                serde_json::from_str::<BeatStatus>(&json).expect("parse"),
                status
            );
        }
        for (beat_type, wire) in [(BeatType::Sensor, "sensor"), (BeatType::Step, "step")] {
            assert_eq!(beat_type.as_str(), wire);
            assert_eq!(beat_type.to_string(), wire);
            assert_eq!(wire.parse::<BeatType>().expect("from_str"), beat_type);
            let json = serde_json::to_string(&beat_type).expect("serialize");
            assert_eq!(json, format!("\"{wire}\""));
            assert_eq!(
                serde_json::from_str::<BeatType>(&json).expect("parse"),
                beat_type
            );
        }
        assert!("passed".parse::<BeatStatus>().is_err());
        assert!("manual".parse::<BeatType>().is_err());
        assert!(serde_json::from_str::<BeatStatus>("\"passed\"").is_err());
        assert!(serde_json::from_str::<BeatType>("\"manual\"").is_err());
    }

    /// The outcome mapping pins the recorder's contract: only a pass is `ok`,
    /// and only an advisory failure is `warn`.
    #[test]
    fn outcome_mapping_records_warn_only_for_advisory_failures() {
        assert_eq!(BeatStatus::for_outcome(true, false), BeatStatus::Ok);
        assert_eq!(BeatStatus::for_outcome(true, true), BeatStatus::Ok);
        assert_eq!(BeatStatus::for_outcome(false, true), BeatStatus::Warn);
        assert_eq!(BeatStatus::for_outcome(false, false), BeatStatus::Failed);
    }

    /// Gate predicates match the sensor name and the passing outcome, so a
    /// pass for another sensor — or a non-sensor beat — never satisfies a gate.
    #[test]
    fn beat_gate_predicates_require_the_named_passing_sensor() {
        let beat = Beat {
            id: 1,
            task_id: Some(2),
            scope: BeatScope::Task(2),
            beat_type: BeatType::Sensor,
            status: BeatStatus::Ok,
            sensor_exit_code: Some(0),
            sensor_name: Some("check".to_owned()),
            started_at: 1,
            completed_at: Some(2),
        };
        assert!(beat.is_sensor_beat_for("check"));
        assert!(!beat.is_sensor_beat_for("fmt"));
        assert!(beat.is_ok());
        let failed = Beat {
            status: BeatStatus::Failed,
            ..beat.clone()
        };
        assert!(failed.is_sensor_beat_for("check"));
        assert!(!failed.is_ok());
        let step = Beat {
            beat_type: BeatType::Step,
            sensor_name: Some("check".to_owned()),
            ..beat.clone()
        };
        assert!(!step.is_sensor_beat_for("check"));
    }

    /// The struct keeps its shape across the boundary: unknown fields are
    /// rejected and a missing scope still reads as unscoped.
    #[test]
    fn beat_serde_shape_pins_scope_and_rejects_unknown_fields() {
        let beat = Beat {
            id: 1,
            task_id: None,
            scope: BeatScope::Global,
            beat_type: BeatType::Sensor,
            status: BeatStatus::Ok,
            sensor_exit_code: Some(0),
            sensor_name: Some("check".to_owned()),
            started_at: 1,
            completed_at: Some(1),
        };
        let json = serde_json::to_string(&beat).expect("serialize");
        assert!(json.contains("\"scope\":\"global\""));
        assert!(json.contains("\"status\":\"ok\""));
        assert!(json.contains("\"beat_type\":\"sensor\""));
        let parsed: Beat = serde_json::from_str(&json).expect("parse");
        assert_eq!(parsed, beat);

        let missing_scope = json.replace("\"scope\":\"global\",", "");
        let parsed: Beat = serde_json::from_str(&missing_scope).expect("default scope");
        assert_eq!(parsed.scope, BeatScope::Global);

        let extra = json.replace('}', ",\"extra\":1}");
        assert!(serde_json::from_str::<Beat>(&extra).is_err());
    }
}
