//! Lexical scanning for `do-harness split`.
//!
//! Split needs three things from Rust's lexical structure, and they all rest on
//! the same question — which bytes are code and which are literal or comment
//! text:
//!
//! - **Brace balance** ([`line_scan`]): a `{` inside `"…"`, `r#"…"#`, or a
//!   comment must not count toward an item's span.
//! - **Line comments** ([`strip_comment`]): a brace-less item ends at its `;`,
//!   which may sit behind a trailing comment.
//! - **Token interiors** ([`protected_line_starts`]): a move must never
//!   re-indent the inside of a multi-line token. Raw strings are the sharp
//!   edge — their delimiters are user-chosen (`r"…"`, `r#"…"#`, `br##"…"##`) —
//!   but a plain multi-line string or block comment has the same hazard, since
//!   `dedent` rewrites leading whitespace line by line.
//!
//! Deliberately not an AST: `cargo fmt`-normalized sources make a
//! brace-balanced walk reliable, and anything it cannot classify is refused by
//! the caller rather than guessed at.

/// Index just past a raw string starting at `index`, when one starts there.
///
/// Returns `None` at an ordinary identifier character, so `r` in `render` or
/// `borrow` is never mistaken for a prefix. An unterminated literal consumes
/// the rest of the input, matching how `rustc` reports the error the caller
/// sees anyway.
#[must_use]
pub fn skip_raw_string(bytes: &[u8], index: usize) -> Option<usize> {
    let (mut cursor, hashes) = raw_string_open(bytes, index)?;
    while cursor < bytes.len() {
        if bytes[cursor] == b'"' {
            let mut after = cursor + 1;
            let mut seen = 0usize;
            while seen < hashes && bytes.get(after) == Some(&b'#') {
                seen += 1;
                after += 1;
            }
            if seen == hashes {
                return Some(after);
            }
        }
        cursor += 1;
    }
    Some(bytes.len())
}

/// Splits a raw-string prefix into `(index after the opening quote, hashes)`.
///
/// `None` when no raw string opens at `index`, which covers both an ordinary
/// byte and an identifier that merely contains `r`.
fn raw_string_open(bytes: &[u8], index: usize) -> Option<(usize, usize)> {
    if index > 0 && is_ident_byte(bytes[index - 1]) {
        return None;
    }
    let mut cursor = index;
    if matches!(bytes.get(cursor), Some(b'b' | b'c')) {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'r') {
        return None;
    }
    cursor += 1;
    let mut hashes = 0usize;
    while bytes.get(cursor) == Some(&b'#') {
        hashes += 1;
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'"') {
        return None;
    }
    Some((cursor + 1, hashes))
}

/// Net brace delta for one line plus whether an opening brace appeared,
/// ignoring braces inside strings, chars, raw strings, and comments.
pub fn line_scan(line: &str, block_depth: &mut usize) -> (i32, bool) {
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
            b'r' | b'b' | b'c' => match skip_raw_string(bytes, index) {
                Some(next) => index = next,
                None => index += 1,
            },
            _ => index += 1,
        }
    }
    (delta, saw_open)
}

