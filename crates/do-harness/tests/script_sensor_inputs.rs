//! Keeps `do-harness.toml`'s `when-changed` lists honest for shared shell code.
//!
//! `scripts/lib.sh` is sourced by most of the repository's sensor scripts, so a
//! change to it changes what those sensors would report. A sensor that does not
//! list it is still *run* by the full suite, but `--changed` selection silently
//! skips it — the change is then never checked by the very sensor it altered.
//! This test derives the wiring from the scripts themselves rather than from a
//! hand-maintained list, so adding a `source .../lib.sh` line to a new sensor
//! script fails here until the policy names the shared file.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Does a `when-changed` pattern match `scripts/lib.sh`?
///
/// `when-changed` globs are simple (`*`, `**`, literal), so this is a minimal
/// matcher rather than a dependency: `*` never crosses `/`, `**` does.
fn pattern_matches(pattern: &str, path: &str) -> bool {
    fn walk(pattern: &[u8], path: &[u8]) -> bool {
        match (pattern.first(), path.first()) {
            (None, None) => true,
            (Some(b'*'), _) if pattern.get(1) == Some(&b'*') => {
                let rest = &pattern[2..];
                // `**/` also matches zero directories.
                if rest.first() == Some(&b'/') && walk(&rest[1..], path) {
                    return true;
                }
                (0..=path.len()).any(|skip| walk(rest, &path[skip..]))
            }
            (Some(b'*'), _) => (0..=path.len()).any(|end| {
                path[..end].iter().all(|byte| *byte != b'/') && walk(&pattern[1..], &path[end..])
            }),
            (Some(expected), Some(actual)) if expected == actual => walk(&pattern[1..], &path[1..]),
            _ => false,
        }
    }
    walk(pattern.as_bytes(), path.as_bytes())
}

/// The `scripts/*.sh` files a sensor's argv names.
fn argv_scripts(argv: &[toml::Value], root: &Path) -> Vec<(String, PathBuf)> {
    argv.iter()
        .filter_map(|arg| arg.as_str())
        .filter(|arg| arg.starts_with("scripts/"))
        .filter(|arg| Path::new(arg).extension().is_some_and(|ext| ext == "sh"))
        .map(|arg| (arg.to_owned(), root.join(arg)))
        .collect()
}

#[test]
fn every_sensor_that_runs_a_lib_sourcing_script_declares_lib_sh_as_an_input() {
    let root = workspace_root();
    let policy = root.join("do-harness.toml");
    let content = std::fs::read_to_string(&policy).expect("read do-harness.toml");
    let document: toml::Value = toml::from_str(&content).expect("parse do-harness.toml");
    let sensors = document
        .get("sensors")
        .and_then(|value| value.as_array())
        .expect("[[sensors]] entries");

    let mut findings = Vec::new();

    for sensor in sensors {
        let name = sensor
            .get("name")
            .and_then(|value| value.as_str())
            .expect("sensor name");
        let argv = sensor
            .get("argv")
            .and_then(|value| value.as_array())
            .expect("sensor argv");

        let sources_lib = argv_scripts(argv, &root).into_iter().any(|(_, path)| {
            std::fs::read_to_string(&path).is_ok_and(|body| body.contains("lib.sh"))
        });
        if !sources_lib {
            continue;
        }

        // A sensor with no `when-changed` runs on every change (e.g. commitlint),
        // so it already sees a `lib.sh` edit and needs no entry.
        let Some(patterns) = sensor
            .get("when-changed")
            .and_then(|value| value.as_array())
        else {
            continue;
        };
        let declares = patterns
            .iter()
            .filter_map(|pattern| pattern.as_str())
            .any(|pattern| pattern_matches(pattern, "scripts/lib.sh"));

        if !declares {
            findings.push(name.to_owned());
        }
    }

    assert!(
        findings.is_empty(),
        "sensor(s) {findings:?} run a script that sources scripts/lib.sh but do not list it in \
         `when-changed`, so `verify --changed` would skip them after a lib.sh edit"
    );
}

#[test]
fn the_matcher_separates_single_and_double_star() {
    assert!(pattern_matches("scripts/lib.sh", "scripts/lib.sh"));
    assert!(pattern_matches("scripts/*.sh", "scripts/lib.sh"));
    assert!(pattern_matches("scripts/**", "scripts/lib.sh"));
    assert!(pattern_matches("**/*.sh", "scripts/lib.sh"));
    assert!(!pattern_matches("*.sh", "scripts/lib.sh"));
    assert!(!pattern_matches("scripts/*.sh", "scripts/lib/lib.sh"));
    assert!(!pattern_matches("scripts/lib.sh", "scripts/lib-changed.sh"));
}
