pub(super) fn dollar_quote_tag(input: &[u8]) -> Option<Vec<u8>> {
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
        assert_eq!(dollar_quote_tag(b"$$body$$").unwrap(), b"$$");
        assert_eq!(dollar_quote_tag(b"$tag_1$body$").unwrap(), b"$tag_1$");
        assert!(dollar_quote_tag(b"$1$body$").is_none());
        assert!(dollar_quote_tag(b"$bad-tag$body$").is_none());
    }
}
