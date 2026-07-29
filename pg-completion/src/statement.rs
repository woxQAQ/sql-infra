use pg_parser::{TextRange, TextSize};

use crate::lexical;

/// Find the semicolon-delimited statement containing `point`, ignoring
/// semicolons in quoted strings, quoted identifiers, dollar quotes, and
/// comments.
pub(super) fn range_at(source: &str, point: TextSize) -> TextRange {
    let point = usize::from(point);
    let (mut start, mut end) = statement_bounds(source, point);

    while start < end && start < point && source.as_bytes()[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && end > point && source.as_bytes()[end - 1].is_ascii_whitespace() {
        end -= 1;
    }

    TextRange::new(text_size(start), text_size(end))
}

fn statement_bounds(source: &str, point: usize) -> (usize, usize) {
    let bytes = source.as_bytes();
    let mut start = 0usize;
    let mut pos = 0usize;
    let mut block_depth = 0usize;
    let mut single_quote = false;
    let mut single_quote_escapes = false;
    let mut double_quote = false;
    let mut line_comment = false;
    let mut dollar_tag: Option<Vec<u8>> = None;

    while pos < bytes.len() {
        if line_comment {
            if lexical::is_line_break(bytes[pos]) {
                line_comment = false;
            }
            pos += 1;
            continue;
        }
        if block_depth > 0 {
            if bytes[pos..].starts_with(b"/*") {
                block_depth += 1;
                pos += 2;
            } else if bytes[pos..].starts_with(b"*/") {
                block_depth -= 1;
                pos += 2;
            } else {
                pos += 1;
            }
            continue;
        }
        if let Some(tag) = &dollar_tag {
            if bytes[pos..].starts_with(tag) {
                pos += tag.len();
                dollar_tag = None;
            } else {
                pos += 1;
            }
            continue;
        }
        if single_quote {
            if single_quote_escapes && bytes[pos] == b'\\' {
                pos = (pos + 2).min(bytes.len());
            } else if bytes[pos] == b'\'' {
                if bytes.get(pos + 1) == Some(&b'\'') {
                    pos += 2;
                } else {
                    single_quote = false;
                    pos += 1;
                }
            } else {
                pos += 1;
            }
            continue;
        }
        if double_quote {
            if bytes[pos] == b'"' {
                if bytes.get(pos + 1) == Some(&b'"') {
                    pos += 2;
                } else {
                    double_quote = false;
                    pos += 1;
                }
            } else {
                pos += 1;
            }
            continue;
        }

        if bytes[pos..].starts_with(b"--") {
            line_comment = true;
            pos += 2;
        } else if bytes[pos..].starts_with(b"/*") {
            block_depth = 1;
            pos += 2;
        } else if bytes[pos] == b'\'' {
            single_quote = true;
            single_quote_escapes = lexical::escape_string_starts_at_quote(bytes, pos);
            pos += 1;
        } else if bytes[pos] == b'"' {
            double_quote = true;
            pos += 1;
        } else if bytes[pos] == b'$' {
            if let Some(tag) = lexical::dollar_quote_tag(bytes, pos, bytes.len()) {
                pos += tag.len();
                dollar_tag = Some(tag);
            } else {
                pos += 1;
            }
        } else {
            if bytes[pos] == b';' {
                if pos < point {
                    start = pos + 1;
                } else {
                    return (start, pos);
                }
            }
            pos += 1;
        }
    }
    (start, source.len())
}

fn text_size(value: usize) -> TextSize {
    TextSize::try_from(value).expect("source length was represented by TextSize")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_semicolons_in_lexical_containers() {
        let sql = "select ';'; select $$;$$; select \";\"; /* ; */ select 4";
        assert_eq!(
            range_at(sql, text_size(sql.len())),
            TextRange::new(text_size(38), text_size(sql.len()))
        );
    }

    #[test]
    fn follows_escape_strings_and_carriage_return_comments() {
        let sql = "select E'a\\';b'; select 2";
        assert_eq!(
            range_at(sql, text_size(sql.len())),
            TextRange::new(text_size(17), text_size(sql.len()))
        );

        let sql = "select 1 -- comment\r; select 2";
        assert_eq!(
            range_at(sql, text_size(sql.len())),
            TextRange::new(text_size(22), text_size(sql.len()))
        );
    }

    #[test]
    fn does_not_treat_a_parameter_suffix_as_a_dollar_quote() {
        let sql = "select $1$; select 2";
        assert_eq!(
            range_at(sql, text_size(sql.len())),
            TextRange::new(text_size(12), text_size(sql.len()))
        );
    }

    #[test]
    fn does_not_treat_an_identifier_suffix_as_a_dollar_quote() {
        let sql = "select name$tag$; select 2";
        assert_eq!(
            range_at(sql, text_size(sql.len())),
            TextRange::new(text_size(18), text_size(sql.len()))
        );
    }
}
