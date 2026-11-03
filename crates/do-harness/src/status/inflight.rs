//! Running-state report construction for `status`.

use super::{CurrentSide, EvidenceState, StatusReport, report};
use crate::evidence::running::RunningInfo;

/// Builds the `running` report for an in-flight verify run.
///
/// An in-flight run outranks evidence on disk: the run is about to replace
/// it, so judging the old artifact would invite a duplicate launch.
pub(super) fn running_report(
    set: Option<&str>,
    current: CurrentSide,
    running: RunningInfo,
) -> StatusReport {
    let mut report = report(
        EvidenceState::Running,
        set,
        "in_progress",
        None,
        current,
        Vec::new(),
    );
    report.running = Some(running);
    report
}
