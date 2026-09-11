//! Unified-diff parsing into reviewable units.
//!
//! One changed hunk is one unit. Units carry git's deterministic function
//! anchor (`path:symbol`) and the payload lines a reviewer needs; parse
//! problems become warnings and the affected hunk stays residual.

use serde::{Deserialize, Serialize};

/// Synthetic unit header for pure renames.
pub const HEADER_RENAME_ONLY: &str = "rename-only";
/// Synthetic unit header for mode-only changes.
pub const HEADER_MODE_CHANGE: &str = "mode change";

/// File-level change kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Change {
    /// File added.
    Added,
    /// File modified.
    Modified,
    /// File deleted.
    Deleted,
    /// File renamed, possibly with content changes.
    Renamed,
}

/// One changed hunk: the atomic unit a reviewer sees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unit {
    /// Stable unit identity (`path:new_start`, deduplicated).
    pub id: String,
    /// Repository-relative path after the change.
    pub path: String,
    /// File-level change kind.
    pub change: Change,
    /// First line in the base file (0 when unknown).
    pub old_start: u64,
    /// First line in the head file (0 when unknown).
    pub new_start: u64,
    /// Git function-context anchor, when the diff carries one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    /// Payload lines: context, additions (`+`), deletions (`-`).
    pub lines: Vec<String>,
}

/// Parsed diff plus non-fatal diagnostics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parsed {
    /// Units in diff order.
    pub units: Vec<Unit>,
    /// Parse diagnostics; affected regions stay residual.
    pub warnings: Vec<String>,
}

#[derive(Debug, Default)]
#[allow(clippy::struct_excessive_bools)]
struct FileState {
    started: bool,
    path: Option<String>,
    change: Option<Change>,
    old_null: bool,
    new_null: bool,
    binary: bool,
    mode_changed: bool,
    added_marker: bool,
    deleted_marker: bool,
    saw_hunk: bool,
}

#[derive(Debug)]
struct Pending {
    old_start: u64,
    new_start: u64,
    header: Option<String>,
    lines: Vec<String>,
}

/// Parses unified-diff text into residual units.
#[must_use]
pub fn parse(diff: &str) -> Parsed {
    let mut out = Parsed::default();
    let mut file = FileState::default();
    let mut pending: Option<Pending> = None;

    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            finish_file(&mut file, &mut pending, &mut out);
            file.started = true;
            file.path = diff_git_path(rest);
            continue;
        }
        if line.starts_with("@@") {
            finish_hunk(&mut file, &mut pending, &mut out);
            pending = Some(start_hunk(line, &mut out));
            continue;
        }
        if let Some(hunk) = pending.as_mut() {
            if is_payload(line) {
                hunk.lines.push(line.to_owned());
                continue;
            }
            finish_hunk(&mut file, &mut pending, &mut out);
            if line.is_empty() {
                continue;
            }
        }
        apply_meta(line, &mut file);
    }
    finish_file(&mut file, &mut pending, &mut out);
    out
}

/// Whether `line` belongs to a hunk payload.
fn is_payload(line: &str) -> bool {
    matches!(line.bytes().next(), Some(b' ' | b'+' | b'-' | b'\\'))
}

/// Starts a pending hunk from an `@@` header, recording parse failures.
fn start_hunk(line: &str, out: &mut Parsed) -> Pending {
    parse_hunk_header(line).map_or_else(
        || {
            out.warnings
                .push(format!("unparsed hunk header kept as residual: {line}"));
            Pending {
                old_start: 0,
                new_start: 0,
                header: None,
                lines: Vec::new(),
            }
        },
        |(old_start, new_start, header)| Pending {
            old_start,
            new_start,
            header,
            lines: Vec::new(),
        },
    )
}

/// Materializes the pending hunk as a unit.
fn finish_hunk(file: &mut FileState, pending: &mut Option<Pending>, out: &mut Parsed) {
    let Some(hunk) = pending.take() else {
        return;
    };
    file.saw_hunk = true;
    let path = file.path.clone().unwrap_or_else(|| {
        out.warnings
            .push("hunk without a file path kept as residual".to_owned());
        "<unknown>".to_owned()
    });
    let change = resolve_change(file);
    let id = unique_id(out, &path, hunk.new_start);
    out.units.push(Unit {
        id,
        path,
        change,
        old_start: hunk.old_start,
        new_start: hunk.new_start,
        header: hunk.header,
        lines: hunk.lines,
    });
}

/// Emits a synthetic unit for file changes without hunks (binary, mode,
/// empty add/delete, rename-only); resets state for the next file.
fn finish_file(file: &mut FileState, pending: &mut Option<Pending>, out: &mut Parsed) {
    finish_hunk(file, pending, out);
    if !file.started {
        return;
    }
    let state = std::mem::take(file);
    if state.saw_hunk {
        return;
    }
    let change = resolve_change(&state);
    let Some(path) = state.path else {
        out.warnings
            .push("diff section without a path kept as residual".to_owned());
        return;
    };
    let reason = if state.binary {
        "binary"
    } else if state.added_marker || state.deleted_marker {
        "empty file"
    } else if state.change == Some(Change::Renamed) {
        HEADER_RENAME_ONLY
    } else if state.mode_changed {
        HEADER_MODE_CHANGE
    } else {
        return;
    };
    out.units.push(Unit {
        id: unique_id(out, &path, 0),
        path,
        change,
        old_start: 0,
        new_start: 0,
        header: Some(reason.to_owned()),
        lines: Vec::new(),
    });
}

