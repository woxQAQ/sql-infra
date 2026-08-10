//! PostgreSQL-compatible tokenization with byte-accurate ranges.
//!
//! Strict lexing reports malformed input immediately; completion lexing may
//! replace an error at the editing point with an `Incomplete` token. The
//! implementation follows PostgreSQL `scan.l`, `gram.y`, and `kwlist.h` rules.

use crate::BareLabel;
use crate::KEYWORDS;
use crate::KeywordCategory;
use crate::TextRange;
use crate::TextSize;
use crate::TokenKind;

/// PostgreSQL's fixed name width, including the terminating NUL byte.
const NAMEDATALEN: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Keyword {
    pub word: &'static str,
    pub kind: TokenKind,
    pub category: KeywordCategory,
    pub bare_label: BareLabel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TokenValue {
    Integer(i32),
    String(std::string::String),
    Keyword(&'static str),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub range: TextRange,
    pub value: Option<TokenValue>,
}

impl Token {
    fn new(kind: TokenKind, location: usize) -> Self {
        Self {
            kind,
            range: TextRange::empty(TextSize::from_usize(location)),
            value: None,
        }
    }

    fn string(kind: TokenKind, location: usize, value: impl Into<std::string::String>) -> Self {
        Self {
            kind,
            range: TextRange::empty(TextSize::from_usize(location)),
            value: Some(TokenValue::String(value.into())),
        }
    }

    fn integer(kind: TokenKind, location: usize, value: i32) -> Self {
        Self {
            kind,
            range: TextRange::empty(TextSize::from_usize(location)),
            value: Some(TokenValue::Integer(value)),
        }
    }

    fn keyword(kind: TokenKind, location: usize, word: &'static str) -> Self {
        Self {
            kind,
            range: TextRange::empty(TextSize::from_usize(location)),
            value: Some(TokenValue::Keyword(word)),
        }
    }

    pub(crate) fn synthetic(kind: TokenKind, location: usize) -> Self {
        Self::new(kind, location)
    }

    pub(crate) fn completion_hole(location: usize) -> Self {
        Self::string(TokenKind::Ident, location, "__completion_hole__")
    }

    pub fn location(&self) -> usize {
        self.range.start().into()
    }

    pub fn end_location(&self) -> usize {
        self.range.end().into()
    }

    fn finish(&mut self, end: usize) {
        self.range = TextRange::new(self.range.start(), TextSize::from_usize(end));
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LexError {
    pub message: std::string::String,
    pub range: TextRange,
}

impl LexError {
    fn new(location: usize, message: impl Into<std::string::String>) -> Self {
        Self {
            range: TextRange::empty(TextSize::from_usize(location)),
            message: message.into(),
        }
    }

    fn ranged(start: usize, end: usize, message: impl Into<std::string::String>) -> Self {
        Self {
            range: TextRange::new(TextSize::from_usize(start), TextSize::from_usize(end)),
            message: message.into(),
        }
    }

    pub fn location(&self) -> usize {
        self.range.start().into()
    }
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at byte {}", self.message, self.location())
    }
}

impl std::error::Error for LexError {}

pub fn lookup_keyword(word: &str) -> Option<&'static Keyword> {
    let lower = word.to_ascii_lowercase();
    KEYWORDS
        .binary_search_by(|keyword| keyword.word.cmp(lower.as_str()))
        .ok()
        .map(|index| &KEYWORDS[index])
}

pub fn lex(input: &str) -> Result<Vec<Token>, LexError> {
    let mut lexer = Lexer::new(input)?;
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_token()?;
        let is_eof = token.kind == TokenKind::Eof;
        tokens.push(token);
        if is_eof {
            return Ok(tokens);
        }
    }
}

/// Tokens produced for editor input, including any lexical error recovered at
/// the completion point.
#[derive(Clone, Debug, PartialEq)]
pub struct CompletionTokenization {
    tokens: Vec<Token>,
    recovered_error: Option<LexError>,
}

impl CompletionTokenization {
    /// Returns the token stream, including a synthetic `Incomplete` token when
    /// tokenization recovered at the completion point.
    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    /// Returns the lexical error replaced by an `Incomplete` token, if any.
    pub fn recovered_error(&self) -> Option<&LexError> {
        self.recovered_error.as_ref()
    }

    /// Consumes the result and returns its token stream.
    pub fn into_tokens(self) -> Vec<Token> {
        self.tokens
    }
}

/// Tokenize editor input without weakening the strict [`lex`] interface.
///
/// A lexical failure that starts at or after `point`, or whose invalid range
/// reaches `point`, is represented by an `Incomplete` token. A failure wholly
/// before `point` remains fatal because the parser cannot reliably infer the
/// grammar state past it.
pub fn lex_for_completion(
    input: &str,
    point: TextSize,
) -> Result<CompletionTokenization, LexError> {
    let completion_offset = usize::from(point).min(input.len());
    match lex(input) {
        Ok(tokens) => Ok(CompletionTokenization {
            tokens,
            recovered_error: None,
        }),
        Err(error) if usize::from(error.range.end()) >= completion_offset => {
            let valid_prefix_end = usize::from(error.range.start()).min(input.len());
            let mut tokens = lex(&input[..valid_prefix_end])?;
            debug_assert_eq!(tokens.last().map(|token| token.kind), Some(TokenKind::Eof));
            tokens.pop();
            tokens.push(Token {
                kind: TokenKind::Incomplete,
                range: error.range,
                value: None,
            });
            tokens.push(Token::new(TokenKind::Eof, usize::from(error.range.end())));
            Ok(CompletionTokenization {
                tokens,
                recovered_error: Some(error),
            })
        }
        Err(error) => Err(error),
    }
}

pub struct Lexer<'a> {
    input: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Result<Self, LexError> {
        TextSize::try_from(input.len()).map_err(|error| LexError::new(0, error.to_string()))?;
        Ok(Self {
            input,
            bytes: input.as_bytes(),
            pos: 0,
        })
    }

    pub fn next_token(&mut self) -> Result<Token, LexError> {
        let mut token = self.scan_token()?;
        token.finish(self.pos);
        Ok(token)
    }

    fn scan_token(&mut self) -> Result<Token, LexError> {
        self.skip_whitespace_and_comments()?;
        let token_start = self.pos;
        if self.eof() {
            return Ok(Token::new(TokenKind::Eof, token_start));
        }

        if self.starts_with_ignore_ascii_case("b'") {
            self.pos += 2;
            return self.scan_bit_or_hex_string(token_start, b'b', TokenKind::BConst);
        }
        if self.starts_with_ignore_ascii_case("x'") {
            self.pos += 2;
            return self.scan_bit_or_hex_string(token_start, b'x', TokenKind::XConst);
        }
        if self.starts_with_ignore_ascii_case("n'") {
            self.pos += 1;
            return Ok(Token::keyword(TokenKind::Nchar, token_start, "nchar"));
        }
        if self.starts_with_ignore_ascii_case("e'") {
            self.pos += 2;
            return self.scan_quoted_string(
                token_start,
                StringEscapeMode::Backslash,
                TokenKind::SConst,
            );
        }
        if self.starts_with_ignore_ascii_case("u&\"") {
            self.pos += 3;
            return self.scan_quoted_identifier(token_start, TokenKind::UIdent);
        }
        if self.starts_with_ignore_ascii_case("u&'") {
            self.pos += 3;
            return self.scan_quoted_string(
                token_start,
                StringEscapeMode::Literal,
                TokenKind::USConst,
            );
        }
        if self.starts_with_ignore_ascii_case("u&") {
            self.pos += 1;
            return Ok(Token::string(TokenKind::Ident, token_start, "u"));
        }
        if self.peek() == Some(b'\'') {
            self.pos += 1;
            return self.scan_quoted_string(
                token_start,
                StringEscapeMode::Literal,
                TokenKind::SConst,
            );
        }
        if self.peek() == Some(b'"') {
            self.pos += 1;
            return self.scan_quoted_identifier(token_start, TokenKind::Ident);
        }
        if self.peek() == Some(b'$')
            && let Some(token) = self.try_scan_dollar_quote_or_parameter()?
        {
            return Ok(token);
        }

        if let Some(kind) = self
            .bytes
            .get(self.pos..self.pos + 2)
            .and_then(two_byte_token_kind)
        {
            self.pos += 2;
            return Ok(Token::new(kind, token_start));
        }

        if self.peek().is_some_and(is_dec_digit)
            || (self.peek() == Some(b'.') && self.peek_n(1).is_some_and(is_dec_digit))
        {
            return self.scan_number();
        }

        if self.peek().is_some_and(is_ident_start) {
            return Ok(self.scan_identifier_or_keyword());
        }

        if self.peek().is_some_and(is_operator_char) {
            return self.scan_operator();
        }

        if self.peek().is_some_and(is_single_char_token) {
            let ch = self
                .bump_ascii_char()
                .expect("single-character token check guarantees a byte");
            return Ok(Token::new(TokenKind::Char(ch), token_start));
        }

        let ch = self
            .bump_char()
            .expect("non-EOF lexer position must start a UTF-8 character");
        Ok(Token::new(TokenKind::Char(ch), token_start))
    }

    fn eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek_n(&self, n: usize) -> Option<u8> {
        self.bytes.get(self.pos + n).copied()
    }

    fn starts_with(&self, needle: &str) -> bool {
        self.bytes[self.pos..].starts_with(needle.as_bytes())
    }

    fn starts_with_ignore_ascii_case(&self, needle: &str) -> bool {
        let hay = self.bytes.get(self.pos..self.pos + needle.len());
        hay.is_some_and(|hay| hay.eq_ignore_ascii_case(needle.as_bytes()))
    }

    fn bump_ascii_char(&mut self) -> Option<char> {
        let b = self.peek()?;
        self.pos += 1;
        Some(b as char)
    }

    fn bump_char(&mut self) -> Option<char> {
        let rest = self.input.get(self.pos..)?;
        let ch = rest.chars().next()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn error<T>(
        &self,
        range_start: usize,
        message: impl Into<std::string::String>,
    ) -> Result<T, LexError> {
        Err(LexError::ranged(range_start, self.pos, message))
    }

    fn skip_whitespace_and_comments(&mut self) -> Result<(), LexError> {
        loop {
            let trivia_start = self.pos;
            while self.peek().is_some_and(is_space) {
                self.pos += 1;
            }
            if self.starts_with("--") {
                self.pos += 2;
                while let Some(b) = self.peek() {
                    if b == b'\n' || b == b'\r' {
                        break;
                    }
                    self.pos += 1;
                }
                continue;
            }
            if self.starts_with("/*") {
                self.skip_block_comment()?;
                continue;
            }
            if self.pos == trivia_start {
                return Ok(());
            }
        }
    }

    fn skip_block_comment(&mut self) -> Result<(), LexError> {
        let comment_start = self.pos;
        self.pos += 2;
        let mut nested_depth = 0usize;
        while !self.eof() {
            if self.starts_with("/*") {
                nested_depth += 1;
                self.pos += 2;
            } else if self.starts_with("*/") {
                self.pos += 2;
                if nested_depth == 0 {
                    return Ok(());
                }
                nested_depth -= 1;
            } else {
                self.pos += 1;
            }
        }
        self.error(comment_start, "unterminated /* comment")
    }

    fn scan_bit_or_hex_string(
        &mut self,
        token_start: usize,
        prefix: u8,
        kind: TokenKind,
    ) -> Result<Token, LexError> {
        let mut literal = vec![prefix];
        loop {
            while let Some(b) = self.peek() {
                if b == b'\'' {
                    break;
                }
                literal.push(b);
                self.pos += 1;
            }
            if self.eof() {
                let message = if prefix == b'b' {
                    "unterminated bit string literal"
                } else {
                    "unterminated hexadecimal string literal"
                };
                return self.error(token_start, message);
            }
            self.pos += 1;
            if self.try_consume_string_continuation() {
                continue;
            }
            return Ok(Token::string(kind, token_start, string_from_bytes(literal)));
        }
    }

    fn scan_quoted_string(
        &mut self,
        token_start: usize,
        escape_mode: StringEscapeMode,
        kind: TokenKind,
    ) -> Result<Token, LexError> {
        let mut literal = Vec::new();
        while let Some(b) = self.peek() {
            if b == b'\'' {
                if self.peek_n(1) == Some(b'\'') {
                    literal.push(b'\'');
                    self.pos += 2;
                    continue;
                }
                self.pos += 1;
                if self.try_consume_string_continuation() {
                    continue;
                }
                return Ok(Token::string(kind, token_start, string_from_bytes(literal)));
            }

            if escape_mode == StringEscapeMode::Backslash && b == b'\\' {
                self.pos += 1;
                self.scan_escape_sequence(&mut literal)?;
                continue;
            }

            literal.push(b);
            self.pos += 1;
        }
        self.error(token_start, "unterminated quoted string")
    }

    fn scan_escape_sequence(&mut self, literal: &mut Vec<u8>) -> Result<(), LexError> {
        let Some(escaped) = self.peek() else {
            literal.push(b'\\');
            return Ok(());
        };

        if matches!(escaped, b'u' | b'U') {
            return self.scan_unicode_escape(literal);
        }

        if (b'0'..=b'7').contains(&escaped) {
            let digits_start = self.pos;
            let mut digits_end = self.pos;
            for _ in 0..3 {
                if self
                    .bytes
                    .get(digits_end)
                    .is_some_and(|b| (b'0'..=b'7').contains(b))
                {
                    digits_end += 1;
                } else {
                    break;
                }
            }
            let byte = u8::from_str_radix(&self.input[digits_start..digits_end], 8).unwrap();
            literal.push(byte);
            self.pos = digits_end;
            return Ok(());
        }

        if escaped == b'x' {
            let digits_start = self.pos + 1;
            let mut digits_end = digits_start;
            for _ in 0..2 {
                if self
                    .bytes
                    .get(digits_end)
                    .is_some_and(|b| b.is_ascii_hexdigit())
                {
                    digits_end += 1;
                } else {
                    break;
                }
            }
            if digits_end > digits_start {
                let byte = u8::from_str_radix(&self.input[digits_start..digits_end], 16).unwrap();
                literal.push(byte);
                self.pos = digits_end;
                return Ok(());
            }
        }

        self.pos += 1;
        literal.push(match escaped {
            b'b' => 0x08,
            b'f' => 0x0C,
            b'n' => b'\n',
            b'r' => b'\n',
            b't' => b'\t',
            b'v' => 0x0B,
            other => other,
        });
        Ok(())
    }

    fn scan_unicode_escape(&mut self, literal: &mut Vec<u8>) -> Result<(), LexError> {
        let escape_start = self.pos - 1;
        let width = match self.peek() {
            Some(b'u') => 4,
            Some(b'U') => 8,
            _ => unreachable!("Unicode escape scanning starts at 'u' or 'U'"),
        };
        self.pos += 1;
        let first_value = self.read_fixed_hex_escape(escape_start, width)?;

        if is_high_surrogate(first_value) {
            if !(self.peek() == Some(b'\\') && matches!(self.peek_n(1), Some(b'u' | b'U'))) {
                return self.error(self.pos, "invalid Unicode surrogate pair");
            }

            let second_escape_start = self.pos;
            self.pos += 1;
            let second_width = if self.peek() == Some(b'u') { 4 } else { 8 };
            self.pos += 1;
            let second_value = self.read_fixed_hex_escape(second_escape_start, second_width)?;
            if !is_low_surrogate(second_value) {
                return self.error(self.pos, "invalid Unicode surrogate pair");
            }

            let codepoint = 0x10000 + (((first_value - 0xD800) << 10) | (second_value - 0xDC00));
            push_codepoint(literal, codepoint, escape_start, self.pos)
        } else if is_low_surrogate(first_value) {
            self.error(escape_start, "invalid Unicode surrogate pair")
        } else {
            push_codepoint(literal, first_value, escape_start, self.pos)
        }
    }

    fn read_fixed_hex_escape(
        &mut self,
        escape_start: usize,
        width: usize,
    ) -> Result<u32, LexError> {
        let digits_end = self.pos + width;
        if digits_end > self.bytes.len()
            || !self.bytes[self.pos..digits_end]
                .iter()
                .all(u8::is_ascii_hexdigit)
        {
            return self.error(escape_start, "invalid Unicode escape");
        }
        let escape_value = u32::from_str_radix(&self.input[self.pos..digits_end], 16).unwrap();
        self.pos = digits_end;
        Ok(escape_value)
    }

    fn try_consume_string_continuation(&mut self) -> bool {
        let mut continuation_end = self.pos;
        let mut saw_newline = false;
        loop {
            match self.bytes.get(continuation_end).copied() {
                Some(b'\n' | b'\r') => {
                    saw_newline = true;
                    continuation_end += 1;
                }
                Some(b' ' | b'\t' | 0x0C | 0x0B) => continuation_end += 1,
                Some(b'-') if self.bytes.get(continuation_end + 1) == Some(&b'-') => {
                    continuation_end += 2;
                    while let Some(b) = self.bytes.get(continuation_end).copied() {
                        if b == b'\n' || b == b'\r' {
                            break;
                        }
                        continuation_end += 1;
                    }
                }
                _ => break,
            }
        }

        // PostgreSQL concatenates adjacent quoted strings only when the
        // separating whitespace contains a newline.
        if !saw_newline || self.bytes.get(continuation_end) != Some(&b'\'') {
            return false;
        }

        self.pos = continuation_end + 1;
        true
    }

    fn try_scan_dollar_quote_or_parameter(&mut self) -> Result<Option<Token>, LexError> {
        let token_start = self.pos;
        if let Some(delimiter_end) = self.dollar_quote_delimiter_end(self.pos) {
            let delimiter = &self.input[self.pos..delimiter_end];
            self.pos = delimiter_end;
            let content_start = self.pos;
            if let Some(content_len) = self.input[content_start..].find(delimiter) {
                let content_end = content_start + content_len;
                let content = self.input[content_start..content_end].to_owned();
                self.pos = content_end + delimiter.len();
                return Ok(Some(Token::string(TokenKind::SConst, token_start, content)));
            }
            return self.error(token_start, "unterminated dollar-quoted string");
        }

        if self.peek_n(1).is_some_and(is_dec_digit) {
            self.pos += 1;
            let digits_start = self.pos;
            while self.peek().is_some_and(is_dec_digit) {
                self.pos += 1;
            }
            if self.peek().is_some_and(is_ident_start) {
                return self.error(token_start, "trailing junk after parameter");
            }
            let digits = &self.input[digits_start..self.pos];
            let number = digits.parse::<i32>().map_err(|_| {
                LexError::ranged(token_start, self.pos, "parameter number too large")
            })?;
            return Ok(Some(Token::integer(TokenKind::Param, token_start, number)));
        }

        Ok(None)
    }

    fn dollar_quote_delimiter_end(&self, delimiter_start: usize) -> Option<usize> {
        if self.bytes.get(delimiter_start) != Some(&b'$') {
            return None;
        }
        let mut delimiter_end = delimiter_start + 1;
        if self.bytes.get(delimiter_end) == Some(&b'$') {
            return Some(delimiter_end + 1);
        }
        if !self
            .bytes
            .get(delimiter_end)
            .is_some_and(|b| is_dollar_quote_tag_start(*b))
        {
            return None;
        }
        delimiter_end += 1;
        while self
            .bytes
            .get(delimiter_end)
            .is_some_and(|b| is_dollar_quote_tag_continue(*b))
        {
            delimiter_end += 1;
        }
        if self.bytes.get(delimiter_end) == Some(&b'$') {
            Some(delimiter_end + 1)
        } else {
            None
        }
    }

    fn scan_quoted_identifier(
        &mut self,
        token_start: usize,
        kind: TokenKind,
    ) -> Result<Token, LexError> {
        let mut literal = Vec::new();
        while let Some(b) = self.peek() {
            if b == b'"' {
                if self.peek_n(1) == Some(b'"') {
                    literal.push(b'"');
                    self.pos += 2;
                    continue;
                }
                self.pos += 1;
                if literal.is_empty() {
                    return self.error(token_start, "zero-length delimited identifier");
                }
                let identifier = truncate_identifier(&string_from_bytes(literal));
                return Ok(Token::string(kind, token_start, identifier));
            }
            literal.push(b);
            self.pos += 1;
        }
        self.error(token_start, "unterminated quoted identifier")
    }

    fn scan_identifier_or_keyword(&mut self) -> Token {
        let token_start = self.pos;
        self.pos += 1;
        while self.peek().is_some_and(is_ident_cont) {
            self.pos += 1;
        }
        let identifier = &self.input[token_start..self.pos];
        if let Some(keyword) = lookup_keyword(identifier) {
            return Token::keyword(keyword.kind, token_start, keyword.word);
        }
        Token::string(
            TokenKind::Ident,
            token_start,
            downcase_truncate_identifier(identifier),
        )
    }

    fn scan_number(&mut self) -> Result<Token, LexError> {
        let token_start = self.pos;
        if self.peek() == Some(b'.') {
            self.pos += 1;
            self.scan_decimal_digits();
            self.scan_exponent(token_start)?;
            self.reject_numeric_junk(token_start)?;
            return Ok(Token::string(
                TokenKind::FConst,
                token_start,
                &self.input[token_start..self.pos],
            ));
        }

        if self.starts_with_ignore_ascii_case("0x") {
            return self.scan_prefixed_integer(16, "invalid hexadecimal integer");
        }
        if self.starts_with_ignore_ascii_case("0o") {
            return self.scan_prefixed_integer(8, "invalid octal integer");
        }
        if self.starts_with_ignore_ascii_case("0b") {
            return self.scan_prefixed_integer(2, "invalid binary integer");
        }

        self.pos += 1;
        self.scan_decimal_digits();

        if self.starts_with("..") {
            return Ok(self.integer_token(token_start, 10));
        }

        let mut has_decimal_or_exponent = false;
        if self.peek() == Some(b'.') {
            has_decimal_or_exponent = true;
            self.pos += 1;
            if self.peek().is_some_and(is_dec_digit) {
                self.pos += 1;
                self.scan_decimal_digits();
            }
        }
        if self.peek().is_some_and(|b| b == b'e' || b == b'E') {
            has_decimal_or_exponent = true;
            self.scan_exponent(token_start)?;
        }
        self.reject_numeric_junk(token_start)?;
        if has_decimal_or_exponent {
            Ok(Token::string(
                TokenKind::FConst,
                token_start,
                &self.input[token_start..self.pos],
            ))
        } else {
            Ok(self.integer_token(token_start, 10))
        }
    }

    fn scan_prefixed_integer(
        &mut self,
        radix: u32,
        invalid_message: &'static str,
    ) -> Result<Token, LexError> {
        let token_start = self.pos;
        self.pos += 2;
        let mut saw_digit = false;
        if self.peek() == Some(b'_') {
            self.pos += 1;
        }
        while self.peek().is_some_and(|b| is_digit_for_radix(b, radix)) {
            saw_digit = true;
            self.pos += 1;
            if self.peek() == Some(b'_')
                && self.peek_n(1).is_some_and(|b| is_digit_for_radix(b, radix))
            {
                self.pos += 1;
            }
        }
        if !saw_digit {
            return self.error(token_start, invalid_message);
        }
        self.reject_numeric_junk(token_start)?;
        Ok(self.integer_token(token_start, radix))
    }

    fn scan_decimal_digits(&mut self) {
        loop {
            if self.peek().is_some_and(is_dec_digit) {
                self.pos += 1;
            } else if self.peek() == Some(b'_') && self.peek_n(1).is_some_and(is_dec_digit) {
                self.pos += 2;
            } else {
                break;
            }
        }
    }

    fn scan_exponent(&mut self, token_start: usize) -> Result<(), LexError> {
        if !self.peek().is_some_and(|b| b == b'e' || b == b'E') {
            return Ok(());
        }
        let exponent_start = self.pos;
        self.pos += 1;
        if self.peek().is_some_and(|b| b == b'+' || b == b'-') {
            self.pos += 1;
        }
        if !self.peek().is_some_and(is_dec_digit) {
            self.pos = exponent_start;
            return self.error(token_start, "trailing junk after numeric literal");
        }
        self.pos += 1;
        self.scan_decimal_digits();
        Ok(())
    }

    fn reject_numeric_junk(&self, token_start: usize) -> Result<(), LexError> {
        if self.peek().is_some_and(is_ident_start) {
            return Err(LexError::ranged(
                token_start,
                self.pos,
                "trailing junk after numeric literal",
            ));
        }
        Ok(())
    }

    fn integer_token(&self, token_start: usize, radix: u32) -> Token {
        let lexeme = &self.input[token_start..self.pos];
        let normalized = lexeme.replace('_', "");
        let prefix_len = prefix_for_radix(radix).len();
        let digits = &normalized[prefix_len..];

        match i32::from_str_radix(digits, radix) {
            Ok(integer) => Token::integer(TokenKind::IConst, token_start, integer),
            // PostgreSQL classifies out-of-range integer lexemes as FCONST
            // rather than rejecting them in the lexer.
            Err(_) => Token::string(TokenKind::FConst, token_start, lexeme),
        }
    }

    fn scan_operator(&mut self) -> Result<Token, LexError> {
        let token_start = self.pos;
        while self.peek().is_some_and(is_operator_char) {
            // Comment openers terminate an operator even when they occur
            // after one or more operator characters.
            if self.starts_with("/*") || self.starts_with("--") {
                break;
            }
            self.pos += 1;
        }
        let mut operator_end = self.pos;

        if operator_end - token_start > 1 {
            let bytes = &self.bytes[token_start..operator_end];
            if matches!(bytes.last(), Some(b'+' | b'-')) {
                // SQL-compatible operator sequences cannot end in '+' or '-'.
                // PostgreSQL permits the suffix only when an earlier character
                // makes the sequence unambiguously a user-defined operator.
                let allows_trailing_sign = bytes[..bytes.len() - 1]
                    .iter()
                    .copied()
                    .any(is_non_sql_operator_char);
                if !allows_trailing_sign {
                    while operator_end - token_start > 1
                        && matches!(self.bytes[operator_end - 1], b'+' | b'-')
                    {
                        operator_end -= 1;
                    }
                    self.pos = operator_end;
                }
            }
        }

        let operator = &self.input[token_start..operator_end];
        if operator.len() == 1 {
            let byte = operator.as_bytes()[0];
            if is_single_char_token(byte) {
                return Ok(Token::new(TokenKind::Char(byte as char), token_start));
            }
        }
        if operator.len() >= NAMEDATALEN {
            return self.error(token_start, "operator too long");
        }
        Ok(Token::string(TokenKind::Op, token_start, operator))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StringEscapeMode {
    Literal,
    Backslash,
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0C | 0x0B)
}

fn is_dec_digit(b: u8) -> bool {
    b.is_ascii_digit()
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b >= 0x80
}

fn is_ident_cont(b: u8) -> bool {
    is_ident_start(b) || b.is_ascii_digit() || b == b'$'
}

fn is_dollar_quote_tag_start(b: u8) -> bool {
    is_ident_start(b)
}

fn is_dollar_quote_tag_continue(b: u8) -> bool {
    is_ident_start(b) || b.is_ascii_digit()
}

fn is_single_char_token(b: u8) -> bool {
    matches!(
        b,
        b',' | b'('
            | b')'
            | b'['
            | b']'
            | b'.'
            | b';'
            | b':'
            | b'|'
            | b'+'
            | b'-'
            | b'*'
            | b'/'
            | b'%'
            | b'^'
            | b'<'
            | b'>'
            | b'='
    )
}

fn is_operator_char(b: u8) -> bool {
    matches!(
        b,
        b'~' | b'!'
            | b'@'
            | b'#'
            | b'^'
            | b'&'
            | b'|'
            | b'`'
            | b'?'
            | b'+'
            | b'-'
            | b'*'
            | b'/'
            | b'%'
            | b'<'
            | b'>'
            | b'='
    )
}

fn is_non_sql_operator_char(b: u8) -> bool {
    matches!(
        b,
        b'~' | b'!' | b'@' | b'#' | b'^' | b'&' | b'|' | b'`' | b'?' | b'%'
    )
}

fn two_byte_token_kind(bytes: &[u8]) -> Option<TokenKind> {
    match bytes {
        b"::" => Some(TokenKind::TypeCast),
        b".." => Some(TokenKind::DotDot),
        b":=" => Some(TokenKind::ColonEquals),
        b"=>" => Some(TokenKind::EqualsGreater),
        b"<=" => Some(TokenKind::LessEquals),
        b">=" => Some(TokenKind::GreaterEquals),
        b"<>" | b"!=" => Some(TokenKind::NotEquals),
        b"->" => Some(TokenKind::RightArrow),
        _ => None,
    }
}

fn is_digit_for_radix(b: u8, radix: u32) -> bool {
    match radix {
        2 => matches!(b, b'0' | b'1'),
        8 => matches!(b, b'0'..=b'7'),
        10 => b.is_ascii_digit(),
        16 => b.is_ascii_hexdigit(),
        _ => false,
    }
}

fn prefix_for_radix(radix: u32) -> &'static str {
    match radix {
        2 => "0b",
        8 => "0o",
        10 => "",
        16 => "0x",
        _ => unreachable!("unsupported integer radix {radix}"),
    }
}

fn downcase_truncate_identifier(raw: &str) -> std::string::String {
    truncate_identifier(&raw.to_ascii_lowercase())
}

fn truncate_identifier(raw: &str) -> std::string::String {
    let max = NAMEDATALEN - 1;
    if raw.len() <= max {
        return raw.to_owned();
    }
    let mut end = max;
    while !raw.is_char_boundary(end) {
        end -= 1;
    }
    raw[..end].to_owned()
}

fn string_from_bytes(bytes: Vec<u8>) -> std::string::String {
    match std::string::String::from_utf8(bytes) {
        Ok(value) => value,
        Err(err) => std::string::String::from_utf8_lossy(err.as_bytes()).into_owned(),
    }
}

fn is_high_surrogate(value: u32) -> bool {
    (0xD800..=0xDBFF).contains(&value)
}

fn is_low_surrogate(value: u32) -> bool {
    (0xDC00..=0xDFFF).contains(&value)
}

fn push_codepoint(
    literal: &mut Vec<u8>,
    codepoint: u32,
    escape_start: usize,
    escape_end: usize,
) -> Result<(), LexError> {
    let Some(ch) = char::from_u32(codepoint) else {
        return Err(LexError::ranged(
            escape_start,
            escape_end,
            "invalid Unicode escape value",
        ));
    };
    let mut encoded = [0; 4];
    literal.extend_from_slice(ch.encode_utf8(&mut encoded).as_bytes());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SourceText;

    fn kinds(sql: &str) -> Vec<TokenKind> {
        lex(sql)
            .unwrap()
            .into_iter()
            .map(|token| token.kind)
            .collect()
    }

    #[test]
    fn lexes_keywords_identifiers_and_punctuation() {
        assert_eq!(
            kinds("SELECT foo, $1 FROM bar::int"),
            vec![
                TokenKind::Select,
                TokenKind::Ident,
                TokenKind::Char(','),
                TokenKind::Param,
                TokenKind::From,
                TokenKind::Ident,
                TokenKind::TypeCast,
                TokenKind::IntP,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_standard_extended_and_dollar_strings() {
        let tokens = lex("'a''b' E'c\\n' $$raw$$").unwrap();
        assert_eq!(tokens[0].value, Some(TokenValue::String("a'b".into())));
        assert_eq!(tokens[1].value, Some(TokenValue::String("c\n".into())));
        assert_eq!(tokens[2].value, Some(TokenValue::String("raw".into())));
    }

    #[test]
    fn lexes_extended_string_byte_and_unicode_escapes() {
        let tokens = lex(r"E'\101\x42\u0043\U00000044\uD83D\uDE00'").unwrap();
        assert_eq!(tokens[0].value, Some(TokenValue::String("ABCD😀".into())));
    }

    #[test]
    fn concatenates_adjacent_strings_only_across_newline() {
        let tokens = lex("'a'
'b'")
        .unwrap();
        assert_eq!(tokens[0].value, Some(TokenValue::String("ab".into())));
        assert_eq!(tokens[1].kind, TokenKind::Eof);

        let tokens = lex("'a' 'b'").unwrap();
        assert_eq!(tokens[0].value, Some(TokenValue::String("a".into())));
        assert_eq!(tokens[1].value, Some(TokenValue::String("b".into())));
    }

    #[test]
    fn distinguishes_dollar_quotes_parameters_and_bare_dollars() {
        let tokens = lex("$tag$body$tag$ $42 $foo").unwrap();
        assert_eq!(tokens[0].value, Some(TokenValue::String("body".into())));
        assert_eq!(tokens[1].value, Some(TokenValue::Integer(42)));
        assert_eq!(tokens[2].kind, TokenKind::Char('$'));
        assert_eq!(tokens[3].value, Some(TokenValue::String("foo".into())));
    }

    #[test]
    fn lexes_prefixed_numbers_and_numeric_fail() {
        let tokens = lex("0x10 0X11 0o10 0b10 1..10 1.5e2").unwrap();
        assert_eq!(tokens[0].value, Some(TokenValue::Integer(16)));
        assert_eq!(tokens[1].value, Some(TokenValue::Integer(17)));
        assert_eq!(tokens[2].value, Some(TokenValue::Integer(8)));
        assert_eq!(tokens[3].value, Some(TokenValue::Integer(2)));
        assert_eq!(tokens[4].value, Some(TokenValue::Integer(1)));
        assert_eq!(tokens[5].kind, TokenKind::DotDot);
        assert_eq!(tokens[6].value, Some(TokenValue::Integer(10)));
        assert_eq!(tokens[7].kind, TokenKind::FConst);
    }

    #[test]
    fn preserves_integer_separators_and_out_of_range_lexemes() {
        let tokens = lex("1_000 0x_FF 2147483648").unwrap();
        assert_eq!(tokens[0].value, Some(TokenValue::Integer(1000)));
        assert_eq!(tokens[1].value, Some(TokenValue::Integer(255)));
        assert_eq!(tokens[2].kind, TokenKind::FConst);
        assert_eq!(
            tokens[2].value,
            Some(TokenValue::String("2147483648".into()))
        );
    }

    #[test]
    fn handles_nested_comments_and_operator_comment_boundaries() {
        assert_eq!(
            kinds("1 /* outer /* inner */ done */ +/*comment*/ 2"),
            vec![
                TokenKind::IConst,
                TokenKind::Char('+'),
                TokenKind::IConst,
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn applies_postgres_trailing_sign_rules_to_operators() {
        assert_eq!(
            kinds("a =- b ?- c >=- d"),
            vec![
                TokenKind::Ident,
                TokenKind::Char('='),
                TokenKind::Char('-'),
                TokenKind::Ident,
                TokenKind::Op,
                TokenKind::Ident,
                TokenKind::GreaterEquals,
                TokenKind::Char('-'),
                TokenKind::Ident,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn rejects_trailing_numeric_junk() {
        assert!(lex("123abc").is_err());
        assert!(lex("$1abc").is_err());
    }

    #[test]
    fn token_ranges_cover_complete_utf8_lexemes_and_skip_trivia() {
        let sql = "  select /* comment */ 中文::text";
        let tokens = lex(sql).unwrap();
        let source = SourceText::new(sql).unwrap();
        let lexemes = tokens
            .iter()
            .map(|token| source.slice(token.range).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(lexemes, ["select", "中文", "::", "text", ""]);
        assert_eq!(tokens[1].location(), sql.find("中文").unwrap());
        assert_eq!(tokens[1].end_location(), sql.find("中文").unwrap() + 6);
    }

    #[test]
    fn lexical_error_ranges_cover_the_invalid_construct() {
        let sql = "select 'unterminated";
        let error = lex(sql).unwrap_err();
        assert_eq!(error.location(), sql.find('\'').unwrap());
        assert_eq!(usize::from(error.range.end()), sql.len());
    }

    #[test]
    fn completion_tokenization_recovers_only_at_or_after_the_point() {
        let sql = "select  from \"unfinished";
        let point = TextSize::new(7);
        let recovered = lex_for_completion(sql, point).unwrap();
        assert!(recovered.recovered_error().is_some());
        assert!(
            recovered
                .tokens()
                .iter()
                .any(|token| token.kind == TokenKind::Incomplete)
        );

        let earlier_error = "select 1e+ from users";
        let point = TextSize::try_from(earlier_error.len()).unwrap();
        assert!(lex_for_completion(earlier_error, point).is_err());
    }

    #[test]
    fn strict_lexing_remains_strict_for_completion_input() {
        let sql = "select \"unfinished";
        assert!(lex(sql).is_err());
        assert!(lex_for_completion(sql, TextSize::new(7)).is_ok());
        assert!(lex(sql).is_err());
    }
}
