//! Shared lexical predicates for incomplete editor input.
//!
//! Statement isolation and prefix analysis use these rules without demanding a
//! fully valid token stream. Dollar tags and identifier characters must stay in
//! sync with `pg-parser`'s lexer semantics.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dollar_tags_follow_the_lexer_identifier_rules() {
        assert_eq!(dollar_quote_tag(b"$$body$$", 0, 8).unwrap(), b"$$");
        assert_eq!(
            dollar_quote_tag(b"$tag_1$body$", 0, 12).unwrap(),
            b"$tag_1$"
        );
        assert!(dollar_quote_tag(b"$1$body$", 0, 8).is_none());
        assert!(dollar_quote_tag(b"$bad-tag$body$", 0, 14).is_none());
        assert!(dollar_quote_tag(b"name$tag$", 4, 9).is_none());
        assert_eq!(dollar_quote_tag(b"name $tag$", 5, 10).unwrap(), b"$tag$");
    }
}
