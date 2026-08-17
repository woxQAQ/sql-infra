//! Top-level SQL statement scanning without per-token allocation.
//!
//! This module recognizes only lexical constructs that can contain or affect
//! semicolons. It deliberately avoids full token construction, literal
//! decoding, numeric validation, and keyword-table lookup.

use crate::LexError;
use crate::TextRange;
use crate::TextSize;
use crate::parser::StatementRange;

/// Split SQL into statement ranges without parsing its grammar.
///
/// Semicolons inside strings, quoted identifiers, dollar-quoted strings,
/// comments, parentheses, brackets, and `BEGIN ATOMIC` routine bodies do not
/// terminate a statement. Empty statements are omitted. The `syntax` range
/// excludes the terminating semicolon; `terminator` contains it when present.
///
/// Unterminated strings, quoted identifiers, dollar-quoted strings, and block
/// comments return a [`LexError`]. Other SQL grammar is not validated.
pub fn split_statement_ranges(sql: &str) -> Result<Vec<StatementRange>, LexError> {
    Splitter::new(sql)?.split()
}

struct Splitter<'a> {
    sql: &'a str,
    bytes: &'a [u8],
    pos: usize,
    statement_start: Option<usize>,
    delimiter_depth: usize,
    atomic_depth: usize,
    case_depth: usize,
    pending_begin: bool,
    ranges: Vec<StatementRange>,
}

impl<'a> Splitter<'a> {
    fn new(sql: &'a str) -> Result<Self, LexError> {
        TextSize::try_from(sql.len()).map_err(|error| LexError {
            message: error.to_string(),
            range: TextRange::empty(TextSize::ZERO),
        })?;
        Ok(Self {
            sql,
            bytes: sql.as_bytes(),
            pos: 0,
            statement_start: None,
            delimiter_depth: 0,
            atomic_depth: 0,
            case_depth: 0,
            pending_begin: false,
            ranges: Vec::new(),
        })
    }

    fn split(mut self) -> Result<Vec<StatementRange>, LexError> {
        while self.pos < self.bytes.len() {
            self.skip_trivia()?;
            if self.pos == self.bytes.len() {
                break;
            }

            let start = self.pos;
            let is_empty_statement_terminator =
                self.bytes[self.pos] == b';' && self.delimiter_depth == 0 && self.atomic_depth == 0;
            if !is_empty_statement_terminator {
                self.statement_start.get_or_insert(start);
            }

            if self.scan_prefixed_quote()? || self.scan_dollar_quote_or_parameter()? {
                self.pending_begin = false;
                continue;
            }

            match self.bytes[self.pos] {
                b'\'' => {
                    self.pos += 1;
                    self.scan_single_quote(start, false, "unterminated quoted string")?;
                    self.pending_begin = false;
                }
                b'"' => {
                    self.pos += 1;
                    self.scan_quoted_identifier(start)?;
                    self.pending_begin = false;
                }
                b'(' | b'[' => {
                    self.pos += 1;
                    self.delimiter_depth += 1;
                    self.pending_begin = false;
                }
                b')' | b']' => {
                    self.pos += 1;
                    self.delimiter_depth = self.delimiter_depth.saturating_sub(1);
                    self.pending_begin = false;
                }
                b';' if self.delimiter_depth == 0 && self.atomic_depth == 0 => {
                    self.pos += 1;
                    self.finish_statement(start);
                }
                byte if is_ident_start(byte) => self.scan_word(),
                _ => {
                    self.pos += 1;
                    self.pending_begin = false;
                }
            }
        }

        if let Some(start) = self.statement_start {
            self.ranges.push(StatementRange {
                syntax: self.range(start, self.sql.len()),
                terminator: None,
            });
        }
        Ok(self.ranges)
    }

    fn skip_trivia(&mut self) -> Result<(), LexError> {
        loop {
            while self.peek().is_some_and(is_space) {
                self.pos += 1;
            }
            if self.starts_with(b"--") {
                self.pos += 2;
                while self
                    .peek()
                    .is_some_and(|byte| !matches!(byte, b'\n' | b'\r'))
                {
                    self.pos += 1;
                }
            } else if self.starts_with(b"/*") {
                self.skip_block_comment()?;
            } else {
                return Ok(());
            }
        }
    }

    fn skip_block_comment(&mut self) -> Result<(), LexError> {
        let start = self.pos;
        self.pos += 2;
        let mut depth = 1usize;
        while self.pos < self.bytes.len() {
            if self.starts_with(b"/*") {
                depth += 1;
                self.pos += 2;
            } else if self.starts_with(b"*/") {
                depth -= 1;
                self.pos += 2;
                if depth == 0 {
                    return Ok(());
                }
            } else {
                self.pos += 1;
            }
        }
        Err(self.error(start, "unterminated /* comment"))
    }

    fn scan_prefixed_quote(&mut self) -> Result<bool, LexError> {
        let start = self.pos;
        let Some(first) = self.peek() else {
            return Ok(false);
        };
        let lower = first.to_ascii_lowercase();

        if matches!(lower, b'e' | b'b' | b'x' | b'n') && self.peek_n(1) == Some(b'\'') {
            self.pos += 2;
            let backslash_escapes = lower == b'e';
            let message = match lower {
                b'b' => "unterminated bit string literal",
                b'x' => "unterminated hexadecimal string literal",
                _ => "unterminated quoted string",
            };
            self.scan_single_quote(start, backslash_escapes, message)?;
            return Ok(true);
        }

        if lower == b'u' && self.peek_n(1) == Some(b'&') {
            match self.peek_n(2) {
                Some(b'\'') => {
                    self.pos += 3;
                    self.scan_single_quote(start, false, "unterminated quoted string")?;
                    return Ok(true);
                }
                Some(b'"') => {
                    self.pos += 3;
                    self.scan_quoted_identifier(start)?;
                    return Ok(true);
                }
                _ => {}
            }
        }
        Ok(false)
    }

