//! Guard: everything the scaffolded skills point at must exist for an adopter.
//!
//! An adopting repository has no do-harness checkout, no development-methodology
//! skills, and no workspace crates. A path or command the scaffolded guidance
//! names but `init` never writes sends the agent chasing something that is not
//! there, so both are checked against the initialized tree and the CLI itself.

use std::path::Path;

use clap::CommandFactory;

use super::super::*;
use super::opts;

/// Every markdown file `init` scaffolds under `.agents/skills`, with the text.
fn scaffolded_skill_docs(root: &Path) -> Vec<(String, String)> {
    let mut docs = Vec::new();
    let mut pending = vec![root.join(".agents/skills")];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().is_some_and(|ext| ext == "md") {
                let rel = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                docs.push((rel, fs::read_to_string(&path).unwrap()));
            }
        }
    }
    docs.sort();
    assert!(!docs.is_empty(), "init scaffolded no skill documents");
    docs
}

/// Backticked spans in `text` — markdown's spelling of a literal path.
fn code_spans(text: &str) -> Vec<String> {
    let mut spans = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('`') {
        rest = &rest[start + 1..];
        match rest.find('`') {
            Some(end) => {
                spans.push(rest[..end].to_owned());
                rest = &rest[end + 1..];
            }
            None => break,
        }
    }
    spans
}

/// A `.agents/...` path named by `span`, truncated at its first placeholder.
///
/// `.agents/events/<YYYY>/<MM>/<DD>/` is a pattern, not a file, so only the
/// concrete prefix — `.agents/events/` — can be required to exist.
fn mentioned_agent_path(span: &str) -> Option<&str> {
    let start = span.find(".agents/")?;
    let rest = &span[start..];
    let end = rest
        .find(|c: char| c == '<' || c.is_whitespace() || c == '`')
        .unwrap_or(rest.len());
    Some(rest[..end].trim_end_matches(['.', ':', ',', ')']))
}

/// The `do-harness …` invocation a backticked span claims, if it is one.
fn mentioned_command(span: &str) -> Option<Vec<String>> {
    let normalized = span.split_whitespace().collect::<Vec<_>>().join(" ");
    let rest = normalized.trim().strip_prefix("do-harness ")?;
    let words: Vec<String> = rest
        .split(' ')
        .map(|word| word.trim_matches(|c: char| c == '`' || c == ',' || c == '.'))
        .filter(|word| !word.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    words.first()?;
    Some(words)
}

/// The signal sets the config `init` generated actually defines.
fn configured_sets(root: &Path) -> Vec<String> {
    let text = fs::read_to_string(root.join("do-harness.toml")).unwrap();
    let mut sets = Vec::new();
    let mut in_sets = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_sets = line.starts_with("[signal-sets");
            continue;
        }
        if in_sets {
            if let Some((key, _)) = line.split_once('=') {
                sets.push(key.trim().trim_matches('"').to_owned());
            }
        }
    }
    sets
}

/// Whether the CLI defines `path`, following subcommands at each step.
fn command_exists(path: &[String]) -> bool {
    let mut command = crate::cli::Cli::command();
    for name in path {
        let Some(next) = command
            .get_subcommands()
            .find(|sub| sub.get_name() == name)
            .cloned()
        else {
            return false;
        };
        command = next;
    }
    true
}

/// Whether the CLI defines `name` as a group with subcommands of its own.
fn command_takes_subcommand(name: &str) -> bool {
    crate::cli::Cli::command()
        .get_subcommands()
        .find(|sub| sub.get_name() == name)
        .is_some_and(|sub| sub.get_subcommands().next().is_some())
}

#[tokio::test(flavor = "current_thread")]
async fn scaffolded_skills_reference_only_paths_init_writes() {
    let dir = tempfile::tempdir().unwrap();
    init_workspace(dir.path(), &opts(Some(Language::Generic)))
        .await
        .unwrap();

    let mut checked = 0;
    for (doc, text) in scaffolded_skill_docs(dir.path()) {
        for span in code_spans(&text) {
            let Some(path) = mentioned_agent_path(&span) else {
                continue;
            };
            checked += 1;
            assert!(
                dir.path().join(path).exists(),
                "{doc} names `{path}`, which init does not write"
            );
        }
    }
    assert!(
        checked > 0,
        "no `.agents/...` path was extracted; the guard would pass vacuously"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn scaffolded_skills_name_only_real_commands() {
    let dir = tempfile::tempdir().unwrap();
    init_workspace(dir.path(), &opts(Some(Language::Generic)))
        .await
        .unwrap();

    let mut checked = 0;
    for (doc, text) in scaffolded_skill_docs(dir.path()) {
        for span in code_spans(&text) {
            let Some(words) = mentioned_command(&span) else {
                continue;
            };
            checked += 1;
            let command = &words[0];
            assert!(
                command_exists(std::slice::from_ref(command)),
                "{doc} names `do-harness {command}`, which is not a subcommand"
            );
            // A group is always followed by its verb; a flag or placeholder
            // after a leaf command (`--only fmt`, `<file>`) is not a path.
            if let Some(verb) = words.get(1) {
                if command_takes_subcommand(command)
                    && !verb.starts_with('-')
                    && !verb.starts_with('<')
                {
                    assert!(
                        command_exists(&[command.clone(), verb.clone()]),
                        "{doc} names `do-harness {command} {verb}`, which is not a subcommand path"
                    );
                }
            }
        }
    }
    assert!(
        checked > 5,
        "only {checked} command(s) extracted; the guard would pass vacuously"
    );
}

/// A `--set` naming a set the generated config does not define runs nothing, and
/// `verify` still exits 0, so the guidance would be silently vacuous.
#[tokio::test(flavor = "current_thread")]
async fn scaffolded_skills_name_only_configured_signal_sets() {
    let dir = tempfile::tempdir().unwrap();
    init_workspace(dir.path(), &opts(Some(Language::Rust)))
        .await
        .unwrap();

    let sets = configured_sets(dir.path());
    assert!(
        sets.contains(&"verification".to_owned()),
        "the rust pack configured no `verification` set: {sets:?}"
    );
    let mut checked = 0;
    for (doc, text) in scaffolded_skill_docs(dir.path()) {
        for span in code_spans(&text) {
            let Some(words) = mentioned_command(&span) else {
                continue;
            };
            let Some(index) = words.iter().position(|word| word == "--set") else {
                continue;
            };
            let Some(name) = words.get(index + 1) else {
                continue;
            };
            if name.starts_with('<') {
                continue;
            }
            checked += 1;
            assert!(
                sets.contains(name),
                "{doc} names `--set {name}`, which the generated config does not define: {sets:?}"
            );
        }
    }
    assert!(
        checked > 0,
        "no `--set` name was extracted; the guard would pass vacuously"
    );
}
