//! `do-harness split`: mechanical decomposition for the 500-LOC ceiling.
//!
//! When the `loc` sensor fires, the split point is usually obvious but the
//! edit is fiddly: extract the inline `#[cfg(test)]` module, or move the
//! largest top-level item into a sibling module, then wire the declaration
//! back. This command performs that edit deterministically and prints the
//! plan first, so the agent applies a known transformation instead of
//! guessing one.
//!
//! Two strategies, in order:
//!
//! 1. **Test extraction** — a trailing top-level `#[cfg(test)] mod …` moves to
//!    `<stem>_tests.rs`, replaced by `#[path = …] mod …;`. The path attribute
//!    keeps `super` pointing at the original module, so `use super::*;` in the
//!    test body and every test name keep working untouched. This is the
//!    dominant case for near-limit files and the transformation distilled in
//!    `harness/references/heuristics.md` (trace 19).
//! 2. **Item extraction** — otherwise the largest movable top-level item moves
//!    to `<target>.rs`, gains `pub(crate)` when private, and is re-exported so
//!    callers still resolve the name.
//!
//! Parsing is a line-based, brace-balanced heuristic with string, char, raw
//! string, and comment awareness — not an AST. When the heuristic cannot see a
//! safe transformation it refuses with a diagnostic instead of writing code
//! that does not compile.

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::loc;

mod lex;
mod names;
mod scan;

use names::{item_name, raise_visibility, snake_case, validate_ident, visibility_of};
use scan::{ItemKind, TopItem, scan_top_items};

/// Which transformation a plan performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitKind {
    /// The inline test module moved to a sibling file.
    Tests,
    /// A top-level item moved to a sibling module.
    Item,
}

/// A fully-resolved transformation: every line that will be written.
#[derive(Debug, Clone, PartialEq)]
pub struct SplitPlan {
    /// Repository-relative source path.
    pub source: String,
    /// Sibling file that receives the extracted text, relative to the root.
    pub target_file: String,
    /// Complete contents of the sibling file.
    pub target_text: String,
    /// Lines written into the source in place of `span`.
    pub declaration: Vec<String>,
    /// Replaced source span, 1-based and inclusive.
    pub span: (usize, usize),
    /// What moves, for reporting (`mod tests`, `fn render_…`).
    pub what: String,
    /// Source line count before the split.
    pub source_lines: usize,
    /// Source line count after the split.
    pub result_lines: usize,
    /// Which strategy produced the plan.
    pub kind: SplitKind,
}