    fn scan_single_quote(
        &mut self,
        start: usize,
        backslash_escapes: bool,
        message: &'static str,
    ) -> Result<(), LexError> {
        while let Some(byte) = self.peek() {
            if backslash_escapes && byte == b'\\' {
                self.pos = (self.pos + 2).min(self.bytes.len());
            } else if byte != b'\'' {
                self.pos += 1;
            } else if self.peek_n(1) == Some(b'\'') {
                self.pos += 2;
            } else {
                self.pos += 1;
                if let Some(continuation) = self.string_continuation() {
                    self.pos = continuation;
                } else {
                    return Ok(());
                }
            }
        }
        Err(self.error(start, message))
    }

    fn string_continuation(&self) -> Option<usize> {
        let mut pos = self.pos;
        let mut saw_newline = false;
        loop {
            match self.bytes.get(pos).copied() {
                Some(b'\n' | b'\r') => {
                    saw_newline = true;
                    pos += 1;
                }
                Some(b' ' | b'\t' | 0x0b | 0x0c) => pos += 1,
                Some(b'-') if self.bytes.get(pos + 1) == Some(&b'-') => {
                    pos += 2;
                    while self
                        .bytes
                        .get(pos)
                        .is_some_and(|byte| !matches!(byte, b'\n' | b'\r'))
                    {
                        pos += 1;
                    }
                }
                _ => break,
            }
        }
        (saw_newline && self.bytes.get(pos) == Some(&b'\'')).then_some(pos + 1)
    }

    fn scan_quoted_identifier(&mut self, start: usize) -> Result<(), LexError> {
        let content_start = self.pos;
        while let Some(byte) = self.peek() {
            if byte != b'"' {
                self.pos += 1;
            } else if self.peek_n(1) == Some(b'"') {
                self.pos += 2;
            } else if self.pos == content_start {
                self.pos += 1;
                return Err(self.error(start, "zero-length delimited identifier"));
            } else {
                self.pos += 1;
                return Ok(());
            }
        }
        Err(self.error(start, "unterminated quoted identifier"))
    }

    fn scan_dollar_quote_or_parameter(&mut self) -> Result<bool, LexError> {
        if self.peek() != Some(b'$') {
            return Ok(false);
        }
        let start = self.pos;
        if let Some(delimiter_end) = self.dollar_quote_delimiter_end() {
            let delimiter = &self.sql[start..delimiter_end];
            let content_start = delimiter_end;
            if let Some(offset) = self.sql[content_start..].find(delimiter) {
                self.pos = content_start + offset + delimiter.len();
                return Ok(true);
            }
            self.pos = self.bytes.len();
            return Err(self.error(start, "unterminated dollar-quoted string"));
        }

        if self.peek_n(1).is_some_and(|byte| byte.is_ascii_digit()) {
            self.pos += 2;
            while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                self.pos += 1;
            }
            if self.peek().is_some_and(is_ident_start) {
                return Err(self.error(start, "trailing junk after parameter"));
            }
            return Ok(true);
        }
        Ok(false)
    }

    fn dollar_quote_delimiter_end(&self) -> Option<usize> {
        let mut end = self.pos + 1;
        if self.bytes.get(end) == Some(&b'$') {
            return Some(end + 1);
        }
        if !self.bytes.get(end).copied().is_some_and(is_ident_start) {
            return None;
        }
        end += 1;
        while self
            .bytes
            .get(end)
            .copied()
            .is_some_and(is_dollar_tag_continue)
        {
            end += 1;
        }
        (self.bytes.get(end) == Some(&b'$')).then_some(end + 1)
    }

    fn scan_word(&mut self) {
        let start = self.pos;
        self.pos += 1;
        while self.peek().is_some_and(is_ident_continue) {
            self.pos += 1;
        }
        let word = &self.sql[start..self.pos];

        if word.eq_ignore_ascii_case("atomic") && self.pending_begin {
            self.atomic_depth += 1;
            self.pending_begin = false;
        } else if word.eq_ignore_ascii_case("begin") {
            self.pending_begin = true;
        } else {
            self.pending_begin = false;
            if self.atomic_depth > 0 && word.eq_ignore_ascii_case("case") {
                self.case_depth += 1;
            } else if self.atomic_depth > 0 && word.eq_ignore_ascii_case("end") {
                if self.case_depth > 0 {
                    self.case_depth -= 1;
                } else {
                    self.atomic_depth -= 1;
                }
            }
        }
    }

    fn finish_statement(&mut self, semicolon: usize) {
        if let Some(start) = self.statement_start.take() {
            self.ranges.push(StatementRange {
                syntax: self.range(start, semicolon),
                terminator: Some(self.range(semicolon, semicolon + 1)),
            });
        }
        self.delimiter_depth = 0;
        self.atomic_depth = 0;
        self.case_depth = 0;
        self.pending_begin = false;
    }

    fn starts_with(&self, pattern: &[u8]) -> bool {
        self.bytes[self.pos..].starts_with(pattern)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek_n(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    fn range(&self, start: usize, end: usize) -> TextRange {
        TextRange::new(TextSize::from_usize(start), TextSize::from_usize(end))
    }

    fn error(&self, start: usize, message: impl Into<String>) -> LexError {
        LexError {
            message: message.into(),
            range: self.range(start, self.pos),
        }
    }
}

fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80
}

fn is_ident_continue(byte: u8) -> bool {
    is_ident_start(byte) || byte.is_ascii_digit() || byte == b'$'
}

fn is_dollar_tag_continue(byte: u8) -> bool {
    is_ident_start(byte) || byte.is_ascii_digit()
}
