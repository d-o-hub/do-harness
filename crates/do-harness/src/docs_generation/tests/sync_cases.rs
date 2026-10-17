//! The sync engine end to end: fixtures, drift detection, and repair.

use std::fs;

use crate::docs_generation::contract::{CliContract, build};
use crate::docs_generation::sync::{self, Issue};
use crate::docs_generation::tests::support::{region, write_file};

/// Builds a temporary repository whose committed artifacts match `contract`.
fn fixture_repo(contract: &CliContract) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    for expectation in sync::expectations(contract) {
        let id = expectation.region.as_deref();
        match (expectation.file, id) {
            (sync::README_FILE, Some(id)) => write_file(
                root,
                sync::README_FILE,
                &format!(
                    "# Fixture\n\nIntro prose.\n\n{}\nTrailing prose.\n",
                    region(id, &expectation.expected)
                ),
            ),
            (sync::CLI_REFERENCE_FILE, Some(id)) => {
                let path = root.join(sync::CLI_REFERENCE_FILE);
                let mut text = fs::read_to_string(&path)
                    .unwrap_or_else(|_| String::from("# CLI reference\n\n"));
                text.push_str(&region(id, &expectation.expected));
                text.push('\n');
                write_file(root, sync::CLI_REFERENCE_FILE, &text);
            }
            (sync::CONTRACT_FILE, None) => {
                write_file(root, sync::CONTRACT_FILE, &expectation.expected);
            }
            (file, id) => panic!("unexpected expectation {file} {id:?}"),
        }
    }
    dir
}

/// Removes one generated region (markers included) from a fixture file.
fn remove_region(text: &str, id: &str) -> String {
    let begin = format!("<!-- BEGIN GENERATED: {id} -->");
    let end = format!("<!-- END GENERATED: {id} -->");
    let start = text.find(&begin).expect("begin marker");
    let stop = text.find(&end).expect("end marker") + end.len();
    let mut updated = String::with_capacity(text.len());
    updated.push_str(&text[..start]);
    updated.push_str(&text[stop..]);
    updated
}

#[test]
fn fresh_fixture_is_clean() {
    let contract = build();
    let dir = fixture_repo(&contract);
    let report = sync::check_contract(dir.path(), &contract);
    assert!(report.is_clean(), "{}", report.message());
}

#[test]
fn removing_a_readme_row_is_a_fixable_diff_that_apply_repairs() {
    let contract = build();
    let dir = fixture_repo(&contract);
    let readme = dir.path().join(sync::README_FILE);
    let text = fs::read_to_string(&readme).expect("fixture README");
    let lines: Vec<&str> = text.lines().collect();
    let separator = lines
        .iter()
        .position(|line| line.starts_with("| ---"))
        .expect("table separator row");
    let row = separator + 1;
    let dropped = lines
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != row)
        .map(|(_, line)| *line)
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&readme, format!("{dropped}\n")).expect("mutated README");

    let report = sync::check_contract(dir.path(), &contract);
    assert!(
        report.issues.iter().any(|issue| matches!(
            issue,
            Issue::Diff { file, region: Some(region), .. }
                if file == sync::README_FILE && region == sync::README_REGION
        )),
        "{}",
        report.message()
    );
    assert!(report.message().contains("repair:"), "{}", report.message());

    let changed = report
        .apply(dir.path())
        .expect("one row removal is fixable");
    assert_eq!(changed.len(), 1);
    let clean = sync::check_contract(dir.path(), &contract);
    assert!(clean.is_clean(), "{}", clean.message());

    let bytes = fs::read(&readme).expect("README bytes");
    let again = sync::check_contract(dir.path(), &contract);
    let changed_again = again.apply(dir.path()).expect("clean repo applies nothing");
    assert_eq!(
        changed_again,
        Vec::<String>::new(),
        "clean repo applies nothing"
    );
    assert_eq!(
        bytes,
        fs::read(&readme).expect("README bytes"),
        "two update runs are byte-identical"
    );
}

