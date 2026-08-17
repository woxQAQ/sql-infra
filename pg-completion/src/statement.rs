//! Isolation of the semicolon-delimited statement containing the editing point.
//!
//! The scanner recognizes strings, identifiers, dollar quotes, and nested
//! comments so contained semicolons do not split the active statement.

use pg_parser::Loc;
use pg_parser::TextSize;

use crate::lexical;

/// Find the semicolon-delimited statement containing `point`, ignoring
/// semicolons in quoted strings, quoted identifiers, dollar quotes, and
/// comments.
pub(super) fn loc_at(source: &str, point: TextSize) -> Loc {
    let point = usize::from(point);
    let (mut start, mut end) = statement_bounds(source, point);

    while start < end && start < point && source.as_bytes()[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && end > point && source.as_bytes()[end - 1].is_ascii_whitespace() {
        end -= 1;
    }

    Loc::new(TextSize::from_usize(start), TextSize::from_usize(end))
}

fn statement_bounds(source: &str, point: usize) -> (usize, usize) {
    let bytes = source.as_bytes();
    let mut start = 0usize;
    let mut pos = 0usize;

    while pos < bytes.len() {
        if bytes[pos..].starts_with(b"--") {
            pos = skip_line_comment(bytes, pos);
            continue;
        }
        if bytes[pos..].starts_with(b"/*") {
            pos = skip_block_comment(bytes, pos);
            continue;
        }

        match bytes[pos] {
            b'\'' => {
                let backslash_escapes = lexical::escape_string_starts_at_quote(bytes, pos);
                pos = skip_single_quoted_string(bytes, pos, backslash_escapes);
            }
            b'"' => pos = skip_quoted_identifier(bytes, pos),
            b'$' => {
                if let Some(tag) = lexical::dollar_quote_tag(bytes, pos, bytes.len()) {
                    pos = skip_dollar_quoted_string(bytes, pos, &tag);
                } else {
                    pos += 1;
                }
            }
            b';' if pos >= point => return (start, pos),
            b';' => {
                start = pos + 1;
                pos += 1;
            }
            _ => pos += 1,
        }
    }
    (start, source.len())
}

fn skip_line_comment(bytes: &[u8], opening: usize) -> usize {
    let mut cursor = opening + 2;
    while cursor < bytes.len() && !lexical::is_line_break(bytes[cursor]) {
        cursor += 1;
    }
    if cursor < bytes.len() {
        cursor += 1;
    }
    cursor
}

fn skip_block_comment(bytes: &[u8], opening: usize) -> usize {
    let mut cursor = opening + 2;
    let mut depth = 1usize;
    while cursor < bytes.len() {
        if bytes[cursor..].starts_with(b"/*") {
            depth += 1;
            cursor += 2;
        } else if bytes[cursor..].starts_with(b"*/") {
            depth -= 1;
            cursor += 2;
            if depth == 0 {
                break;
            }
        } else {
            cursor += 1;
        }
    }
    cursor
}

fn skip_dollar_quoted_string(bytes: &[u8], opening: usize, tag: &[u8]) -> usize {
    let mut cursor = opening + tag.len();
    while cursor < bytes.len() {
        if bytes[cursor..].starts_with(tag) {
            return cursor + tag.len();
        }
        cursor += 1;
    }
    cursor
}

fn skip_single_quoted_string(bytes: &[u8], opening: usize, backslash_escapes: bool) -> usize {
    let mut cursor = opening + 1;
    while cursor < bytes.len() {
        if backslash_escapes && bytes[cursor] == b'\\' {
            cursor = (cursor + 2).min(bytes.len());
        } else if bytes[cursor] != b'\'' {
            cursor += 1;
        } else if bytes.get(cursor + 1) == Some(&b'\'') {
            cursor += 2;
        } else {
            return cursor + 1;
        }
    }
    cursor
}

fn skip_quoted_identifier(bytes: &[u8], opening: usize) -> usize {
    let mut cursor = opening + 1;
    while cursor < bytes.len() {
        if bytes[cursor] != b'"' {
            cursor += 1;
        } else if bytes.get(cursor + 1) == Some(&b'"') {
            cursor += 2;
        } else {
            return cursor + 1;
        }
    }
    cursor
}
