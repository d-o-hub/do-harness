//! Unit tests for the declarative policy engine (`policy.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use serde_json::json;
use std::fs;
use std::path::Path;

use crate::McpLikeToolCall;

fn call(tool: &str, params: Option<serde_json::Value>) -> McpLikeToolCall {
    McpLikeToolCall::new(tool.to_string(), params)
}

fn parse(policy_yaml: &str) -> Policy {
    serde_yaml::from_str(policy_yaml).expect("parse policy")
}

#[test]
fn test_parse_valid_policy() {
    let policy = parse(
        r#"
version: 1
rules:
  - action: deny
    tool: "fs.remove"
"#,
    );
    assert_eq!(policy.version, 1);
    assert_eq!(policy.rules.len(), 1);
}

/// Unknown fields are rejected: a policy typo fails closed, never silently
/// ignored.
#[test]
fn test_parse_rejects_unknown_fields() {
    let err = serde_yaml::from_str::<Policy>("version: 1\nbogus: true\n").unwrap_err();
    assert!(err.to_string().contains("unknown field"), "{err}");
    let err = serde_yaml::from_str::<Rule>("action: deny\ntool: t\nnope: 1\n").unwrap_err();
    assert!(err.to_string().contains("unknown field"), "{err}");
}

#[test]
fn test_parse_rejects_invalid_action() {
    let err =
        serde_yaml::from_str::<Policy>("version: 1\nrules:\n  - action: maybe\n    tool: t\n")
            .unwrap_err();
    assert_ne!(err.to_string(), String::new());
}

#[test]
fn test_load_policy_records_sha256() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("policy.yaml");
    fs::write(&path, "version: 1\n").expect("write");
    let loaded = load_policy(&path).expect("load");
    assert_eq!(loaded.sha256.len(), 64);
    assert!(loaded.sha256.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn test_load_policy_rejects_missing_file() {
    let err = load_policy(Path::new("/nonexistent/policy.yaml")).expect_err("missing file");
    assert!(err.to_string().contains("policy"), "{err}");
}

#[test]
fn test_load_policy_rejects_bad_yaml() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("policy.yaml");
    fs::write(&path, "version: [unclosed").expect("write");
    let err = load_policy(&path).expect_err("bad yaml");
    assert!(err.to_string().contains("policy"), "{err}");
}

#[test]
fn test_load_policy_rejects_unsupported_version() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("policy.yaml");
    fs::write(&path, "version: 99\n").expect("write");
    let err = load_policy(&path).expect_err("unsupported version");
    assert!(err.to_string().contains("version"), "{err}");
}

#[test]
fn test_evaluate_empty_policy_default_denies() {
    let policy = parse("version: 1\n");
    let decision = policy.evaluate(&call("data.read", None));
    assert_eq!(
        decision,
        PolicyDecision::Deny {
            reason: NO_RULE_MATCH_REASON.to_string(),
        }
    );
}

#[test]
fn test_evaluate_first_match_wins_precedence() {
    let policy = parse(
        r#"
version: 1
rules:
  - action: deny
    tool: "data.*"
  - action: allow
    tool: "data.read"
"#,
    );
    assert_eq!(
        policy.evaluate(&call("data.read", None)),
        PolicyDecision::Deny {
            reason: "denied by policy".to_string(),
        }
    );
}

#[test]
fn test_evaluate_tool_glob_patterns() {
    let policy = parse(
        r#"
version: 1
rules:
  - action: allow
    tool: "data.*"
  - action: allow
    tool: "*read"
"#,
    );
    assert_eq!(
        policy.evaluate(&call("data.read", None)),
        PolicyDecision::Allow
    );
    assert_eq!(
        policy.evaluate(&call("data.write", None)),
        PolicyDecision::Allow
    );
    assert_eq!(
        policy.evaluate(&call("cache.read", None)),
        PolicyDecision::Allow
    );
    assert_eq!(
        policy.evaluate(&call("cache.write", None)),
        PolicyDecision::Deny {
            reason: NO_RULE_MATCH_REASON.to_string(),
        }
    );
}

