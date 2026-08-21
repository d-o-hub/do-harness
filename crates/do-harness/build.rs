//! Build script for `do-harness` to capture git metadata at compile time.

use std::path::Path;
use std::process::Command;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let root_git_dir = Path::new(&manifest_dir).join("../../.git");

    if root_git_dir.exists() {
        println!(
            "cargo:rerun-if-changed={}",
            root_git_dir.join("HEAD").display()
        );
        println!(
            "cargo:rerun-if-changed={}",
            root_git_dir.join("index").display()
        );
    }

    let sha = get_git_sha();
    let date = get_git_date();
    let dirty = get_git_dirty();

    if let Some(s) = sha {
        println!("cargo:rustc-env=DO_HARNESS_GIT_SHA={s}");
    }
    if let Some(d) = date {
        println!("cargo:rustc-env=DO_HARNESS_GIT_DATE={d}");
    }
    println!("cargo:rustc-env=DO_HARNESS_GIT_DIRTY={dirty}");
}

fn get_git_sha() -> Option<String> {
    if let Ok(v) = std::env::var("DO_HARNESS_GIT_SHA") {
        if !v.is_empty() {
            return Some(v);
        }
    }
    if let Ok(v) = std::env::var("GIT_COMMIT") {
        if !v.is_empty() {
            return Some(v);
        }
    }
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    if output.status.success() {
        let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !sha.is_empty() {
            return Some(sha);
        }
    }
    None
}

fn get_git_date() -> Option<String> {
    if let Ok(v) = std::env::var("DO_HARNESS_GIT_DATE") {
        if !v.is_empty() {
            return Some(v);
        }
    }
    if let Ok(v) = std::env::var("GIT_DATE") {
        if !v.is_empty() {
            return Some(v);
        }
    }
    let output = Command::new("git")
        .args(["log", "-1", "--format=%cd", "--date=format:%Y-%m-%d"])
        .output()
        .ok()?;
    if output.status.success() {
        let date = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !date.is_empty() {
            return Some(date);
        }
    }
    None
}

fn get_git_dirty() -> bool {
    if let Ok(v) = std::env::var("DO_HARNESS_GIT_DIRTY") {
        return v == "true" || v == "1";
    }
    let output = Command::new("git").args(["status", "--porcelain"]).output();
    match output {
        Ok(out) if out.status.success() => !out.stdout.is_empty(),
        _ => false,
    }
}
