//! Generated-region markers: parsing, comparison, and byte-exact splicing.
//!
//! A region is delimited by two exact lines:
//!
//! ```text
//! <!-- BEGIN GENERATED: <id> -->
//! ...
//! <!-- END GENERATED: <id> -->
//! ```
//!
//! The parser rejects a BEGIN while another region is open (nesting), an END
//! without a matching BEGIN, mismatched ids, and duplicated ids. Content
//! comparisons are `\r\n`-normalized so a Windows checkout does not fail for
//! its line endings; rewriting converts the rendered content to the file's
//! own line ending, so bytes outside the markers are preserved exactly.

const BEGIN_PREFIX: &str = "<!-- BEGIN GENERATED: ";
const END_PREFIX: &str = "<!-- END GENERATED: ";
const MARKER_SUFFIX: &str = " -->";

/// A parsed region: its id plus the byte range of its content in the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    /// Marker id (`cli-commands`, `cli-contract pr review`, …).
    pub id: String,
    /// Byte range of the content between the marker lines (newlines included).
    pub content: std::ops::Range<usize>,
}

/// Parses every region; the error names the offending id and the problem.
pub fn parse_regions(text: &str) -> Result<Vec<Region>, String> {
    let mut regions: Vec<Region> = Vec::new();
    let mut open: Option<(String, usize)> = None;
    let mut offset = 0;
    for raw_line in text.split_inclusive('\n') {
        let line_start = offset;
        offset += raw_line.len();
        let line = raw_line.trim_end_matches(['\n', '\r']).trim_end();
        if let Some(id) = marker_id(line, BEGIN_PREFIX) {
            if let Some((open_id, _)) = &open {
                return Err(format!(
                    "region `{open_id}` has no END marker before `{id}` begins (markers must not nest)"
                ));
            }
            open = Some((id.to_string(), offset));
        } else if let Some(id) = marker_id(line, END_PREFIX) {
            let Some((open_id, content_start)) = open.take() else {
                return Err(format!(
                    "region `{id}` has an END marker without a BEGIN marker"
                ));
            };
            if open_id != id {
                return Err(format!(
                    "region `{open_id}` is closed by an END marker for `{id}`"
                ));
            }
            if regions.iter().any(|region| region.id == open_id) {
                return Err(format!("region `{open_id}` appears more than once"));
            }
            regions.push(Region {
                id: open_id,
                content: content_start..line_start,
            });
        }
    }
    if let Some((id, _)) = open {
        return Err(format!(
            "region `{id}` has a BEGIN marker without an END marker"
        ));
    }
    Ok(regions)
}

/// The id of a marker line, when the line is exactly a marker.
fn marker_id<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    line.strip_prefix(prefix)?.strip_suffix(MARKER_SUFFIX)
}

/// `\r\n` → `\n`, so comparisons ignore checkout line endings.
pub fn normalize_eol(text: &str) -> String {
    text.replace("\r\n", "\n")
}

/// Rewrites `region` in `text` with `content`, preserving the file's own EOL
/// style and every byte outside the region.
pub fn splice(text: &str, region: &Region, content: &str) -> String {
    let current = &text[region.content.clone()];
    // A CRLF checkout must stay CRLF; an empty region still follows its file.
    let eol = if current.contains("\r\n") || (current.is_empty() && text.contains("\r\n")) {
        "\r\n"
    } else {
        "\n"
    };
    let mut body = normalize_eol(content);
    if !body.ends_with('\n') {
        body.push('\n');
    }
    let body = if eol == "\r\n" {
        body.replace('\n', "\r\n")
    } else {
        body
    };
    let mut out = String::with_capacity(text.len() + body.len());
    out.push_str(&text[..region.content.start]);
    out.push_str(&body);
    out.push_str(&text[region.content.end..]);
    out
}
