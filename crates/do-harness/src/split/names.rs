//! Identifier, visibility, and text-shaping helpers for `do-harness split`.
//!
//! Module naming, re-export visibility, and body dedenting are decisions about
//! text, not about item boundaries, so they live apart from the scanner.

use anyhow::{Result, bail};

use super::scan::ItemKind;

/// Visibility and item modifiers stripped before reading an item's name.
const ITEM_MODIFIERS: &[&str] = &[
    "pub(crate) ",
    "pub(super) ",
    "pub ",
    "unsafe ",
    "async ",
    "default ",
];

/// Name declared by an item, used for module naming and re-exporting.
#[must_use]
pub fn item_name(line: &str, kind: ItemKind) -> Option<String> {
    if kind == ItemKind::Impl {
        return impl_target(line);
    }
    let mut rest = line;
    while let Some(modifier) = ITEM_MODIFIERS
        .iter()
        .find(|modifier| rest.starts_with(**modifier))
    {
        rest = &rest[modifier.len()..];
    }
    let after = rest.split_once([' ', '(']).map_or("", |(_, tail)| tail);
    leading_ident(after)
}

/// Type an `impl` block belongs to, preferring the trait's target.
fn impl_target(line: &str) -> Option<String> {
    let rest = line.strip_prefix("impl")?.trim_start();
    let rest = skip_generics(rest);
    match rest.split_once(" for ") {
        Some((_, target)) => leading_ident(target),
        None => leading_ident(rest),
    }
}

/// Drops a leading `<…>` parameter list, if present.
fn skip_generics(text: &str) -> &str {
    if let Some(stripped) = text.strip_prefix('<') {
        if let Some(close) = stripped.find('>') {
            return stripped[close + 1..].trim_start();
        }
    }
    text
}

/// First identifier in `text`.
fn leading_ident(text: &str) -> Option<String> {
    let name: String = text
        .trim_start()
        .chars()
        .take_while(|character| character.is_alphanumeric() || *character == '_')
        .collect();
    (!name.is_empty()).then_some(name)
}

/// Visibility prefix to re-export a moved item with.
#[must_use]
pub fn visibility_of(line: &str) -> &'static str {
    if line.starts_with("pub ") {
        "pub "
    } else {
        "pub(crate) "
    }
}

/// Adds `pub(crate)` to a private movable item; other items are unchanged.
///
/// `impl` blocks have no visibility, and `mod` is never moved.
#[must_use]
pub fn raise_visibility(line: &str, kind: ItemKind) -> String {
    if kind == ItemKind::Impl
        || kind == ItemKind::Mod
        || line.starts_with("pub ")
        || line.starts_with("pub(")
    {
        return line.to_owned();
    }
    format!("pub(crate) {line}")
}

/// Validates a user-supplied module name.
///
/// # Errors
///
/// Returns an error when the name is not an ASCII identifier or is a keyword.
pub fn validate_ident(name: &str) -> Result<&str> {
    let valid = !name.is_empty()
        && !name.starts_with(|character: char| character.is_ascii_digit())
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
        && !is_keyword(name);
    if !valid {
        bail!("--target {name:?} is not a usable module name (ASCII identifier, not a keyword)");
    }
    Ok(name)
}

/// Rust keywords rejected as module names.
fn is_keyword(name: &str) -> bool {
    matches!(
        name,
        "as" | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
    )
}

/// `CamelCase` and `SCREAMING_SNAKE` to `snake_case`.
#[must_use]
pub fn snake_case(name: &str) -> String {
    let characters: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 4);
    for (index, character) in characters.iter().enumerate() {
        if character.is_uppercase() && index > 0 {
            let previous = characters[index - 1];
            let boundary = previous.is_lowercase()
                || previous.is_numeric()
                || (previous.is_uppercase()
                    && characters
                        .get(index + 1)
                        .is_some_and(|next| next.is_lowercase()));
            if boundary {
                out.push('_');
            }
        }
        out.extend(character.to_lowercase());
    }
    out
}

/// Removes the common leading indentation from `lines`.
#[must_use]
pub fn dedent(lines: &[&str]) -> Vec<String> {
    let indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    let prefix = " ".repeat(indent);
    lines
        .iter()
        .map(|line| line.strip_prefix(&prefix).unwrap_or(line).to_owned())
        .collect()
}
