#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fmt::Write;

use super::*;

/// A file body of `preamble` lines followed by an inline test module.
fn with_test_module(preamble: usize, tests: usize) -> String {
    let mut text = String::new();
    for i in 0..preamble {
        writeln!(text, "fn f{i}() {{}}").unwrap();
    }
    text.push_str("#[cfg(test)]\nmod tests {\n");
    for i in 0..tests {
        writeln!(text, "    fn t{i}() {{}}").unwrap();
    }
    text.push_str("}\n");
    text
}

#[test]
fn split_counts_marker_line_as_test() {
    let text = with_test_module(10, 3);
    let info = measure_text("x.rs".to_owned(), &text);
    assert_eq!(
        info.lines, 16,
        "10 code + cfg(test) + mod + 3 tests + brace"
    );
    assert_eq!(info.code_lines, Some(10));
    assert_eq!(info.test_lines, Some(6));
    assert_eq!(
        info.code_lines.unwrap() + info.test_lines.unwrap(),
        info.lines
    );
    assert_eq!(test_marker_line(&text), Some(11));
}

#[test]
fn file_without_test_module_has_no_split() {
    let info = measure_text("x.rs".to_owned(), "fn a() {}\nfn b() {}\n");
    assert_eq!(info.code_lines, None);
    assert_eq!(info.test_lines, None);
    assert_eq!(test_marker_line("fn a() {}\nfn b() {}\n"), None);
}

/// The ceiling itself is still OK; the threshold is inclusive.
#[test]
fn status_bands_are_inclusive_at_thresholds() {
    assert_eq!(LocStatus::of(WARN_THRESHOLD - 1), LocStatus::Ok);
    assert_eq!(LocStatus::of(WARN_THRESHOLD), LocStatus::Warn);
    assert_eq!(LocStatus::of(MAX_LINES), LocStatus::Warn);
    assert_eq!(LocStatus::of(MAX_LINES + 1), LocStatus::Fail);
}

/// `loc` must cover everything either shipped `check-loc.sh` scans, so a
/// scaffolded single-crate repo (`src/`) and this workspace (`crates/`) are
/// both reported, with `target/` and non-Rust files excluded.
#[test]
fn scan_covers_crates_and_src_but_skips_target() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("crates/app/src")).unwrap();
    std::fs::create_dir_all(root.join("crates/app/target/debug")).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("crates/app/src/lib.rs"), "fn a() {}\n").unwrap();
    std::fs::write(root.join("crates/app/target/debug/gen.rs"), "fn b() {}\n").unwrap();
    std::fs::write(root.join("src/outside.rs"), "fn c() {}\n").unwrap();
    std::fs::write(root.join("crates/app/README.md"), "noise\n").unwrap();

    let scope = LocScope::resolve(root, None, &[], &[]);
    let infos = scan(root, &scope).unwrap();
    let paths: Vec<&str> = infos.iter().map(|info| info.path.as_str()).collect();
    assert_eq!(paths, vec!["crates/app/src/lib.rs", "src/outside.rs"]);
}

/// A scaffolded single-crate repo has no `crates/` at all: `loc` must still
/// measure it rather than failing.
#[test]
fn scan_works_without_a_crates_directory() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();

    let scope = LocScope::resolve(root, None, &[], &[]);
    let infos = scan(root, &scope).unwrap();
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].path, "src/main.rs");
}

#[test]
fn scan_fails_when_no_sources_exist() {
    let dir = tempfile::tempdir().unwrap();
    let scope = LocScope::resolve(dir.path(), None, &[], &[]);
    let err = scan(dir.path(), &scope).unwrap_err().to_string();
    assert!(err.contains("no source files matching"), "{err}");
}

#[test]
fn explicit_paths_are_measured_and_missing_paths_fail() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("crates/app")).unwrap();
    std::fs::write(root.join("crates/app/lib.rs"), with_test_module(2, 1)).unwrap();
    std::fs::write(root.join("crates/app/other.rs"), "fn a() {}\n").unwrap();
    std::fs::write(root.join("crates/app/notes.txt"), "noise\n").unwrap();

    let scope = LocScope::resolve(root, None, &[], &[]);
    let infos = collect_paths(root, &[root.join("crates/app")], &scope.exts).unwrap();
    assert_eq!(
        infos.iter().map(|i| i.path.as_str()).collect::<Vec<_>>(),
        vec!["crates/app/lib.rs", "crates/app/other.rs"],
        "directories expand to matching files only"
    );
    assert_eq!(infos[0].test_lines, Some(4));

    assert!(
        collect_paths(root, &[root.join("crates/missing")], &scope.exts)
            .unwrap_err()
            .to_string()
            .contains("no such file or directory")
    );
}

#[test]
fn scan_generic_workspace_discovers_packages_and_apps() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("packages/contracts/src")).unwrap();
    std::fs::create_dir_all(root.join("apps/web/src")).unwrap();
    std::fs::create_dir_all(root.join("packages/contracts/node_modules")).unwrap();
    std::fs::write(root.join("do-harness.toml"), "language = \"generic\"\n").unwrap();
    std::fs::write(
        root.join("packages/contracts/src/evidence-framing.ts"),
        "export const x = 1;\n",
    )
    .unwrap();
    std::fs::write(
        root.join("apps/web/src/index.tsx"),
        "export const App = () => null;\n",
    )
    .unwrap();
    std::fs::write(
        root.join("packages/contracts/node_modules/noise.ts"),
        "export const noise = 2;\n",
    )
    .unwrap();

    let scope = LocScope::resolve(root, None, &[], &[]);
    assert_eq!(scope.roots, vec!["packages", "apps", "src", "crates"]);
    assert_eq!(scope.exts, vec!["ts", "tsx", "js", "jsx", "rs"]);

    let infos = scan(root, &scope).unwrap();
    let paths: Vec<&str> = infos.iter().map(|info| info.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "apps/web/src/index.tsx",
            "packages/contracts/src/evidence-framing.ts"
        ]
    );
}

#[test]
fn cli_root_and_ext_overrides_win() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("web/src")).unwrap();
    std::fs::write(root.join("web/src/app.ts"), "const a = 1;\n").unwrap();
    std::fs::write(root.join("web/src/ignored.rs"), "fn f() {}\n").unwrap();

    let scope = LocScope::resolve(
        root,
        None,
        &["./web/src/".to_string()],
        &[".ts".to_string()],
    );
    assert_eq!(scope.roots, vec!["web/src"]);
    assert_eq!(scope.exts, vec!["ts"]);
    assert!(scope.roots_explicit);

    let infos = scan(root, &scope).unwrap();
    assert_eq!(infos.len(), 1);
    assert_eq!(infos[0].path, "web/src/app.ts");
}

#[test]
fn parses_sensor_argv_roots_and_exts() {
    let argv = vec![
        "bash".to_owned(),
        "scripts/check-loc.sh".to_owned(),
        "--root".to_owned(),
        "web,src,crates".to_owned(),
        "--ext".to_owned(),
        "rs,ts,tsx".to_owned(),
    ];
    let (roots, exts) = parse_sensor_argv(&argv);
    assert_eq!(roots, vec!["web", "src", "crates"]);
    assert_eq!(exts, vec!["rs", "ts", "tsx"]);
}
