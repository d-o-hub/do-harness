//! Blessing: pins grader hashes and ratchets the pass-rate bar.

use anyhow::{Result, bail};

use super::grading::SkillReport;

pub(super) async fn bless_skill(
    conn: &do_harness_db::Connection,
    name: &str,
    report: &SkillReport,
    hashes: &crate::eval_integrity::GraderHashes,
    approver: &str,
    format: crate::report::Format,
) -> Result<()> {
    if report.gate_failed {
        bail!("cannot bless skill '{name}': structure gate failed; fix it and rerun with --bless");
    }
    if report.evals_invalid {
        bail!(
            "cannot bless skill '{name}': evals/evals.json does not parse; \
             fix the fixture and rerun with --bless"
        );
    }
    match (report.graded, report.passed) {
        (0, _) => {}
        (graded, passed) if passed < graded => {
            bail!(
                "cannot bless skill '{name}': {passed}/{graded} assertions green; only fully green runs are blessable"
            );
        }
        _ => {}
    }
    // A lift floor is measured against one set of graders, so capture whether
    // they changed before the baseline below is overwritten. Re-blessing a new
    // fixture set starts its floor from its own lift; ratcheting across that
    // boundary would make a legitimate fixture change permanently unreachable.
    let previous = do_harness_db::get_grader_baseline(conn, name).await?;
    let fixture_set_changed = previous
        .as_ref()
        .is_some_and(|baseline| !hashes.matches_baseline(baseline));
    do_harness_db::bless_grader_baseline(
        conn,
        name,
        &hashes.walkthrough_sha,
        &hashes.specs_sha,
        approver,
        None,
    )
    .await?;
    if report.graded > 0 {
        let best = do_harness_db::max_pass_rate(conn, name).await?;
        if let Some(floor) = crate::eval_integrity::GraderHashes::bar_floor(best) {
            if do_harness_db::raise_skill_bar(conn, name, report.mode, floor).await? {
                super::diag!(
                    format,
                    "{name}: blessed; {} bar floor raised to {floor:.2}",
                    report.mode
                );
            } else {
                super::diag!(
                    format,
                    "{name}: blessed; {} bar floor unchanged",
                    report.mode
                );
            }
        }
        if let Some(floor) = crate::eval_integrity::GraderHashes::lift_floor(report.lift) {
            let moved = if fixture_set_changed {
                do_harness_db::set_lift_floor(conn, name, report.mode, floor).await?
            } else {
                do_harness_db::raise_lift_floor(conn, name, report.mode, floor).await?
            };
            if moved {
                let verb = if fixture_set_changed {
                    "reset"
                } else {
                    "raised"
                };
                super::diag!(
                    format,
                    "{name}: blessed; {} lift floor {verb} to {floor:+.2}",
                    report.mode,
                );
            } else {
                super::diag!(
                    format,
                    "{name}: blessed; {} lift floor unchanged",
                    report.mode
                );
            }
        }
    } else {
        super::diag!(
            format,
            "{name}: blessed; no graded assertions so no bar was set"
        );
    }
    Ok(())
}
