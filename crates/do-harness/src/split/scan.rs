//! Top-level item discovery for `do-harness split`.
//!
//! Finds item boundaries and item kinds: column-0 keywords give the kind, and
//! balanced braces (from [`super::lex`]) give the span. Deliberately not an
//! AST — `cargo fmt`-normalized sources make the walk reliable, and anything it
//! cannot classify is refused rather than guessed.

use anyhow::{Result, bail};

use super::lex::{line_scan, strip_comment};

/// Visibility and item modifiers stripped before reading an item's keyword.
const ITEM_MODIFIERS: &[&str] = &[
    "pub(crate) ",
    "pub(super) ",
    "pub ",
    "unsafe ",
    "async ",
    "default ",
];

/// Item classes the splitter can move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    /// `fn` item.
    Fn,
    /// `impl` block.
    Impl,
    /// `mod` declaration or inline module.
    Mod,
    /// `struct` definition.
    Struct,
    /// `enum` definition.
    Enum,
    /// `trait` definition.
    Trait,
    /// `const` item.
    Const,
    /// `static` item.
    Static,
    /// `type` alias.
    Type,
    /// `macro_rules!` macro.
    MacroRules,
}

/// One top-level item with 0-based inclusive line bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopItem {
    /// First line of the item, including its attributes and doc comments.
    pub start: usize,
    /// Declaration line (`fn`/`impl`/`mod`/…).
    pub header: usize,
    /// Last line of the item.
    pub end: usize,
    /// Item class.
    pub kind: ItemKind,
}

/// Parses the item keyword at column 0, ignoring visibility and modifiers.
///
/// Indented lines are never top-level items, so nested items are attributed to
/// their enclosing owner.
#[must_use]
pub fn parse_kind(line: &str) -> Option<ItemKind> {
    if line.len() != line.trim_start().len() {
        return None;
    }
    if line.starts_with("macro_rules!") {
        return Some(ItemKind::MacroRules);
    }
    let mut rest = line;
    while let Some(modifier) = ITEM_MODIFIERS
        .iter()
        .find(|modifier| rest.starts_with(**modifier))
    {
        rest = &rest[modifier.len()..];
    }
    let keyword = rest
        .split(|character: char| character.is_whitespace() || character == '(' || character == '{')
        .next()
        .unwrap_or("");
    Some(match keyword {
        "fn" => ItemKind::Fn,
        "impl" => ItemKind::Impl,
        "mod" => ItemKind::Mod,
        "struct" => ItemKind::Struct,
        "enum" => ItemKind::Enum,
        "trait" => ItemKind::Trait,
        "const" => ItemKind::Const,
        "static" => ItemKind::Static,
        "type" => ItemKind::Type,
        _ => return None,
    })
}

/// Collects every top-level item in `lines`, in source order.
///
/// # Errors
///
/// Returns an error when an item's braces never balance, which means the
/// line-based heuristic cannot trust the boundaries it derived.
pub fn scan_top_items(lines: &[&str]) -> Result<Vec<TopItem>> {
    let mut items = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let Some(kind) = parse_kind(lines[index]) else {
            index += 1;
            continue;
        };
        let end = scan_item_end(lines, index).ok_or_else(|| {
            anyhow::anyhow!(
                "unbalanced braces in the item starting at line {} (`{}`)",
                index + 1,
                lines[index].trim()
            )
        })?;
        let mut start = index;
        while start > 0 && is_leading_attribute(lines[start - 1]) {
            start -= 1;
        }
        items.push(TopItem {
            start,
            header: index,
            end,
            kind,
        });
        index = end + 1;
    }
    Ok(items)
}

/// Last line of the item declared at `header`, or `None` when unbalanced.
fn scan_item_end(lines: &[&str], header: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut block_depth = 0usize;
    let mut saw_open = false;
    for (offset, line) in lines[header..].iter().enumerate() {
        let (delta, opened) = line_scan(line, &mut block_depth);
        depth += delta;
        saw_open |= opened;
        if depth < 0 {
            return None;
        }
        if block_depth > 0 {
            continue;
        }
        if saw_open && depth == 0 {
            return Some(header + offset);
        }
        // Brace-less items (`const X: u32 = 1;`, `type T = U;`) end at `;`.
        if !saw_open && strip_comment(line).trim_end().ends_with(';') {
            return Some(header + offset);
        }
    }
    None
}

/// Column-0 attribute or doc-comment line belonging to the item below it.
fn is_leading_attribute(line: &str) -> bool {
    line.len() == line.trim_start().len() && (line.starts_with("#[") || line.starts_with("///"))
}

/// Refuses files whose shape cannot be moved without changing meaning.
///
/// # Errors
///
/// Returns an error naming the construct that makes the split unsafe.
pub fn refuse_unsupported(lines: &[&str]) -> Result<()> {
    if let Some(line) = lines
        .iter()
        .find(|line| line.trim_start().starts_with("macro_rules!"))
    {
        bail!(
            "refusing to split: a moved `macro_rules!` loses its textual scope (`{}`)",
            line.trim()
        );
    }
    if let Some(line) = lines
        .iter()
        .find(|line| line.contains("include!(") || line.contains("include_str!("))
    {
        bail!(
            "refusing to split: `include!`/`include_str!` paths resolve against the file (`{}`)",
            line.trim()
        );
    }
    if let Some(line) = lines.iter().find(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("#[cfg(") && !trimmed.starts_with("#[cfg(test)]")
    }) {
        bail!(
            "refusing to split: conditional compilation beyond `#[cfg(test)]` (`{}`)",
            line.trim()
        );
    }
    Ok(())
}
