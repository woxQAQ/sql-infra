//! Editing-point normalization, lexical context, and identifier-prefix analysis.
//!
//! The module computes replacement ranges, quoting mode, normalized name parts,
//! qualifiers, and whether parser grammar suggestions are meaningful at the
//! point. It tolerates incomplete quoted and Unicode identifiers.

use pg_parser::TextRange;
use pg_parser::TextSize;
use pg_parser::Token;
use pg_parser::TokenValue;

use crate::CompletionDiagnostic;
use crate::CompletionDiagnosticKind;
use crate::lexical;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum IdentifierQuoting {
    #[default]
    Unquoted,
    Quoted,
    UnicodeQuoted,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompletionPrefix {
    pub raw: String,
    pub normalized: String,
    pub quoting: IdentifierQuoting,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamePart {
    pub text: String,
    pub normalized: String,
    pub quoted: bool,
    pub range: TextRange,
}

pub(super) fn name_part_from_token(
    source: &str,
    base: TextSize,
    token: &Token,
) -> Option<NamePart> {
    let mut text = match &token.value {
        Some(TokenValue::String(value)) => value.clone(),
        Some(TokenValue::Keyword(value)) => (*value).to_owned(),
        _ => return None,
    };
    let start = usize::from(token.range.start());
    let raw = source.get(start..usize::from(token.range.end()))?;
    let unicode_quoted = raw.to_ascii_lowercase().starts_with("u&\"");
    let quoted = raw.starts_with('"') || unicode_quoted;
    if unicode_quoted {
        text = decode_unicode_identifier(&text, '\\').unwrap_or(text);
    }
    Some(NamePart {
        normalized: if quoted {
            text.clone()
        } else {
            text.to_ascii_lowercase()
        },
        text,
        quoted,
        range: token.range + base,
    })
}

pub(super) struct NormalizedPoint {
    pub point: TextSize,
    pub diagnostics: Vec<CompletionDiagnostic>,
}

pub(super) struct CompletionSite {
    pub prefix: CompletionPrefix,
    pub replacement_range: TextRange,
    pub qualifier: Vec<NamePart>,
    lexical_context: LexicalContext,
}

impl CompletionSite {
    pub(super) fn supports_grammar_completion(&self) -> bool {
        self.lexical_context.supports_grammar_completion()
    }
}

pub(super) fn normalize_point(source: &str, requested: TextSize) -> NormalizedPoint {
    let requested = usize::from(requested);
    let mut point = requested.min(source.len());
    let mut diagnostics = Vec::new();
    if requested > source.len() {
        diagnostics.push(CompletionDiagnostic {
            kind: CompletionDiagnosticKind::PointClampedToEof,
            range: TextRange::empty(TextSize::from_usize(source.len())),
        });
    }
    if !source.is_char_boundary(point) {
        let original = point;
        while !source.is_char_boundary(point) {
            point -= 1;
        }
        diagnostics.push(CompletionDiagnostic {
            kind: CompletionDiagnosticKind::PointMovedToCharBoundary,
            range: TextRange::new(TextSize::from_usize(point), TextSize::from_usize(original)),
        });
    }
    NormalizedPoint {
        point: TextSize::from_usize(point),
        diagnostics,
    }
}

pub(super) fn analyze(source: &str, statement: TextRange, point: TextSize) -> CompletionSite {
    let point_usize = usize::from(point);
    let statement_start = usize::from(statement.start());
    let statement_end = usize::from(statement.end());
    let lexical_context = lexical_context(source, statement_start, point_usize);
    let supports_grammar_completion = lexical_context.supports_grammar_completion();
    let (start, end, quoting, raw, normalized) = match lexical_context {
        LexicalContext::DoubleQuote { open } => {
            quoted_prefix(source, statement_start, open, point_usize, statement_end)
        }
        LexicalContext::Normal
            if point_usize > statement_start && source.as_bytes()[point_usize - 1] == b'"' =>
        {
            if let Some((part, start)) = name_part_ending_at(source, statement_start, point_usize) {
                let unicode = source[start..point_usize]
                    .get(..2)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("u&"));
                let quote = if unicode { start + 2 } else { start };
                (
                    start,
                    point_usize,
                    if unicode {
                        IdentifierQuoting::UnicodeQuoted
                    } else {
                        IdentifierQuoting::Quoted
                    },
                    source[quote + 1..point_usize - 1].to_owned(),
                    part.normalized,
                )
            } else {
                (
                    point_usize,
                    point_usize,
                    IdentifierQuoting::Unquoted,
                    String::new(),
                    String::new(),
                )
            }
        }
        _ if !supports_grammar_completion => (
            point_usize,
            point_usize,
            IdentifierQuoting::Unquoted,
            String::new(),
            String::new(),
        ),
        _ => {
            let candidate_start = unquoted_start(source, statement_start, point_usize);
            let identifier = source[candidate_start..point_usize]
                .chars()
                .next()
                .is_some_and(is_identifier_start);
            if identifier {
                let end = unquoted_end(source, point_usize, statement_end);
                let raw = source[candidate_start..point_usize].to_owned();
                let normalized = raw.to_ascii_lowercase();
                (
                    candidate_start,
                    end,
                    IdentifierQuoting::Unquoted,
                    raw,
                    normalized,
                )
            } else {
                (
                    point_usize,
                    point_usize,
                    IdentifierQuoting::Unquoted,
                    String::new(),
                    String::new(),
                )
            }
        }
    };
    let qualifier = if supports_grammar_completion {
        qualifier_before(source, statement_start, start)
    } else {
        Vec::new()
    };
    CompletionSite {
        prefix: CompletionPrefix {
            raw,
            normalized,
            quoting,
        },
        replacement_range: TextRange::new(TextSize::from_usize(start), TextSize::from_usize(end)),
        qualifier,
        lexical_context,
    }
}

fn quoted_prefix(
    source: &str,
    lower_bound: usize,
    quote: usize,
    point: usize,
    upper_bound: usize,
) -> (usize, usize, IdentifierQuoting, String, String) {
    let unicode_start = unicode_quote_start(source, lower_bound, quote);
    let unicode = unicode_start.is_some();
    let start = unicode_start.unwrap_or(quote);
    let end = quoted_identifier_end(source, point, upper_bound);
    let raw = source[quote + 1..point].to_owned();
    let unescaped = raw.replace("\"\"", "\"");
    let normalized = if unicode {
        decode_unicode_identifier(&unescaped, '\\').unwrap_or(unescaped)
    } else {
        unescaped
    };
    (
        start,
        end,
        if unicode {
            IdentifierQuoting::UnicodeQuoted
        } else {
            IdentifierQuoting::Quoted
        },
        raw,
        normalized,
    )
}

fn unicode_quote_start(source: &str, lower_bound: usize, quote: usize) -> Option<usize> {
    let start = quote.checked_sub(2)?;
    if start < lower_bound || !source[start..quote].eq_ignore_ascii_case("u&") {
        return None;
    }
    (start == lower_bound
        || source[lower_bound..start]
            .chars()
            .next_back()
            .is_none_or(|ch| !is_identifier_continue(ch)))
    .then_some(start)
}

fn decode_unicode_identifier(input: &str, escape: char) -> Option<String> {
    let chars = input.chars().collect::<Vec<_>>();
    let mut decoded = String::with_capacity(input.len());
    let mut index = 0usize;
    while index < chars.len() {
        if chars[index] != escape {
            decoded.push(chars[index]);
            index += 1;
            continue;
        }
        if chars.get(index + 1) == Some(&escape) {
            decoded.push(escape);
            index += 2;
            continue;
        }

        let plus = chars.get(index + 1) == Some(&'+');
        let digits_start = index + if plus { 2 } else { 1 };
        let width = if plus { 6 } else { 4 };
        let digits_end = digits_start.checked_add(width)?;
        let value = unicode_escape_value(&chars, digits_start, digits_end)?;
        index = digits_end;

        let codepoint = if (0xD800..=0xDBFF).contains(&value) {
            if chars.get(index) != Some(&escape) {
                return None;
            }
            let second_end = index.checked_add(5)?;
            let second = unicode_escape_value(&chars, index + 1, second_end)?;
            if !(0xDC00..=0xDFFF).contains(&second) {
                return None;
            }
            index = second_end;
            0x10000 + ((value - 0xD800) << 10) + (second - 0xDC00)
        } else if (0xDC00..=0xDFFF).contains(&value) {
            return None;
        } else {
            value
        };
        if codepoint == 0 {
            return None;
        }
        decoded.push(char::from_u32(codepoint)?);
    }
    Some(decoded)
}

fn unicode_escape_value(chars: &[char], start: usize, end: usize) -> Option<u32> {
    if end > chars.len() {
        return None;
    }
    chars[start..end].iter().try_fold(0u32, |value, ch| {
        ch.to_digit(16)
            .and_then(|digit| value.checked_mul(16)?.checked_add(digit))
    })
}

fn quoted_identifier_end(source: &str, point: usize, upper_bound: usize) -> usize {
    let bytes = source.as_bytes();
    let mut cursor = point;
    while cursor < upper_bound {
        if bytes[cursor] != b'"' {
            cursor += 1;
        } else if bytes.get(cursor + 1) == Some(&b'"') && cursor + 1 < upper_bound {
            cursor += 2;
        } else {
            return cursor + 1;
        }
    }
    upper_bound
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LexicalContext {
    Normal,
    SingleQuote { escapes: bool },
    DoubleQuote { open: usize },
    LineComment,
    BlockComment,
    DollarQuote,
}

impl LexicalContext {
    fn supports_grammar_completion(self) -> bool {
        !matches!(
            self,
            Self::SingleQuote { .. } | Self::LineComment | Self::BlockComment | Self::DollarQuote
        )
    }
}

fn lexical_context(source: &str, start: usize, point: usize) -> LexicalContext {
    let bytes = source.as_bytes();
    let mut pos = start;
    let mut context = LexicalContext::Normal;
    let mut block_depth = 0usize;
    let mut dollar_tag: Option<Vec<u8>> = None;
    while pos < point {
        match context {
            LexicalContext::LineComment => {
                if lexical::is_line_break(bytes[pos]) {
                    context = LexicalContext::Normal;
                }
                pos += 1;
            }
            LexicalContext::BlockComment => {
                if bytes[pos..].starts_with(b"/*") {
                    block_depth += 1;
                    pos += 2;
                } else if bytes[pos..].starts_with(b"*/") {
                    block_depth -= 1;
                    pos += 2;
                    if block_depth == 0 {
                        context = LexicalContext::Normal;
                    }
                } else {
                    pos += 1;
                }
            }
            LexicalContext::DollarQuote => {
                let tag = dollar_tag.as_ref().expect("dollar quote owns its tag");
                if bytes[pos..].starts_with(tag) {
                    pos += tag.len();
                    dollar_tag = None;
                    context = LexicalContext::Normal;
                } else {
                    pos += 1;
                }
            }
            LexicalContext::SingleQuote { escapes } => {
                if escapes && bytes[pos] == b'\\' {
                    pos = (pos + 2).min(point);
                } else if bytes[pos] == b'\'' {
                    if bytes.get(pos + 1) == Some(&b'\'') {
                        pos += 2;
                    } else {
                        context = LexicalContext::Normal;
                        pos += 1;
                    }
                } else {
                    pos += 1;
                }
            }
            LexicalContext::DoubleQuote { .. } => {
                if bytes[pos] == b'"' {
                    if bytes.get(pos + 1) == Some(&b'"') {
                        pos += 2;
                    } else {
                        context = LexicalContext::Normal;
                        pos += 1;
                    }
                } else {
                    pos += 1;
                }
            }
            LexicalContext::Normal => {
                if bytes[pos..].starts_with(b"--") {
                    context = LexicalContext::LineComment;
                    pos += 2;
                } else if bytes[pos..].starts_with(b"/*") {
                    context = LexicalContext::BlockComment;
                    block_depth = 1;
                    pos += 2;
                } else if bytes[pos] == b'\'' {
                    context = LexicalContext::SingleQuote {
                        escapes: lexical::escape_string_starts_at_quote(bytes, pos),
                    };
                    pos += 1;
                } else if bytes[pos] == b'"' {
                    context = LexicalContext::DoubleQuote { open: pos };
                    pos += 1;
                } else if bytes[pos] == b'$' {
                    if let Some(tag) = lexical::dollar_quote_tag(bytes, pos, point) {
                        pos += tag.len();
                        dollar_tag = Some(tag);
                        context = LexicalContext::DollarQuote;
                    } else {
                        pos += 1;
                    }
                } else {
                    pos += 1;
                }
            }
        }
    }
    context
}

fn unquoted_start(source: &str, lower_bound: usize, point: usize) -> usize {
    let mut start = point;
    while start > lower_bound {
        let previous = source[..start].char_indices().next_back().unwrap();
        if is_identifier_continue(previous.1) {
            start = previous.0;
        } else {
            break;
        }
    }
    start
}

fn unquoted_end(source: &str, point: usize, upper_bound: usize) -> usize {
    let mut end = point;
    while end < upper_bound {
        let Some(ch) = source[end..upper_bound].chars().next() else {
            break;
        };
        if !is_identifier_continue(ch) {
            break;
        }
        end += ch.len_utf8();
    }
    end
}

fn qualifier_before(source: &str, statement_start: usize, prefix_start: usize) -> Vec<NamePart> {
    let mut cursor = prefix_start;
    let mut reversed = Vec::new();
    loop {
        cursor = trim_ascii_space_back(source, statement_start, cursor);
        if cursor == statement_start || source.as_bytes()[cursor - 1] != b'.' {
            break;
        }
        cursor = trim_ascii_space_back(source, statement_start, cursor - 1);
        let Some((part, start)) = name_part_ending_at(source, statement_start, cursor) else {
            break;
        };
        reversed.push(part);
        cursor = start;
    }
    reversed.reverse();
    reversed
}

fn name_part_ending_at(source: &str, lower_bound: usize, end: usize) -> Option<(NamePart, usize)> {
    if end == lower_bound {
        return None;
    }
    if source.as_bytes()[end - 1] == b'"' {
        let mut cursor = end - 1;
        while cursor > lower_bound {
            cursor -= 1;
            if source.as_bytes()[cursor] == b'"' {
                if cursor > lower_bound && source.as_bytes()[cursor - 1] == b'"' {
                    cursor -= 1;
                    continue;
                }
                let mut text = source[cursor + 1..end - 1].replace("\"\"", "\"");
                let unicode_start = unicode_quote_start(source, lower_bound, cursor);
                let start = unicode_start.unwrap_or(cursor);
                if unicode_start.is_some() {
                    text = decode_unicode_identifier(&text, '\\').unwrap_or(text);
                }
                return Some((
                    NamePart {
                        normalized: text.clone(),
                        text,
                        quoted: true,
                        range: TextRange::new(
                            TextSize::from_usize(start),
                            TextSize::from_usize(end),
                        ),
                    },
                    start,
                ));
            }
        }
        return None;
    }
    let start = unquoted_start(source, lower_bound, end);
    if start == end {
        return None;
    }
    let text = source[start..end].to_owned();
    Some((
        NamePart {
            normalized: text.to_ascii_lowercase(),
            text,
            quoted: false,
            range: TextRange::new(TextSize::from_usize(start), TextSize::from_usize(end)),
        },
        start,
    ))
}

fn trim_ascii_space_back(source: &str, lower_bound: usize, mut end: usize) -> usize {
    while end > lower_bound && source.as_bytes()[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    end
}

fn is_identifier_continue(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_alphanumeric() || !ch.is_ascii()
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch.is_alphabetic() || !ch.is_ascii()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(source: &str) -> TextRange {
        TextRange::new(TextSize::ZERO, TextSize::from_usize(source.len()))
    }

    #[test]
    fn separates_qualifier_and_unquoted_prefix() {
        let source = "select db.Schema.Na";
        let site = analyze(source, range(source), TextSize::from_usize(source.len()));
        assert_eq!(site.prefix.raw, "Na");
        assert_eq!(site.prefix.normalized, "na");
        assert_eq!(
            site.qualifier
                .iter()
                .map(|part| part.normalized.as_str())
                .collect::<Vec<_>>(),
            ["db", "schema"]
        );
    }

    #[test]
    fn keeps_quoted_prefix_case() {
        let source = "select s.\"Mixed";
        let site = analyze(source, range(source), TextSize::from_usize(source.len()));
        assert_eq!(site.prefix.raw, "Mixed");
        assert_eq!(site.prefix.normalized, "Mixed");
        assert_eq!(site.prefix.quoting, IdentifierQuoting::Quoted);
        assert_eq!(site.qualifier[0].normalized, "s");
    }

    #[test]
    fn replacement_range_covers_the_identifier_suffix() {
        let source = "select SELect, s.\"Mixed\"";
        let unquoted_point = source.find("SEL").unwrap() + 3;
        let site = analyze(source, range(source), TextSize::from_usize(unquoted_point));
        assert_eq!(site.prefix.raw, "SEL");
        assert_eq!(
            site.replacement_range,
            TextRange::new(TextSize::from_usize(7), TextSize::from_usize(13))
        );

        let quoted_point = source.find("Mix").unwrap() + 3;
        let site = analyze(source, range(source), TextSize::from_usize(quoted_point));
        assert_eq!(site.prefix.raw, "Mix");
        assert_eq!(
            site.replacement_range,
            TextRange::new(
                TextSize::from_usize(source.find('"').unwrap()),
                TextSize::from_usize(source.len())
            )
        );

        let source = "select \"MixedSuffix";
        let point = source.find("Mix").unwrap() + 3;
        let site = analyze(source, range(source), TextSize::from_usize(point));
        assert_eq!(
            site.replacement_range,
            TextRange::new(
                TextSize::from_usize(source.find('"').unwrap()),
                TextSize::from_usize(source.len())
            )
        );
    }

    #[test]
    fn moves_point_to_utf8_boundary() {
        let source = "名";
        let normalized = normalize_point(source, TextSize::new(2));
        assert_eq!(normalized.point, TextSize::ZERO);
        assert_eq!(
            normalized.diagnostics[0].kind,
            CompletionDiagnosticKind::PointMovedToCharBoundary
        );
    }

    #[test]
    fn does_not_treat_quotes_inside_strings_as_identifier_prefixes() {
        let source = "select 'not a \"name";
        let site = analyze(source, range(source), TextSize::from_usize(source.len()));
        assert!(!site.supports_grammar_completion());
        assert!(site.prefix.raw.is_empty());
    }

    #[test]
    fn extracts_identifiers_containing_a_dollar_quote_shaped_suffix() {
        let source = "SELECT name$tag$";
        let site = analyze(source, range(source), TextSize::from_usize(source.len()));
        assert_eq!(site.prefix.raw, "name$tag$");
        assert_eq!(site.prefix.normalized, "name$tag$");
        assert!(site.supports_grammar_completion());
    }

    #[test]
    fn unicode_quote_prefix_requires_a_token_boundary() {
        let source = "select fooU&\"Mixed\"";
        let point = source.find("Mix").unwrap() + 3;
        let site = analyze(source, range(source), TextSize::from_usize(point));
        let quote = source.find('"').unwrap();
        assert_eq!(site.prefix.quoting, IdentifierQuoting::Quoted);
        assert_eq!(site.replacement_range.start(), TextSize::from_usize(quote));
        let site = analyze(source, range(source), TextSize::from_usize(source.len()));
        assert_eq!(site.prefix.quoting, IdentifierQuoting::Quoted);
        assert_eq!(site.replacement_range.start(), TextSize::from_usize(quote));

        let source = "select U&\"Mixed\"";
        let point = source.find("Mix").unwrap() + 3;
        let site = analyze(source, range(source), TextSize::from_usize(point));
        assert_eq!(site.prefix.quoting, IdentifierQuoting::UnicodeQuoted);
        assert_eq!(
            site.replacement_range.start(),
            TextSize::from_usize(source.find("U&").unwrap())
        );
        let site = analyze(source, range(source), TextSize::from_usize(source.len()));
        assert_eq!(site.prefix.quoting, IdentifierQuoting::UnicodeQuoted);
        assert_eq!(
            site.replacement_range.start(),
            TextSize::from_usize(source.find("U&").unwrap())
        );
    }

    #[test]
    fn decodes_unicode_identifier_escapes_for_matching() {
        assert_eq!(
            decode_unicode_identifier(r"d\0061t\+000061", '\\').as_deref(),
            Some("data")
        );
        assert_eq!(
            decode_unicode_identifier(r"face\D83D\DE00", '\\').as_deref(),
            Some("face😀")
        );
        assert_eq!(
            decode_unicode_identifier(r"slash\\name", '\\').as_deref(),
            Some(r"slash\name")
        );
        assert!(decode_unicode_identifier(r"unfinished\00", '\\').is_none());

        let source = r#"select U&"S\0063hema".U&"M\0069x""#;
        let point = source.find(r"\0069").unwrap() + r"\0069".len();
        let site = analyze(source, range(source), TextSize::from_usize(point));
        assert_eq!(site.prefix.normalized, "Mi");
        assert_eq!(site.qualifier[0].normalized, "Schema");
    }
}