/// Strips a trailing line comment; block comments are left to [`line_scan`].
#[must_use]
pub fn strip_comment(line: &str) -> &str {
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

/// Per-line flags: `true` when the line *begins* inside a multi-line token
/// (a string, a raw string, or a block comment).
///
/// Such a line's leading whitespace is token content, not indentation, so
/// re-indenting it would silently rewrite the literal. Callers that reshape
/// source text use this to leave those lines byte-identical.
#[must_use]
pub fn protected_line_starts(text: &str) -> Vec<bool> {
    let bytes = text.as_bytes();
    let mut flags = vec![false];
    let mut index = 0;
    let mut block_depth = 0usize;
    let mut in_string = false;
    let mut raw_hashes: Option<usize> = None;
    while index < bytes.len() {
        // Checked first so a newline inside any open token still marks the
        // next line as protected.
        if bytes[index] == b'\n' {
            flags.push(block_depth > 0 || in_string || raw_hashes.is_some());
            index += 1;
            continue;
        }
        if let Some(hashes) = raw_hashes {
            if bytes[index] == b'"' {
                let mut after = index + 1;
                let mut seen = 0usize;
                while seen < hashes && bytes.get(after) == Some(&b'#') {
                    seen += 1;
                    after += 1;
                }
                if seen == hashes {
                    raw_hashes = None;
                    index = after;
                    continue;
                }
            }
            index += 1;
            continue;
        }
        if in_string {
            match bytes[index] {
                // A backslash-newline continuation must still reach the
                // newline branch above, or the line count drifts.
                b'\\' if bytes.get(index + 1) != Some(&b'\n') => index += 2,
                b'"' => {
                    in_string = false;
                    index += 1;
                }
                _ => index += 1,
            }
            continue;
        }
        if block_depth > 0 {
            if bytes[index..].starts_with(b"/*") {
                block_depth += 1;
                index += 2;
            } else if bytes[index..].starts_with(b"*/") {
                block_depth -= 1;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        match bytes[index] {
            b'/' if bytes[index..].starts_with(b"//") => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes[index..].starts_with(b"/*") => {
                block_depth = 1;
                index += 2;
            }
            b'"' => {
                in_string = true;
                index += 1;
            }
            b'\'' => index = skip_char(bytes, index),
            b'r' | b'b' | b'c' => match raw_string_open(bytes, index) {
                Some((content, hashes)) => {
                    raw_hashes = Some(hashes);
                    index = content;
                }
                None => index += 1,
            },
            _ => index += 1,
        }
    }
    flags
}

/// ASCII identifier byte.
fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn plain_raw_string_ends_at_its_quote() {
        let bytes = br#"let s = r"a{b}c"; tail"#;
        let start = 8;
        assert_eq!(bytes[start], b'r');
        let end = skip_raw_string(bytes, start).unwrap();
        assert_eq!(&bytes[end - 1..], b"\"; tail");
    }

    #[test]
    fn hashed_raw_string_requires_the_matching_hashes() {
        let bytes = br##"x = r#"a"b{c}"#; y"##;
        let start = 4;
        let end = skip_raw_string(bytes, start).unwrap();
        assert_eq!(bytes[end - 1], b'#', "consumed the closing hash");
        assert_eq!(bytes[end], b';');
    }

    #[test]
    fn byte_and_c_string_prefixes_are_handled() {
        assert!(skip_raw_string(br#"br"x""#, 0).is_some());
        assert!(skip_raw_string(br#"cr"x""#, 0).is_some());
    }

    #[test]
    fn identifiers_containing_r_are_not_prefixes() {
        assert_eq!(skip_raw_string(b"render()", 0), None);
        assert_eq!(skip_raw_string(b"let x = borrow;", 8), None);
        assert_eq!(skip_raw_string(b"1r", 1), None, "preceded by a digit");
    }

    #[test]
    fn line_scan_ignores_braces_in_literals_and_comments() {
        let mut depth = 0;
        assert_eq!(line_scan(r#"let s = "}";"#, &mut depth), (0, false));
        assert_eq!(line_scan("let r = r#\"}\"#;", &mut depth), (0, false));
        assert_eq!(line_scan("let c = '}';", &mut depth), (0, false));
        assert_eq!(line_scan("// }", &mut depth), (0, false));
        assert_eq!(line_scan("/* } */ fn f() {", &mut depth), (1, true));
    }

    /// Interior lines of a multi-line token must be reported as protected, and
    /// the flag count must match the line count exactly.
    #[test]
    fn protected_lines_cover_every_multiline_token() {
        let text = "\
let a = 1;
let s = \"alpha
  beta\";
let r = r#\"raw
  still raw\"#;
/* block
   comment */
let b = 2;
";
        let flags = protected_line_starts(text);
        assert_eq!(
            flags.len(),
            text.split('\n').count(),
            "one flag per `\n`-separated line"
        );
        // 0 `let a = 1;`            1 `let s = "alpha`  (opens, still code)
        // 2 `  beta";` (interior)   3 `let r = r#"raw`  (opens, still code)
        // 4 `  still raw"#;`        5 `/* block`
        // 6 `   comment */`         7 `let b = 2;`      8 trailing empty
        assert_eq!(
            flags,
            vec![false, false, true, false, true, false, true, false, false],
            "only lines beginning inside a token are protected"
        );
    }

    #[test]
    fn backslash_continuation_does_not_drift_the_line_count() {
        let text = "let s = \"alpha\\\n   beta\";\nlet t = 1;\n";
        let flags = protected_line_starts(text);
        assert_eq!(flags.len(), text.split('\n').count());
        assert_eq!(
            flags,
            vec![false, true, false, false],
            "the continued line is protected; the trailing line is not"
        );
    }
}