#[test]
fn test_evaluate_caller_patterns() {
    let policy = parse(
        r#"
version: 1
rules:
  - action: deny
    tool: "shell.exec"
    caller: "ci"
  - action: allow
    tool: "shell.exec"
"#,
    );
    let ci = call("shell.exec", None).with_caller(Some("ci".to_string()));
    assert!(matches!(policy.evaluate(&ci), PolicyDecision::Deny { .. }));
    let human = call("shell.exec", None).with_caller(Some("alice".to_string()));
    assert_eq!(policy.evaluate(&human), PolicyDecision::Allow);
    // Unnamed callers do not match the "ci" caller rule.
    assert_eq!(
        policy.evaluate(&call("shell.exec", None)),
        PolicyDecision::Allow
    );
}

#[test]
fn test_evaluate_params_glob_and_equality() {
    let policy = parse(
        r#"
version: 1
rules:
  - action: deny
    tool: "fs.remove"
    params:
      path: "/etc/*"
  - action: deny
    tool: "db.query"
    params:
      dry_run: false
"#,
    );
    let etc = call("fs.remove", Some(json!({"path": "/etc/passwd"})));
    assert!(matches!(policy.evaluate(&etc), PolicyDecision::Deny { .. }));
    // Unmatched parameter values are not denied by these rules; default deny applies.
    let home = call("fs.remove", Some(json!({"path": "/home/me.txt"})));
    assert_eq!(
        policy.evaluate(&home),
        PolicyDecision::Deny {
            reason: NO_RULE_MATCH_REASON.to_string(),
        }
    );
    // Missing parameter key never matches.
    let no_params = call("fs.remove", Some(json!({"recursive": true})));
    assert_eq!(
        policy.evaluate(&no_params),
        PolicyDecision::Deny {
            reason: NO_RULE_MATCH_REASON.to_string(),
        }
    );
    // Non-string equality compares exactly.
    let live = call("db.query", Some(json!({"dry_run": false})));
    assert!(matches!(
        policy.evaluate(&live),
        PolicyDecision::Deny { .. }
    ));
    let dry = call("db.query", Some(json!({"dry_run": true})));
    assert_eq!(
        policy.evaluate(&dry),
        PolicyDecision::Deny {
            reason: NO_RULE_MATCH_REASON.to_string(),
        }
    );
}

#[test]
fn test_evaluate_params_absent_means_any() {
    let policy = parse(
        r#"
version: 1
rules:
  - action: deny
    tool: "fs.remove"
"#,
    );
    assert!(matches!(
        policy.evaluate(&call("fs.remove", Some(json!({"a": 1})))),
        PolicyDecision::Deny { .. }
    ));
}

#[test]
fn test_evaluate_ask_rule() {
    let policy = parse(
        r#"
version: 1
rules:
  - action: ask
    tool: "shell.exec"
    caller: "ci"
    reason: "review needed"
"#,
    );
    let ci = call("shell.exec", None).with_caller(Some("ci".to_string()));
    assert_eq!(
        policy.evaluate(&ci),
        PolicyDecision::Ask {
            reason: "review needed".to_string(),
        }
    );
}

#[test]
fn test_evaluate_reason_fallbacks() {
    let deny = parse("version: 1\nrules:\n  - action: deny\n    tool: t\n");
    assert_eq!(
        deny.evaluate(&call("t", None)),
        PolicyDecision::Deny {
            reason: "denied by policy".to_string(),
        }
    );
    let ask = parse("version: 1\nrules:\n  - action: ask\n    tool: t\n");
    assert_eq!(
        ask.evaluate(&call("t", None)),
        PolicyDecision::Ask {
            reason: "requires human approval".to_string(),
        }
    );
}

#[test]
fn test_wildcard_match() {
    assert!(wildcard_match("*", "anything"));
    assert!(wildcard_match("data.*", "data.read"));
    assert!(wildcard_match("*read", "cache.read"));
    assert!(!wildcard_match("data.*", "data"));
    assert!(wildcard_match("data*", "data"));
    assert!(!wildcard_match("data.*", "metadata.read"));
    assert!(!wildcard_match("exact", "exactly"));
    assert!(wildcard_match("a*b*c", "aXbYc"));
    assert!(!wildcard_match("a*b*c", "aXbY"));
}

fn write_fixture(dir: &Path, name: &str, content: &str) {
    fs::write(dir.join(name), content).expect("write fixture");
}