#[test]
fn deleting_one_command_section_is_reported_and_never_auto_inserted() {
    let contract = build();
    let dir = fixture_repo(&contract);
    let cli = dir.path().join(sync::CLI_REFERENCE_FILE);
    let text = fs::read_to_string(&cli).expect("fixture reference");
    fs::write(&cli, remove_region(&text, "cli-contract pr review")).expect("mutated reference");

    let report = sync::check_contract(dir.path(), &contract);
    assert!(
        report.issues.iter().any(|issue| matches!(
            issue,
            Issue::MissingRegion { region, .. } if region == "cli-contract pr review"
        )),
        "{}",
        report.message()
    );
    assert!(report.message().contains("cli-contract pr review"));
    let error = report
        .apply(dir.path())
        .expect_err("structural issues are refused");
    assert!(error.contains("missing"), "{error}");
}

#[test]
fn obsolete_command_regions_are_reported_as_extra() {
    let contract = build();
    let dir = fixture_repo(&contract);
    let cli = dir.path().join(sync::CLI_REFERENCE_FILE);
    let text = fs::read_to_string(&cli).expect("fixture reference");
    let mut mutated = text.clone();
    mutated.push('\n');
    mutated.push_str(&region(
        "cli-contract gone-away",
        "`do-harness gone-away`\n",
    ));
    fs::write(&cli, mutated).expect("mutated reference");

    let report = sync::check_contract(dir.path(), &contract);
    assert!(
        report.issues.iter().any(|issue| matches!(
            issue,
            Issue::ExtraRegion { region, .. } if region == "cli-contract gone-away"
        )),
        "{}",
        report.message()
    );
    let error = report
        .apply(dir.path())
        .expect_err("extra regions are refused");
    assert!(
        error.contains("obsolete") || error.contains("no longer produced"),
        "{error}"
    );
}

