//! Structure gate: runs `quick_validate.py` against a skill directory.

use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GateVerdict {
    Pass,
    Fail,
    Unavailable,
}

#[must_use]
pub(super) fn run_structure_gate(dir: &Path, gate_script: &Path) -> (GateVerdict, String) {
    if !gate_script.is_file() {
        return (
            GateVerdict::Unavailable,
            format!("quick_validate.py not found at {}", gate_script.display()),
        );
    }
    let Ok(output) = Command::new("python3").arg(gate_script).arg(dir).output() else {
        eprintln!("warning: python3 is missing or unavailable; structure gate skipped");
        return (
            GateVerdict::Unavailable,
            "gate could not be executed (python3 missing)".to_owned(),
        );
    };
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    let message = combined
        .lines()
        .last()
        .unwrap_or_default()
        .trim()
        .to_owned();
    let verdict = if output.status.success() {
        GateVerdict::Pass
    } else {
        GateVerdict::Fail
    };
    (verdict, message)
}