#[test]
fn test_run_fixtures_passes_all_green() {
    let policy_dir = tempfile::tempdir().expect("tempdir");
    let policy_path = policy_dir.path().join("policy.yaml");
    fs::write(
        &policy_path,
        r#"
version: 1
rules:
  - action: deny
    tool: "fs.remove"
  - action: allow
    tool: "data.read"
"#,
    )
    .expect("write policy");
    let fx = tempfile::tempdir().expect("tempdir");
    write_fixture(
        fx.path(),
        "allow.json",
        r#"{ "name": "allow-read", "request": {"tool": "data.read"}, "expected": "allow" }"#,
    );
    write_fixture(
        fx.path(),
        "deny.yaml",
        r"
name: deny-remove
request:
  tool: fs.remove
expected: deny
",
    );
    let summary = run_fixtures(&policy_path, fx.path()).expect("run");
    assert_eq!(summary.total, 2);
    assert_eq!(summary.passed, 2);
    assert_eq!(summary.failures, Vec::<String>::new());
}

#[test]
fn test_run_fixtures_reports_mismatch() {
    let policy_dir = tempfile::tempdir().expect("tempdir");
    let policy_path = policy_dir.path().join("policy.yaml");
    fs::write(
        &policy_path,
        "version: 1\nrules:\n  - action: deny\n    tool: t\n",
    )
    .expect("write policy");
    let fx = tempfile::tempdir().expect("tempdir");
    write_fixture(
        fx.path(),
        "mismatch.json",
        r#"{ "request": {"tool": "t"}, "expected": "allow" }"#,
    );
    let summary = run_fixtures(&policy_path, fx.path()).expect("run");
    assert_eq!(summary.total, 1);
    assert_eq!(summary.passed, 0);
    assert_eq!(summary.failures.len(), 1);
    assert!(
        summary.failures[0].contains("expected allow, got deny"),
        "{}",
        summary.failures[0]
    );
}

#[test]
fn test_run_fixtures_reports_invalid_expected() {
    let policy_dir = tempfile::tempdir().expect("tempdir");
    let policy_path = policy_dir.path().join("policy.yaml");
    fs::write(&policy_path, "version: 1\n").expect("write policy");
    let fx = tempfile::tempdir().expect("tempdir");
    write_fixture(
        fx.path(),
        "bad-expected.json",
        r#"{ "request": {"tool": "t"}, "expected": "maybe" }"#,
    );
    let summary = run_fixtures(&policy_path, fx.path()).expect("run");
    assert_eq!(summary.failures.len(), 1);
    assert!(
        summary.failures[0].contains("invalid expected"),
        "{}",
        summary.failures[0]
    );
}

#[test]
fn test_run_fixtures_fails_closed_on_missing_policy() {
    let fx = tempfile::tempdir().expect("tempdir");
    let err =
        run_fixtures(Path::new("/nonexistent/policy.yaml"), fx.path()).expect_err("missing policy");
    assert!(err.to_string().contains("policy"), "{err}");
}

#[test]
fn test_run_fixtures_fails_closed_on_missing_dir() {
    let policy_dir = tempfile::tempdir().expect("tempdir");
    let policy_path = policy_dir.path().join("policy.yaml");
    fs::write(&policy_path, "version: 1\n").expect("write policy");
    let err = run_fixtures(&policy_path, Path::new("/nonexistent/fixtures"))
        .expect_err("missing fixtures dir");
    assert!(err.to_string().contains("policy"), "{err}");
}

#[test]
fn test_run_fixtures_empty_dir_is_ok() {
    let policy_dir = tempfile::tempdir().expect("tempdir");
    let policy_path = policy_dir.path().join("policy.yaml");
    fs::write(&policy_path, "version: 1\n").expect("write policy");
    let fx = tempfile::tempdir().expect("tempdir");
    let summary = run_fixtures(&policy_path, fx.path()).expect("run");
    assert_eq!(summary.total, 0);
    assert_eq!(summary.passed, 0);
    assert_eq!(summary.failures, Vec::<String>::new());
}

/// The committed example policy and fixture set stay in sync: the fixture
/// runner must pass the shipped assets.
#[test]
fn test_committed_example_policy_and_fixtures_pass() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let policy = root.join("guardian-policy.example.yaml");
    let fixtures = root.join("tests/fixtures/policy");
    let summary = run_fixtures(&policy, &fixtures).expect("run");
    assert_eq!(summary.total, 4);
    assert_eq!(summary.passed, 4);
    assert_eq!(summary.failures, Vec::<String>::new());
}
