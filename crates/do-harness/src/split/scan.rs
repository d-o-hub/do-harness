//! Brace-balanced line scanning for `do-harness split`.
//!
//! A line-based walk that knows enough Rust lexical structure — string
//! literals, char literals, lifetimes, raw strings, and both comment forms —
//! to count braces that actually nest code. Deliberately not an AST: `split`
//! needs item boundaries and item kinds, which column-0 keywords plus balanced
//! braces give reliably for the `cargo fmt`-normalized sources this repository
//! enforces. Anything the walk cannot classify is refused rather than guessed.

use anyhow::{Result, bail};

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

/// Net brace delta for one line plus whether an opening brace appeared,
/// ignoring braces inside strings, chars, raw strings, and comments.
fn line_scan(line: &str, block_depth: &mut usize) -> (i32, bool) {
    let bytes = line.as_bytes();
    let mut index = 0;
    let mut delta = 0i32;
    let mut saw_open = false;
    while index < bytes.len() {
        if *block_depth > 0 {
            if bytes[index..].starts_with(b"/*") {
                *block_depth += 1;
                index += 2;
            } else if bytes[index..].starts_with(b"*/") {
                *block_depth -= 1;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        match bytes[index] {
            b'/' if bytes[index..].starts_with(b"//") => break,
            b'/' if bytes[index..].starts_with(b"/*") => {
                *block_depth += 1;
                index += 2;
            }
            b'{' => {
                delta += 1;
                saw_open = true;
                index += 1;
            }
            b'}' => {
                delta -= 1;
                index += 1;
            }
            b'"' => index = skip_string(bytes, index),
            b'\'' => index = skip_char(bytes, index),
            b'r' | b'b' | b'c' => match crate::split::lex::skip_raw_string(bytes, index) {
                Some(next) => index = next,
                None => index += 1,
            },
            _ => index += 1,
        }
    }
    (delta, saw_open)
}

/// Strips a trailing line comment; block comments are left to [`line_scan`].
fn strip_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => index = skip_string(bytes, index),
            b'/' if bytes[index..].starts_with(b"//") => return &line[..index],
            _ => index += 1,
        }
    }
    line
}

/// Index just past the closing quote of a `"…"` literal opened at `open`.
fn skip_string(bytes: &[u8], open: usize) -> usize {
    let mut index = open + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'"' => return index + 1,
            _ => index += 1,
        }
    }
    index
}

/// Index just past a char literal at `quote`, or past the `'` of a lifetime.
fn skip_char(bytes: &[u8], quote: usize) -> usize {
    if bytes.get(quote + 1) == Some(&b'\\') {
        let mut index = quote + 2;
        while index < bytes.len() && bytes[index] != b'\'' {
            index += 1;
        }
        return (index + 1).min(bytes.len());
    }
    if bytes.get(quote + 2) == Some(&b'\'') {
        return quote + 3;
    }
    quote + 1
}
