//! Raw-string skipping for the `split` brace scanner.
//!
//! Raw strings are the one Rust literal whose delimiters are user-chosen
//! (`r"…"`, `r#"…"#`, `br##"…"##`), so a naive `"` scan would mis-count the
//! braces inside SQL, JSON fixtures, or regex patterns. This lives apart from
//! [`crate::split::scan`] because it is the only part of the walk with
//! delimiter-matching logic worth testing in isolation.

/// Index just past a raw string starting at `index`, when one starts there.
///
/// Returns `None` at an ordinary identifier character, so `r` in `render` or
/// `borrow` is never mistaken for a prefix. An unterminated literal consumes
/// the rest of the line, matching how `rustc` reports the error the caller
/// will see anyway.
#[must_use]
pub fn skip_raw_string(bytes: &[u8], index: usize) -> Option<usize> {
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
    cursor += 1;
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
}