/// Resolves the file-level change kind from the markers seen so far.
fn resolve_change(file: &FileState) -> Change {
    if let Some(change) = file.change {
        return change;
    }
    if file.added_marker || file.old_null {
        Change::Added
    } else if file.deleted_marker || file.new_null {
        Change::Deleted
    } else {
        Change::Modified
    }
}

/// Applies a non-hunk metadata line to the current file state.
fn apply_meta(line: &str, file: &mut FileState) {
    if let Some(rest) = line.strip_prefix("--- ") {
        if rest == "/dev/null" {
            file.old_null = true;
        } else if file.path.is_none() {
            file.path = line_path(rest);
        }
        return;
    }
    if let Some(rest) = line.strip_prefix("+++ ") {
        if rest == "/dev/null" {
            file.new_null = true;
        } else if let Some(path) = line_path(rest) {
            file.path = Some(path);
        }
        return;
    }
    if let Some(rest) = line.strip_prefix("rename to ") {
        file.path = Some(rest.to_owned());
        file.change = Some(Change::Renamed);
        return;
    }
    if line.starts_with("rename from ") {
        file.change = Some(Change::Renamed);
        return;
    }
    if line.starts_with("new file mode ") {
        file.added_marker = true;
        return;
    }
    if line.starts_with("deleted file mode ") {
        file.deleted_marker = true;
        return;
    }
    if line.starts_with("old mode ") || line.starts_with("new mode ") {
        file.mode_changed = true;
        return;
    }
    if line.starts_with("Binary files ") || line == "GIT binary patch" {
        file.binary = true;
    }
}

/// Parses a `--- a/path` / `+++ b/path` token into a repository path.
fn line_path(token: &str) -> Option<String> {
    let (raw, _) = unquote(token)?;
    let path = raw
        .strip_prefix("a/")
        .or_else(|| raw.strip_prefix("b/"))
        .unwrap_or(&raw);
    (!path.is_empty()).then(|| path.to_owned())
}

/// Parses `diff --git a/from b/to` into `to`.
fn diff_git_path(rest: &str) -> Option<String> {
    if rest.starts_with('"') {
        let (from_raw, used) = unquote(rest)?;
        let after = rest[used..].trim_start();
        let (to_raw, _) = unquote(after)?;
        let to = to_raw.strip_prefix("b/").unwrap_or(&to_raw);
        if to.is_empty() {
            let from = from_raw.strip_prefix("a/").unwrap_or(&from_raw);
            return (!from.is_empty()).then(|| from.to_owned());
        }
        return Some(to.to_owned());
    }
    let index = rest.find(" b/")?;
    let from = rest[..index].strip_prefix("a/").unwrap_or(&rest[..index]);
    let to = rest[index + 1..]
        .strip_prefix("b/")
        .unwrap_or(&rest[index + 1..]);
    if to.is_empty() {
        return (!from.is_empty()).then(|| from.to_owned());
    }
    Some(to.to_owned())
}

/// Decodes a possibly quoted token, returning `(value, bytes consumed)`.
fn unquote(token: &str) -> Option<(String, usize)> {
    if !token.starts_with('"') {
        return (!token.is_empty()).then(|| (token.to_owned(), token.len()));
    }
    let mut out = String::new();
    let mut chars = token.char_indices().skip(1);
    while let Some((index, ch)) = chars.next() {
        match ch {
            '"' => return Some((out, index + 1)),
            '\\' => {
                let (_, escaped) = chars.next()?;
                out.push(match escaped {
                    'n' => '\n',
                    't' => '\t',
                    '\\' => '\\',
                    '"' => '"',
                    other => other,
                });
            }
            other => out.push(other),
        }
    }
    None
}

/// Parses `@@ -old +new @@ anchor`; `None` when the header is malformed.
fn parse_hunk_header(line: &str) -> Option<(u64, u64, Option<String>)> {
    let rest = line.strip_prefix("@@")?.strip_prefix(' ')?;
    let (ranges, header) = match rest.find(" @@") {
        Some(index) => (&rest[..index], Some(rest[index + 3..].trim())),
        None => (rest, None),
    };
    let mut parts = ranges.split_whitespace();
    let old = first_number(parts.next()?, '-')?;
    let new = first_number(parts.next()?, '+')?;
    let header = header.filter(|value| !value.is_empty()).map(str::to_owned);
    Some((old, new, header))
}

/// Reads the start line from a `-12,3` / `+0,0` range specifier.
fn first_number(spec: &str, sign: char) -> Option<u64> {
    spec.strip_prefix(sign)?
        .split(',')
        .next()?
        .parse::<u64>()
        .ok()
}

/// Builds a stable, collision-free unit id.
fn unique_id(out: &Parsed, path: &str, new_start: u64) -> String {
    let base = format!("{path}:{new_start}");
    if !out.units.iter().any(|unit| unit.id == base) {
        return base;
    }
    let mut suffix = 2;
    loop {
        let candidate = format!("{base}#{suffix}");
        if !out.units.iter().any(|unit| unit.id == candidate) {
            return candidate;
        }
        suffix += 1;
    }
}
