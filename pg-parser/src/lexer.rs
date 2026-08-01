// Translated by hand from PostgreSQL's src/backend/parser/scan.l semantics.
// Token names come from gram.y and keyword mappings from parser/kwlist.h.
use crate::{BareLabel, KEYWORDS, KeywordCategory, TokenKind};
use crate::{TextRange, TextSize};
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

// pub static KEYWORDS: &[Keyword] = &;

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
        let done = token.kind == TokenKind::Eof;
        tokens.push(token);
        if done {
            return Ok(tokens);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
/// Tokens produced for editor input, including any lexical error recovered at
/// the completion point.
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
    let point = TextSize::try_from(usize::from(point).min(input.len()))
        .expect("completion point was bounded by input length");
    match lex(input) {
        Ok(tokens) => Ok(CompletionTokenization {
            tokens,
            recovered_error: None,
        }),
        Err(error) if error.range.end() >= point => {
            let safe_end = usize::from(error.range.start()).min(input.len());
            let mut tokens = lex(&input[..safe_end])?;
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
        let mut token = self.next_token_unranged()?;
        token.finish(self.pos);
        Ok(token)
    }

    fn next_token_unranged(&mut self) -> Result<Token, LexError> {
        self.skip_whitespace_and_comments()?;
        let location = self.pos;
        if self.eof() {
            return Ok(Token::new(TokenKind::Eof, location));
        }

        if self.starts_with_ignore_ascii_case("b'") {
            self.pos += 2;
            return self.scan_bit_or_hex_string(location, b'b', TokenKind::BConst);
        }
        if self.starts_with_ignore_ascii_case("x'") {
            self.pos += 2;
            return self.scan_bit_or_hex_string(location, b'x', TokenKind::XConst);
        }
        if self.starts_with_ignore_ascii_case("n'") {
            self.pos += 1;
            return Ok(Token::keyword(TokenKind::Nchar, location, "nchar"));
        }
        if self.starts_with_ignore_ascii_case("e'") {
            self.pos += 2;
            return self.scan_quoted_string(location, StringMode::Extended, TokenKind::SConst);
        }
        if self.starts_with_ignore_ascii_case("u&\"") {
            self.pos += 3;
            return self.scan_quoted_identifier(location, true);
        }
        if self.starts_with_ignore_ascii_case("u&'") {
            self.pos += 3;
            return self.scan_quoted_string(location, StringMode::Unicode, TokenKind::USConst);
        }
        if self.starts_with_ignore_ascii_case("u&") {
            self.pos += 1;
            return Ok(Token::string(TokenKind::Ident, location, "u"));
        }
        if self.peek() == Some(b'\'') {
            self.pos += 1;
            return self.scan_quoted_string(location, StringMode::Standard, TokenKind::SConst);
        }
        if self.peek() == Some(b'"') {
            self.pos += 1;
            return self.scan_quoted_identifier(location, false);
        }
        if self.peek() == Some(b'$')
            && let Some(token) = self.try_scan_dollar_or_param(location)?
        {
            return Ok(token);
        }

        if self.starts_with("::") {
            self.pos += 2;
            return Ok(Token::new(TokenKind::TypeCast, location));
        }
        if self.starts_with("..") {
            self.pos += 2;
            return Ok(Token::new(TokenKind::DotDot, location));
        }
        if self.starts_with(":=") {
            self.pos += 2;
            return Ok(Token::new(TokenKind::ColonEquals, location));
        }
        if self.starts_with("=>") {
            self.pos += 2;
            return Ok(Token::new(TokenKind::EqualsGreater, location));
        }
        if self.starts_with("<=") {
            self.pos += 2;
            return Ok(Token::new(TokenKind::LessEquals, location));
        }
        if self.starts_with(">=") {
            self.pos += 2;
            return Ok(Token::new(TokenKind::GreaterEquals, location));
        }
        if self.starts_with("<>") || self.starts_with("!=") {
            self.pos += 2;
            return Ok(Token::new(TokenKind::NotEquals, location));
        }
        if self.starts_with("->") {
            self.pos += 2;
            return Ok(Token::new(TokenKind::RightArrow, location));
        }

        if self.peek().is_some_and(is_dec_digit)
            || (self.peek() == Some(b'.') && self.peek_n(1).is_some_and(is_dec_digit))
        {
            return self.scan_number(location);
        }

        if self.peek().is_some_and(is_ident_start) {
            return Ok(self.scan_identifier_or_keyword(location));
        }

        if self.peek().is_some_and(is_operator_char) {
            return self.scan_operator(location);
        }

        if self.peek().is_some_and(is_self_char) {
            let ch = self.bump_ascii_char().unwrap();
            return Ok(Token::new(TokenKind::Char(ch), location));
        }

        let ch = self.bump_char().unwrap_or('\0');
        Ok(Token::new(TokenKind::Char(ch), location))
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
        location: usize,
        message: impl Into<std::string::String>,
    ) -> Result<T, LexError> {
        Err(LexError::ranged(location, self.pos, message))
    }

    fn skip_whitespace_and_comments(&mut self) -> Result<(), LexError> {
        loop {
            let start = self.pos;
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
            if self.pos == start {
                return Ok(());
            }
        }
    }

    fn skip_block_comment(&mut self) -> Result<(), LexError> {
        let location = self.pos;
        self.pos += 2;
        let mut depth = 0usize;
        while !self.eof() {
            if self.starts_with("/*") {
                depth += 1;
                self.pos += 2;
            } else if self.starts_with("*/") {
                self.pos += 2;
                if depth == 0 {
                    return Ok(());
                }
                depth -= 1;
            } else {
                self.pos += 1;
            }
        }
        self.error(location, "unterminated /* comment")
    }

    fn scan_bit_or_hex_string(
        &mut self,
        location: usize,
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
                let msg = if prefix == b'b' {
                    "unterminated bit string literal"
                } else {
                    "unterminated hexadecimal string literal"
                };
                return self.error(location, msg);
            }
            self.pos += 1;
            if self.consume_quote_continuation() {
                continue;
            }
            return Ok(Token::string(kind, location, string_from_bytes(literal)));
        }
    }

    fn scan_quoted_string(
        &mut self,
        location: usize,
        mode: StringMode,
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
                if self.consume_quote_continuation() {
                    continue;
                }
                return Ok(Token::string(kind, location, string_from_bytes(literal)));
            }

            if mode == StringMode::Extended && b == b'\\' {
                self.pos += 1;
                self.scan_escape_sequence(location, &mut literal)?;
                continue;
            }

            literal.push(b);
            self.pos += 1;
        }
        self.error(location, "unterminated quoted string")
    }

    fn scan_escape_sequence(
        &mut self,
        location: usize,
        literal: &mut Vec<u8>,
    ) -> Result<(), LexError> {
        let Some(next) = self.peek() else {
            literal.push(b'\\');
            return Ok(());
        };

        if next == b'u' || next == b'U' {
            let escape_location = self.pos - 1;
            let width = if next == b'u' { 4 } else { 8 };
            self.pos += 1;
            let first = self.read_fixed_hex_escape(escape_location, width)?;
            if is_utf16_surrogate_first(first) {
                if !(self.peek() == Some(b'\\') && matches!(self.peek_n(1), Some(b'u' | b'U'))) {
                    return self.error(self.pos, "invalid Unicode surrogate pair");
                }
                self.pos += 1;
                let second_width = if self.peek() == Some(b'u') { 4 } else { 8 };
                self.pos += 1;
                let second = self.read_fixed_hex_escape(self.pos - 2, second_width)?;
                if !is_utf16_surrogate_second(second) {
                    return self.error(self.pos, "invalid Unicode surrogate pair");
                }
                let codepoint = 0x10000 + (((first - 0xD800) << 10) | (second - 0xDC00));
                push_codepoint(literal, codepoint, escape_location, self.pos)?;
            } else if is_utf16_surrogate_second(first) {
                return self.error(escape_location, "invalid Unicode surrogate pair");
            } else {
                push_codepoint(literal, first, escape_location, self.pos)?;
            }
            return Ok(());
        }

        if (b'0'..=b'7').contains(&next) {
            let start = self.pos;
            let mut end = self.pos;
            for _ in 0..3 {
                if self
                    .bytes
                    .get(end)
                    .is_some_and(|b| (b'0'..=b'7').contains(b))
                {
                    end += 1;
                } else {
                    break;
                }
            }
            let value = u8::from_str_radix(&self.input[start..end], 8).unwrap();
            literal.push(value);
            self.pos = end;
            return Ok(());
        }

        if next == b'x' {
            let start = self.pos + 1;
            let mut end = start;
            for _ in 0..2 {
                if self.bytes.get(end).is_some_and(|b| b.is_ascii_hexdigit()) {
                    end += 1;
                } else {
                    break;
                }
            }
            if end > start {
                let value = u8::from_str_radix(&self.input[start..end], 16).unwrap();
                literal.push(value);
                self.pos = end;
                return Ok(());
            }
        }

        self.pos += 1;
        literal.push(match next {
            b'b' => 0x08,
            b'f' => 0x0C,
            b'n' => b'\n',
            b'r' => b'\n',
            b't' => b'\t',
            b'v' => 0x0B,
            other => other,
        });
        let _ = location;
        Ok(())
    }

    fn read_fixed_hex_escape(&mut self, location: usize, width: usize) -> Result<u32, LexError> {
        let end = self.pos + width;
        if end > self.bytes.len() || !self.bytes[self.pos..end].iter().all(u8::is_ascii_hexdigit) {
            return self.error(location, "invalid Unicode escape");
        }
        let value = u32::from_str_radix(&self.input[self.pos..end], 16).unwrap();
        self.pos = end;
        Ok(value)
    }

    fn consume_quote_continuation(&mut self) -> bool {
        let after_quote = self.pos;
        let mut pos = self.pos;
        let mut saw_newline = false;
        loop {
            match self.bytes.get(pos).copied() {
                Some(b'\n' | b'\r') => {
                    saw_newline = true;
                    pos += 1;
                }
                Some(b' ' | b'\t' | 0x0C | 0x0B) => pos += 1,
                Some(b'-') if self.bytes.get(pos + 1) == Some(&b'-') => {
                    pos += 2;
                    while let Some(b) = self.bytes.get(pos).copied() {
                        if b == b'\n' || b == b'\r' {
                            break;
                        }
                        pos += 1;
                    }
                }
                _ => break,
            }
        }
        if saw_newline && self.bytes.get(pos) == Some(&b'\'') {
            self.pos = pos + 1;
            true
        } else {
            self.pos = after_quote;
            false
        }
    }

    fn try_scan_dollar_or_param(&mut self, location: usize) -> Result<Option<Token>, LexError> {
        if let Some(delim_end) = self.dollar_delimiter_end(self.pos) {
            let delimiter = &self.input[self.pos..delim_end];
            self.pos = delim_end;
            let content_start = self.pos;
            if let Some(relative_end) = self.input[self.pos..].find(delimiter) {
                let content_end = content_start + relative_end;
                let value = self.input[content_start..content_end].to_owned();
                self.pos = content_end + delimiter.len();
                return Ok(Some(Token::string(TokenKind::SConst, location, value)));
            }
            return self.error(location, "unterminated dollar-quoted string");
        }

        if self.peek_n(1).is_some_and(is_ident_start) {
            self.pos += 1;
            return Ok(Some(Token::new(TokenKind::Char('$'), location)));
        }

        if self.peek_n(1).is_some_and(is_dec_digit) {
            self.pos += 1;
            let start = self.pos;
            while self.peek().is_some_and(is_dec_digit) {
                self.pos += 1;
            }
            if self.peek().is_some_and(is_ident_start) {
                return self.error(location, "trailing junk after parameter");
            }
            let raw = &self.input[start..self.pos];
            let value = raw
                .parse::<i32>()
                .map_err(|_| LexError::ranged(location, self.pos, "parameter number too large"))?;
            return Ok(Some(Token::integer(TokenKind::Param, location, value)));
        }

        Ok(None)
    }

    fn dollar_delimiter_end(&self, start: usize) -> Option<usize> {
        if self.bytes.get(start) != Some(&b'$') {
            return None;
        }
        let mut pos = start + 1;
        if self.bytes.get(pos) == Some(&b'$') {
            return Some(pos + 1);
        }
        if !self.bytes.get(pos).is_some_and(|b| is_dolq_start(*b)) {
            return None;
        }
        pos += 1;
        while self.bytes.get(pos).is_some_and(|b| is_dolq_cont(*b)) {
            pos += 1;
        }
        if self.bytes.get(pos) == Some(&b'$') {
            Some(pos + 1)
        } else {
            None
        }
    }

    fn scan_quoted_identifier(
        &mut self,
        location: usize,
        unicode: bool,
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
                    return self.error(location, "zero-length delimited identifier");
                }
                let ident = truncate_identifier(&string_from_bytes(literal));
                let kind = if unicode {
                    TokenKind::UIdent
                } else {
                    TokenKind::Ident
                };
                return Ok(Token::string(kind, location, ident));
            }
            literal.push(b);
            self.pos += 1;
        }
        self.error(location, "unterminated quoted identifier")
    }

    fn scan_identifier_or_keyword(&mut self, location: usize) -> Token {
        let start = self.pos;
        self.pos += 1;
        while self.peek().is_some_and(is_ident_cont) {
            self.pos += 1;
        }
        let raw = &self.input[start..self.pos];
        if let Some(keyword) = lookup_keyword(raw) {
            return Token::keyword(keyword.kind, location, keyword.word);
        }
        Token::string(
            TokenKind::Ident,
            location,
            downcase_truncate_identifier(raw),
        )
    }

    fn scan_number(&mut self, location: usize) -> Result<Token, LexError> {
        let start = self.pos;
        if self.peek() == Some(b'.') {
            self.pos += 1;
            self.scan_decinteger_tail();
            self.scan_exponent(location)?;
            self.reject_numeric_junk(location)?;
            return Ok(Token::string(
                TokenKind::FConst,
                location,
                &self.input[start..self.pos],
            ));
        }

        if self.starts_with_ignore_ascii_case("0x") {
            return self.scan_prefixed_integer(location, 16, "invalid hexadecimal integer");
        }
        if self.starts_with_ignore_ascii_case("0o") {
            return self.scan_prefixed_integer(location, 8, "invalid octal integer");
        }
        if self.starts_with_ignore_ascii_case("0b") {
            return self.scan_prefixed_integer(location, 2, "invalid binary integer");
        }

        self.pos += 1;
        self.scan_decinteger_tail();

        if self.starts_with("..") {
            return self.integer_or_float(location, start, 10, "");
        }

        let mut is_float = false;
        if self.peek() == Some(b'.') {
            is_float = true;
            self.pos += 1;
            if self.peek().is_some_and(is_dec_digit) {
                self.pos += 1;
                self.scan_decinteger_tail();
            }
        }
        if self.peek().is_some_and(|b| b == b'e' || b == b'E') {
            is_float = true;
            self.scan_exponent(location)?;
        }
        self.reject_numeric_junk(location)?;
        if is_float {
            Ok(Token::string(
                TokenKind::FConst,
                location,
                &self.input[start..self.pos],
            ))
        } else {
            self.integer_or_float(location, start, 10, "")
        }
    }

    fn scan_prefixed_integer(
        &mut self,
        location: usize,
        radix: u32,
        fail_message: &'static str,
    ) -> Result<Token, LexError> {
        let start = self.pos;
        self.pos += 2;
        let digit_start = self.pos;
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
        if !saw_digit || self.pos == digit_start {
            return self.error(location, fail_message);
        }
        self.reject_numeric_junk(location)?;
        self.integer_or_float(location, start, radix, prefix_for_radix(radix))
    }

    fn scan_decinteger_tail(&mut self) {
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

    fn scan_exponent(&mut self, location: usize) -> Result<(), LexError> {
        if !self.peek().is_some_and(|b| b == b'e' || b == b'E') {
            return Ok(());
        }
        let save = self.pos;
        self.pos += 1;
        if self.peek().is_some_and(|b| b == b'+' || b == b'-') {
            self.pos += 1;
        }
        if !self.peek().is_some_and(is_dec_digit) {
            self.pos = save;
            return self.error(location, "trailing junk after numeric literal");
        }
        self.pos += 1;
        self.scan_decinteger_tail();
        Ok(())
    }

    fn reject_numeric_junk(&self, location: usize) -> Result<(), LexError> {
        if self.peek().is_some_and(is_ident_start) {
            return Err(LexError::ranged(
                location,
                self.pos,
                "trailing junk after numeric literal",
            ));
        }
        Ok(())
    }

    fn integer_or_float(
        &self,
        location: usize,
        start: usize,
        radix: u32,
        prefix: &str,
    ) -> Result<Token, LexError> {
        let raw = &self.input[start..self.pos];
        let cleaned = raw.replace('_', "");
        let digits = if radix == 10 {
            cleaned.as_str()
        } else {
            cleaned.get(prefix.len()..).unwrap_or(cleaned.as_str())
        };
        match i32::from_str_radix(digits, radix) {
            Ok(value) => Ok(Token::integer(TokenKind::IConst, location, value)),
            Err(_) => Ok(Token::string(TokenKind::FConst, location, raw)),
        }
    }

    fn scan_operator(&mut self, location: usize) -> Result<Token, LexError> {
        let start = self.pos;
        while self.peek().is_some_and(is_operator_char) {
            if self.starts_with("/*") || self.starts_with("--") {
                break;
            }
            self.pos += 1;
        }
        let mut end = self.pos;

        if end - start > 1 {
            let bytes = &self.bytes[start..end];
            if matches!(bytes.last(), Some(b'+' | b'-')) {
                let has_non_sql = bytes[..bytes.len() - 1].iter().any(|b| {
                    matches!(
                        b,
                        b'~' | b'!' | b'@' | b'#' | b'^' | b'&' | b'|' | b'`' | b'?' | b'%'
                    )
                });
                if !has_non_sql {
                    while end - start > 1 && matches!(self.bytes[end - 1], b'+' | b'-') {
                        end -= 1;
                    }
                    self.pos = end;
                }
            }
        }

        let op = &self.input[start..end];
        if op.len() == 1 {
            let b = op.as_bytes()[0];
            if is_self_char(b) {
                return Ok(Token::new(TokenKind::Char(b as char), location));
            }
        }
        if op.len() == 2 {
            let kind = match op {
                "=>" => Some(TokenKind::EqualsGreater),
                ">=" => Some(TokenKind::GreaterEquals),
                "<=" => Some(TokenKind::LessEquals),
                "<>" | "!=" => Some(TokenKind::NotEquals),
                "->" => Some(TokenKind::RightArrow),
                _ => None,
            };
            if let Some(kind) = kind {
                return Ok(Token::new(kind, location));
            }
        }
        if op.len() >= NAMEDATALEN {
            return self.error(location, "operator too long");
        }
        Ok(Token::string(TokenKind::Op, location, op))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StringMode {
    Standard,
    Extended,
    Unicode,
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

fn is_dolq_start(b: u8) -> bool {
    is_ident_start(b)
}

fn is_dolq_cont(b: u8) -> bool {
    is_ident_start(b) || b.is_ascii_digit()
}

fn is_self_char(b: u8) -> bool {
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
        16 => "0x",
        _ => "",
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

fn is_utf16_surrogate_first(c: u32) -> bool {
    (0xD800..=0xDBFF).contains(&c)
}

fn is_utf16_surrogate_second(c: u32) -> bool {
    (0xDC00..=0xDFFF).contains(&c)
}

fn push_codepoint(
    literal: &mut Vec<u8>,
    codepoint: u32,
    location: usize,
    end: usize,
) -> Result<(), LexError> {
    let Some(ch) = char::from_u32(codepoint) else {
        return Err(LexError::ranged(
            location,
            end,
            "invalid Unicode escape value",
        ));
    };
    let mut buf = [0; 4];
    literal.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
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
    fn concatenates_adjacent_strings_only_across_newline() {
        let tokens = lex("'a'
'b'")
        .unwrap();
        assert_eq!(tokens[0].value, Some(TokenValue::String("ab".into())));
        assert_eq!(tokens[1].kind, TokenKind::Eof);
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