#[test]
fn missing_files_and_malformed_markers_are_reported() {
    let contract = build();
    let empty = tempfile::tempdir().expect("tempdir");
    let report = sync::check_contract(empty.path(), &contract);
    let missing: Vec<&str> = report
        .issues
        .iter()
        .filter_map(|issue| match issue {
            Issue::MissingFile { file, .. } => Some(file.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        missing,
        [
            sync::README_FILE,
            sync::CLI_REFERENCE_FILE,
            sync::CONTRACT_FILE
        ]
    );

    let dir = tempfile::tempdir().expect("tempdir");
    write_file(
        dir.path(),
        sync::CLI_REFERENCE_FILE,
        "<!-- BEGIN GENERATED: cli-globals -->\nno end\n",
    );
    write_file(dir.path(), sync::README_FILE, "");
    let report = sync::check_contract(dir.path(), &contract);
    assert!(
        report.issues.iter().any(|issue| matches!(
            issue,
            Issue::Malformed { file, detail }
                if file == sync::CLI_REFERENCE_FILE && detail.contains("without an END")
        )),
        "{}",
        report.message()
    );
}

#[cfg(unix)]
#[test]
fn symlinked_artifacts_are_written_through_the_link() {
    let dir = tempfile::tempdir().expect("tempdir");
    let real = dir.path().join("real/README.md");
    fs::create_dir_all(real.parent().expect("parent")).expect("create dirs");
    fs::write(
        &real,
        format!("before\n{}\nafter\n", region(sync::README_REGION, "old\n")),
    )
    .expect("write target");
    let link = dir.path().join(sync::README_FILE);
    std::os::unix::fs::symlink(&real, &link).expect("create symlink");

    sync::write_region(&link, sync::README_REGION, "new content\n").expect("write through link");
    assert!(
        fs::symlink_metadata(&link)
            .expect("link metadata")
            .file_type()
            .is_symlink(),
        "the committed symlink must stay a symlink"
    );
    let updated = fs::read_to_string(&real).expect("target content");
    assert!(updated.contains("new content"), "{updated}");
    assert!(updated.starts_with("before\n"), "{updated}");
    assert!(updated.ends_with("after\n"), "{updated}");
}

/// Byte offsets of a generated region's content (between the marker pair).
fn region_span(text: &str, id: &str) -> (usize, usize) {
    let begin = format!("<!-- BEGIN GENERATED: {id} -->");
    let end = format!("<!-- END GENERATED: {id} -->");
    let start = text.find(&begin).expect("begin marker") + begin.len();
    let stop = text.find(&end).expect("end marker");
    (start, stop)
}

/// Rewrites one region's content, leaving every other byte untouched.
fn mutate_region(text: &str, id: &str, edit: impl FnOnce(&str) -> String) -> String {
    let (start, stop) = region_span(text, id);
    format!(
        "{}{}{}",
        &text[..start],
        edit(&text[start..stop]),
        &text[stop..]
    )
}

/// The first table data row inside a region.
fn first_data_row(text: &str, id: &str) -> String {
    let (start, stop) = region_span(text, id);
    text[start..stop]
        .lines()
        .find(|line| line.starts_with("| `"))
        .expect("a data row")
        .to_string()
}

/// The current bytes of every committed artifact.
fn artifacts(root: &std::path::Path) -> Vec<Vec<u8>> {
    [
        sync::README_FILE,
        sync::CLI_REFERENCE_FILE,
        sync::CONTRACT_FILE,
    ]
    .iter()
    .map(|file| fs::read(root.join(file)).expect("artifact bytes"))
    .collect()
}

#[test]
fn check_mode_never_writes_even_when_drifting() {
    let contract = build();
    let dir = fixture_repo(&contract);
    let readme = dir.path().join(sync::README_FILE);
    let text = fs::read_to_string(&readme).expect("fixture README");
    let mutated = text.replacen("| `verify", "| `verify-renamed", 1);
    assert_ne!(text, mutated, "the fixture must contain a verify row");
    fs::write(&readme, &mutated).expect("mutated README");

    let before = artifacts(dir.path());
    let report = sync::check_contract(dir.path(), &contract);
    assert!(!report.is_clean(), "the mutation must be detected");
    assert_eq!(before, artifacts(dir.path()), "check mode must never write");
}

#[test]
fn changed_defaults_aliases_and_moved_flag_rows_fail_until_regenerated() {
    let contract = build();
    let dir = fixture_repo(&contract);
    let cli = dir.path().join(sync::CLI_REFERENCE_FILE);
    let text = fs::read_to_string(&cli).expect("fixture reference");

    // A default value changed inside `task export`.
    let text = mutate_region(&text, "cli-contract task export", |region| {
        assert!(region.contains("| `json` |"), "{region}");
        region.replacen("| `json` |", "| `yaml` |", 1)
    });
    // An alias changed inside `verify`.
    let text = mutate_region(&text, "cli-contract verify", |region| {
        assert!(region.contains("Aliases: `check`."), "{region}");
        region.replacen("Aliases: `check`.", "Aliases: `checkit`.", 1)
    });
    // One flag row moved from `pr review` to `pr ready`: the text is still
    // present in the document, but under the wrong command, so the region
    // that lost it must still report a diff (mention alone is not coverage).
    let row = first_data_row(&text, "cli-contract pr review");
    let text = mutate_region(&text, "cli-contract pr review", |region| {
        region.replacen(&format!("{row}\n"), "", 1)
    });
    let text = mutate_region(&text, "cli-contract pr ready", |region| {
        region.replacen(
            "| --- | --- | --- | --- | --- |",
            &format!("| --- | --- | --- | --- | --- |\n{row}"),
            1,
        )
    });
    fs::write(&cli, &text).expect("mutated reference");

    let report = sync::check_contract(dir.path(), &contract);
    for changed in [
        "cli-contract task export",
        "cli-contract verify",
        "cli-contract pr review",
        "cli-contract pr ready",
    ] {
        assert!(
            report.issues.iter().any(|issue| matches!(
                issue,
                Issue::Diff { region: Some(found), .. } if found == changed
            )),
            "region `{changed}` must be reported\n{}",
            report.message()
        );
    }

    let changed = report.apply(dir.path()).expect("content diffs are fixable");
    assert_eq!(changed.len(), 4);
    let clean = sync::check_contract(dir.path(), &contract);
    assert!(clean.is_clean(), "{}", clean.message());
}
