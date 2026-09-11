//! Blessing: pins grader hashes and ratchets the pass-rate bar.

use anyhow::{Result, bail};

use super::grading::SkillReport;

pub(super) async fn bless_skill(
    conn: &do_harness_db::Connection,
    name: &str,
    report: &SkillReport,
    hashes: &crate::eval_integrity::GraderHashes,
) -> Result<()> {
    if report.gate_failed {
        bail!("cannot bless skill '{name}': structure gate failed; fix it and rerun with --bless");
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
    do_harness_db::bless_grader_baseline(conn, name, &hashes.walkthrough_sha, &hashes.specs_sha)
        .await?;
    if report.graded > 0 {
        let best = do_harness_db::max_pass_rate(conn, name).await?;
        if let Some(floor) = crate::eval_integrity::GraderHashes::bar_floor(best) {
            if do_harness_db::raise_skill_bar(conn, name, floor).await? {
                println!("{name}: blessed; bar floor raised to {floor:.2}");
            } else {
                println!("{name}: blessed; bar floor unchanged");
            }
        }
    } else {
        println!("{name}: blessed; no graded assertions so no bar was set");
    }
    Ok(())
}
