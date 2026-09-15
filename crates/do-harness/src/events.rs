//! Metrics-event emission for non-trivial harness work.
//!
//! `AGENTS.md` §6 asks for a JSON metrics event per non-trivial task, but the
//! event tree (`.agents/events/YYYY/MM/DD/`) is gitignored, so an event on its
//! own is not durable evidence. Every writer therefore records which *tracked*
//! artifact the learning landed in ([`GuideRef`]) alongside the event, making
//! the path trace -> guide followable even though the event file itself is
//! local state.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

/// A tracked artifact an event's learning was written into.
///
/// Paths are repository-relative and must point at committed content (a
/// skill's `SKILL.md`/`references/`, `docs/`, or `plans/`), never at the
/// gitignored event tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuideRef {
    /// What kind of guide was updated.
    pub kind: GuideKind,
    /// Repository-relative path of the updated guide.
    pub path: String,
}

/// The tracked-guide categories an event may cite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuideKind {
    /// A skill's `SKILL.md`.
    Skill,
    /// A skill's `references/*.md`.
    Reference,
    /// Repository documentation under `docs/`.
    Doc,
    /// A roadmap or epic under `plans/`.
    Plan,
}

impl GuideKind {
    /// Stable wire name used in the event payload.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            GuideKind::Skill => "skill",
            GuideKind::Reference => "reference",
            GuideKind::Doc => "doc",
            GuideKind::Plan => "plan",
        }
    }
}

impl GuideRef {
    /// Builds a guide reference for a repository-relative path.
    ///
    /// The kind is derived from the path so a caller cannot mislabel its
    /// target. Use [`GuideRef::is_tracked`] to confirm the path is durable.
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        let path = path.into();
        let kind = classify(&path);
        GuideRef { kind, path }
    }

    /// Builds a `SKILL.md` reference for `skill`.
    #[must_use]
    pub fn skill(skill: &str) -> Self {
        GuideRef::new(format!(".agents/skills/{skill}/SKILL.md"))
    }

    /// Builds a `references/<file>` reference for `skill`.
    #[must_use]
    pub fn reference(skill: &str, file: &str) -> Self {
        GuideRef::new(format!(".agents/skills/{skill}/references/{file}"))
    }

    /// Whether the referenced guide is tracked by git.
    ///
    /// The event tree and the local state directory are gitignored, so a guide
    /// pointing back into them would re-create the untracked-capture problem
    /// this module exists to remove.
    #[must_use]
    pub fn is_tracked(&self) -> bool {
        let path = self.path.as_str();
        !path.starts_with(".agents/events/") && !path.starts_with(".do-harness/")
    }
}

/// Derives the guide category from a repository-relative path.
fn classify(path: &str) -> GuideKind {
    if path.starts_with(".agents/skills/") && path.ends_with("SKILL.md") {
        GuideKind::Skill
    } else if path.starts_with(".agents/skills/") {
        GuideKind::Reference
    } else if path.starts_with("docs/") {
        GuideKind::Doc
    } else {
        GuideKind::Plan
    }
}

/// Writes one event file and returns its path.
///
/// The file lands in `.agents/events/YYYY/MM/DD/<name>.json` (local state); the
/// payload carries the `guides` array so the durable half of the record — which
/// tracked artifact changed — is recoverable from the event.
///
/// # Errors
///
/// Returns an error when the event directory cannot be created, when a guide
/// reference is untracked, or when the payload cannot be written.
pub async fn write_event(
    root: &Path,
    name: &str,
    payload: serde_json::Value,
    guides: &[GuideRef],
) -> Result<PathBuf> {
    if let Some(bad) = guides.iter().find(|guide| !guide.is_tracked()) {
        anyhow::bail!(
            "event '{name}' cites untracked guide {}; distilled learning must land in a tracked artifact",
            bad.path
        );
    }
    if guides.is_empty() {
        anyhow::bail!("event '{name}' cites no guide; nothing durable would record the learning");
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    let (year, month, day) = civil_from_unix(now);
    let dir = root
        .join(".agents")
        .join("events")
        .join(format!("{year:04}"))
        .join(format!("{month:02}"))
        .join(format!("{day:02}"));
    tokio::fs::create_dir_all(&dir)
        .await
        .with_context(|| format!("failed to create event directory {}", dir.display()))?;

    let guides_json: Vec<serde_json::Value> = guides
        .iter()
        .map(|guide| serde_json::json!({ "kind": guide.kind.as_str(), "path": guide.path }))
        .collect();
    let document = serde_json::json!({
        "date": format!("{year:04}-{month:02}-{day:02}"),
        "event": name,
        "guides": guides_json,
        "payload": payload,
    });
    let path = dir.join(format!("{name}.json"));
    let text = serde_json::to_string_pretty(&document)
        .with_context(|| format!("failed to serialize event '{name}'"))?;
    tokio::fs::write(&path, format!("{text}\n"))
        .await
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

/// Converts a Unix timestamp to `(year, month, day)` in UTC.
///
/// Uses Howard Hinnant's `civil_from_days` so the harness needs no date
/// dependency just to name an event directory.
#[must_use]
pub fn civil_from_unix(seconds: i64) -> (i64, u32, u32) {
    let days = seconds.div_euclid(86_400);
    // Shift the epoch to 0000-03-01 so leap days land at the end of the cycle.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (
        year,
        u32::try_from(m).unwrap_or(1),
        u32::try_from(d).unwrap_or(1),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn civil_from_unix_matches_known_dates() {
        assert_eq!(civil_from_unix(0), (1970, 1, 1));
        // 2000-02-29 (a leap day inside a century-leap year).
        assert_eq!(civil_from_unix(951_782_400), (2000, 2, 29));
        // 2026-09-15T00:00:00Z.
        assert_eq!(civil_from_unix(1_789_430_400), (2026, 9, 15));
        // Pre-epoch input must not panic or produce a negative month.
        let (year, month, day) = civil_from_unix(-86_400);
        assert_eq!((year, month, day), (1969, 12, 31));
    }

    #[test]
    fn guide_refs_reject_untracked_targets() {
        assert!(GuideRef::skill("npm-github-publish").is_tracked());
        assert!(
            !GuideRef {
                kind: GuideKind::Doc,
                path: ".agents/events/2026/09/15/x.json".to_owned(),
            }
            .is_tracked()
        );
        assert!(
            !GuideRef {
                kind: GuideKind::Doc,
                path: ".do-harness/evidence.json".to_owned(),
            }
            .is_tracked()
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn write_event_persists_guides_and_rejects_empty() {
        let dir = tempfile::tempdir().unwrap();
        let guides = vec![GuideRef::skill("harness")];
        let path = write_event(
            dir.path(),
            "unit-test",
            serde_json::json!({ "note": "ok" }),
            &guides,
        )
        .await
        .unwrap();
        assert!(path.starts_with(dir.path().join(".agents/events")));
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.contains(".agents/skills/harness/SKILL.md"));
        assert!(saved.contains("\"guides\""));

        let err = write_event(dir.path(), "no-guides", serde_json::json!({}), &[])
            .await
            .unwrap_err();
        assert!(err.to_string().contains("cites no guide"), "{err}");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn write_event_refuses_an_untracked_guide() {
        let dir = tempfile::tempdir().unwrap();
        let guides = vec![GuideRef {
            kind: GuideKind::Doc,
            path: ".do-harness/scratch.md".to_owned(),
        }];
        let err = write_event(dir.path(), "bad", serde_json::json!({}), &guides)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("untracked guide"), "{err}");
    }
}
