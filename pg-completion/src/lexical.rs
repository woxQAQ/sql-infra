//! Shared lexical predicates for incomplete editor input.
//!
//! Statement isolation and prefix analysis use these rules without demanding a
//! fully valid token stream. Dollar tags and identifier characters must stay in
//! sync with `pg-parser`'s lexer semantics.

/// Returns the PostgreSQL dollar-quote delimiter that starts at `start`.
///
/// `start` and the exclusive `end` bound are byte offsets into `input`. This
/// recognizes only the opening delimiter; the caller keeps the returned bytes
/// to find its matching close. A `$` inside an identifier is not a delimiter.
pub(super) fn dollar_quote_tag(input: &[u8], start: usize, end: usize) -> Option<Vec<u8>> {
    if start >= end || end > input.len() || (start > 0 && is_identifier_continue(input[start - 1]))
    {
        return None;
    }
    let input = &input[start..end];
    if input.first() != Some(&b'$') {
        return None;
    }
    let end = input[1..].iter().position(|byte| *byte == b'$')? + 1;
    if end == 1 {
        return Some(input[..=end].to_vec());
    }
    if !is_identifier_start(input[1])
        || !input[2..end]
            .iter()
            .all(|byte| is_identifier_start(*byte) || byte.is_ascii_digit())
    {
        return None;
    }
    Some(input[..=end].to_vec())
}

pub(super) fn escape_string_starts_at_quote(bytes: &[u8], quote: usize) -> bool {
    quote > 0
        && matches!(bytes[quote - 1], b'e' | b'E')
        && (quote == 1 || !is_identifier_continue(bytes[quote - 2]))
}

pub(super) fn is_line_break(byte: u8) -> bool {
    matches!(byte, b'\n' | b'\r')
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80
}

fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit() || byte == b'$'
}