/// Reads the source, plans the split, and (unless `dry_run`) applies it.
///
/// # Errors
///
/// Returns an error when the path is outside `crates/`, is not readable, or
/// the file has no transformation the heuristic can apply safely.
pub fn run(root: &Path, file: &Path, dry_run: bool, target: Option<&str>) -> Result<()> {
    let path = if file.is_absolute() {
        file.to_path_buf()
    } else {
        root.join(file)
    };
    if !loc::in_scope(root, &path) {
        bail!(
            "split only operates on Rust sources under {}: {}",
            loc::SCOPE_DIRS
                .iter()
                .map(|dir| format!("{dir}/"))
                .collect::<Vec<_>>()
                .join(" or "),
            path.display()
        );
    }
    if path.extension().is_none_or(|ext| ext != "rs") {
        bail!("split operates on .rs files: {}", path.display());
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let lines: Vec<&str> = text.lines().collect();
    let source = loc::display_path(root, &path);

    if lines.len() <= loc::MAX_LINES {
        println!(
            "{source} is {} lines: already under the {}-line ceiling.",
            lines.len(),
            loc::MAX_LINES
        );
        return Ok(());
    }

    let plan = build_plan(root, &path, &lines, target)?;
    print_plan(&plan);
    if plan.result_lines > loc::MAX_LINES {
        println!(
            "NOTE: still {} line(s) over the ceiling; run split again on the result.",
            plan.result_lines - loc::MAX_LINES
        );
    }
    if dry_run {
        println!("DRY RUN: no files written.");
        return Ok(());
    }
    apply(root, &path, &lines, &plan)?;
    println!("WROTE: {} and {}", plan.source, plan.target_file);
    println!("NEXT: cargo fmt --all && cargo check --workspace (fix any now-unused imports)");
    Ok(())
}

/// Builds the plan for an over-limit file: tests first, then the largest item.
fn build_plan(root: &Path, path: &Path, lines: &[&str], target: Option<&str>) -> Result<SplitPlan> {
    let items = scan_top_items(lines)?;
    if let Some(plan) = plan_tests(root, path, lines, &items, target)? {
        return Ok(plan);
    }
    plan_item(root, path, lines, &items, target)
}

/// Writes the sibling file, then replaces the span in the source.
fn apply(root: &Path, path: &Path, lines: &[&str], plan: &SplitPlan) -> Result<()> {
    let target_abs = root.join(&plan.target_file);
    if target_abs.exists() {
        bail!(
            "{} already exists; move or delete it first",
            plan.target_file
        );
    }
    std::fs::write(&target_abs, &plan.target_text)
        .with_context(|| format!("failed to write {}", target_abs.display()))?;

    let (start, end) = (plan.span.0 - 1, plan.span.1 - 1);
    let mut rewritten: Vec<&str> = lines[..start].to_vec();
    rewritten.extend(plan.declaration.iter().map(String::as_str));
    rewritten.extend(lines[end + 1..].iter().copied());
    std::fs::write(path, format!("{}\n", rewritten.join("\n")))
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

// --- strategy 1: trailing inline test module ------------------------------

/// Plans extraction of a trailing inline `#[cfg(test)]` module.
///
/// Returns `Ok(None)` when the shape does not apply (no trailing test module,
/// or removing it would not bring the file under the ceiling), leaving the
/// caller to try item extraction.
fn plan_tests(
    root: &Path,
    path: &Path,
    lines: &[&str],
    items: &[TopItem],
    target: Option<&str>,
) -> Result<Option<SplitPlan>> {
    let Some(marker) = loc::test_marker_line(&lines.join("\n")) else {
        return Ok(None);
    };
    // The marker is 1-based; `item.start` is 0-based, so the item begins at
    // `marker - 1` when the attribute is its first line.
    let item = items
        .iter()
        .find(|item| item.kind == ItemKind::Mod && item.start == marker - 1);
    let Some(item) = item else {
        return Ok(None);
    };
    if items.iter().any(|other| other.start > item.end) {
        return Ok(None); // trailing siblings would change meaning if reordered
    }
    let Some(module) = item_name(lines[item.header], ItemKind::Mod) else {
        return Ok(None);
    };
    let (target_file, module) = match target {
        Some(name) => (format!("{name}.rs"), validate_ident(name)?.to_owned()),
        None => (format!("{}.rs", stem_for(path)), module),
    };
    let body = names::dedent(&lines[item.header + 1..item.end]);
    let target_text = render_target(
        &format!(
            "//! Tests for `{}`, split out to keep that file under the {}-line ceiling.",
            loc::display_path(root, path),
            loc::MAX_LINES
        ),
        &body,
    );
    let mut declaration: Vec<String> = lines[item.start..item.header]
        .iter()
        .map(|line| (*line).to_owned())
        .collect();
    declaration.push(format!("#[path = \"{target_file}\"]"));
    declaration.push(format!("mod {module};"));

    let source_lines = lines.len();
    let result_lines = source_lines - (item.end - item.start + 1) + declaration.len();
    if result_lines > loc::MAX_LINES {
        return Ok(None); // removing the tests is not enough; the item path may be
    }
    Ok(Some(SplitPlan {
        source: loc::display_path(root, path),
        target_file: sibling(root, path, &target_file),
        target_text,
        declaration,
        span: (item.start + 1, item.end + 1),
        what: format!("mod {module}"),
        source_lines,
        result_lines,
        kind: SplitKind::Tests,
    }))
}

// --- strategy 2: largest movable top-level item ---------------------------

/// Plans extraction of the largest movable top-level item.
fn plan_item(
    root: &Path,
    path: &Path,
    lines: &[&str],
    items: &[TopItem],
    target: Option<&str>,
) -> Result<SplitPlan> {
    scan::refuse_unsupported(lines)?;
    let test_marker = loc::test_marker_line(&lines.join("\n")).map(|line| line - 1);
    let candidates: Vec<&TopItem> = items
        .iter()
        .filter(|item| item.kind != ItemKind::Mod)
        .filter(|item| !spans(item, test_marker))
        // Both halves must end up under the ceiling: moving an item that is
        // itself over-limit (or too small to matter) only relocates the
        // violation, so those candidates are not plans at all.
        .filter(|item| fits_both(lines.len(), item))
        .collect();
    if candidates.is_empty() {
        bail!(
            "no single top-level item in {} both fits the {}-line ceiling and brings the file \
             under it: decompose by hand, or extract the inline test module first",
            loc::display_path(root, path),
            loc::MAX_LINES
        );
    }
    // Largest feasible span wins; the earlier item wins ties, so plans are
    // deterministic and a single run leaves both files compliant.
    let Some(item) = candidates
        .iter()
        .copied()
        .max_by_key(|item| (item.end - item.start, std::cmp::Reverse(item.start)))
    else {
        bail!(
            "no feasible top-level item in {}",
            loc::display_path(root, path)
        );
    };

    let module = if let Some(name) = target {
        validate_ident(name)?.to_owned()
    } else {
        let name = item_name(lines[item.header], item.kind).with_context(|| {
            format!(
                "cannot derive a module name from `{}`; pass --target <name>",
                lines[item.header].trim()
            )
        })?;
        snake_case(&name)
    };
    let mut moved: Vec<String> = lines[item.start..=item.end]
        .iter()
        .map(|line| (*line).to_owned())
        .collect();
    let offset = item.header - item.start;
    moved[offset] = raise_visibility(lines[item.header], item.kind);
    let target_file = format!("{module}.rs");
    let target_text = render_target(
        &format!(
            "//! `{module}` split out of `{}` to keep that file under the {}-line ceiling.",
            loc::display_path(root, path),
            loc::MAX_LINES
        ),
        &moved,
    );

    let mut declaration = vec![
        format!("#[path = \"{target_file}\"]"),
        format!("mod {module};"),
    ];
    if item.kind != ItemKind::Impl {
        let name = item_name(lines[item.header], item.kind)
            .with_context(|| format!("cannot re-export `{}`", lines[item.header].trim()))?;
        declaration.push(format!(
            "{}use {module}::{name};",
            visibility_of(lines[item.header])
        ));
    }
    let result_lines = lines.len() - (item.end - item.start + 1) + declaration.len();
    Ok(SplitPlan {
        source: loc::display_path(root, path),
        target_file: sibling(root, path, &target_file),
        target_text,
        declaration,
        span: (item.start + 1, item.end + 1),
        what: lines[item.header].trim().to_owned(),
        source_lines: lines.len(),
        result_lines,
        kind: SplitKind::Item,
    })
}

// --- rendering ------------------------------------------------------------

/// True when 0-based `line` falls inside `item`.
fn spans(item: &TopItem, line: Option<usize>) -> bool {
    line.is_some_and(|line| item.start <= line && line <= item.end)
}

/// Lines [`render_target`] adds around the moved text: the module doc comment,
/// a blank line, the `#[allow]` attribute, the `use super::*;` glob, and a
/// blank line.
const TARGET_OVERHEAD_LINES: usize = 5;

/// Most declaration lines an item plan can emit: `#[path]`, `mod`, `use`.
const MAX_DECLARATION_LINES: usize = 3;

/// True when moving `item` leaves *both* files under the ceiling.
///
/// A move that leaves either half over-limit only relocates the violation, so
/// such an item is not a candidate. Uses the largest declaration the builder
/// can emit, so the filter never admits a plan the builder would reject.
fn fits_both(total_lines: usize, item: &TopItem) -> bool {
    let span = item.end - item.start + 1;
    total_lines - span + MAX_DECLARATION_LINES <= loc::MAX_LINES
        && span + TARGET_OVERHEAD_LINES <= loc::MAX_LINES
}

/// Sibling path of `file` inside its own directory, relative to the root.
fn sibling(root: &Path, file: &Path, name: &str) -> String {
    let dir = file.parent().unwrap_or(file);
    loc::display_path(root, &dir.join(name))
}

/// File stem for the `<stem>_tests.rs` name; `mod.rs` maps to `tests.rs`.
fn stem_for(path: &Path) -> String {
    match path.file_stem().and_then(|stem| stem.to_str()) {
        None | Some("mod") => "tests".to_owned(),
        Some(stem) => format!("{stem}_tests"),
    }
}

/// Renders a sibling module: doc header, any leading inner attributes, the
/// `super` glob when the body needs one, then the moved text.
///
/// Inner attributes (`#![…]`) must come first in the file, so a body that
/// opens with them keeps them at the top and the glob follows. Emitting the
/// glob first would place an inner attribute after an item, which does not
/// compile.
fn render_target(header: &str, body: &[String]) -> String {
    let attrs = body
        .iter()
        .take_while(|line| {
            let trimmed = line.trim_start();
            trimmed.is_empty() || trimmed.starts_with("#![")
        })
        .count();
    let mut text = String::from(header);
    text.push('\n');
    for line in &body[..attrs] {
        text.push_str(line);
        text.push('\n');
    }
    if !body
        .iter()
        .any(|line| line.trim_start().starts_with("use super"))
    {
        // The moved text may name items that stayed behind. `#[allow]` keeps a
        // mechanical move from failing the workspace `-D warnings` gate when it
        // happens to reference nothing in the parent.
        text.push_str("#[allow(unused_imports)]\nuse super::*;\n");
    }
    text.push('\n');
    for line in &body[attrs..] {
        text.push_str(line);
        text.push('\n');
    }
    text
}

/// Prints the plan in a stable, greppable shape.
fn print_plan(plan: &SplitPlan) {
    let removed = plan.source_lines - plan.result_lines;
    println!(
        "SPLIT: {} {} -> {} lines",
        plan.source, plan.source_lines, plan.result_lines
    );
    println!(
        "  EXTRACT: {} (source lines {}..{}, {} line(s) removed)",
        plan.what, plan.span.0, plan.span.1, removed
    );
    println!(
        "  TARGET:  {} ({} lines)",
        plan.target_file,
        plan.target_text.lines().count()
    );
    for line in &plan.declaration {
        println!("  REPLACE WITH: {line}");
    }
}

#[cfg(test)]
#[path = "split_tests.rs"]
mod split_tests;
