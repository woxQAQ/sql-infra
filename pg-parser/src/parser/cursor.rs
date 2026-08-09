//! SQL cursor statements: `DECLARE`, `CLOSE`, `FETCH`, and `MOVE`.
//!
//! Direction and signed-count parsing is kept with the statements that interpret
//! those values.

use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-declare.html
    // DECLARE name [ BINARY ] [ ASENSITIVE | INSENSITIVE ] [ [ NO ] SCROLL ]
    //     CURSOR [ { WITH | WITHOUT } HOLD ] FOR query
    pub(super) fn parse_declare_cursor(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Declare)?;
        self.record_completion_slot(completion::GrammarSlot::AnyName);
        let portalname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("DECLARE requires a cursor name"))?,
        );
        let mut options = CURSOR_OPT_FAST_PLAN;
        loop {
            self.record_completion_tokens(&[
                TokenKind::No,
                TokenKind::Scroll,
                TokenKind::Binary,
                TokenKind::Insensitive,
                TokenKind::Asensitive,
                TokenKind::Cursor,
            ]);
            match self.peek_kind() {
                TokenKind::No => {
                    self.advance();
                    self.expect(TokenKind::Scroll)?;
                    options |= CURSOR_OPT_NO_SCROLL;
                }
                TokenKind::Scroll => {
                    self.advance();
                    options |= CURSOR_OPT_SCROLL;
                }
                TokenKind::Binary => {
                    self.advance();
                    options |= CURSOR_OPT_BINARY;
                }
                TokenKind::Insensitive => {
                    self.advance();
                    options |= CURSOR_OPT_INSENSITIVE;
                }
                TokenKind::Asensitive => {
                    self.advance();
                    options |= CURSOR_OPT_ASENSITIVE;
                }
                _ => break,
            }
        }
        self.expect(TokenKind::Cursor)?;
        if self.consume(TokenKind::With) {
            self.expect(TokenKind::Hold)?;
            options |= CURSOR_OPT_HOLD;
        } else if self.consume(TokenKind::Without) {
            self.expect(TokenKind::Hold)?;
        }
        self.expect(TokenKind::For)?;
        let query = if self.at(TokenKind::With) {
            self.parse_with_statement()?
        } else {
            Node::SelectStmt(self.parse_select(None)?)
        };
        if !matches!(query, Node::SelectStmt(_)) {
            return Err(self.error_here("DECLARE CURSOR query must be a SELECT statement"));
        }
        Ok(node!(DeclareCursorStmt {
            portalname,
            options,
            query: Some(Box::new(query)),
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-close.html
    // CLOSE { name | ALL }
    pub(super) fn parse_close(&mut self) -> PResult<Node> {
        self.expect(TokenKind::Close)?;
        self.record_completion_tokens(&[TokenKind::All]);
        let portalname = if self.consume(TokenKind::All) {
            None
        } else {
            self.record_completion_slot(completion::GrammarSlot::AnyName);
            Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("CLOSE requires a cursor name or ALL"))?,
            )
        };
        Ok(node!(ClosePortalStmt { portalname }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-fetch.html
    // FETCH [ direction ] [ FROM | IN ] cursor_name
    //
    // where direction can be one of:
    //
    //     NEXT
    //     PRIOR
    //     FIRST
    //     LAST
    //     ABSOLUTE count
    //     RELATIVE count
    //     count
    //     ALL
    //     FORWARD
    //     FORWARD count
    //     FORWARD ALL
    //     BACKWARD
    //     BACKWARD count
    //     BACKWARD ALL
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-move.html
    // MOVE [ direction ] [ FROM | IN ] cursor_name
    //
    // where direction can be one of:
    //
    //     NEXT
    //     PRIOR
    //     FIRST
    //     LAST
    //     ABSOLUTE count
    //     RELATIVE count
    //     count
    //     ALL
    //     FORWARD
    //     FORWARD count
    //     FORWARD ALL
    //     BACKWARD
    //     BACKWARD count
    //     BACKWARD ALL
    pub(super) fn parse_fetch_or_move(&mut self) -> PResult<Node> {
        let ismove = self.consume(TokenKind::Move);
        if !ismove {
            self.expect(TokenKind::Fetch)?;
        }
        let (direction, how_many, direction_keyword, location) = self.parse_fetch_direction()?;
        let _ = self.consume(TokenKind::From) || self.consume(TokenKind::InP);
        self.record_completion_slot(completion::GrammarSlot::AnyName);
        let portalname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("FETCH/MOVE requires a cursor name"))?,
        );
        Ok(node!(FetchStmt {
            direction,
            how_many,
            portalname,
            ismove,
            direction_keyword,
            location: location as ParseLoc,
        }))
    }

    pub(super) fn parse_fetch_direction(
        &mut self,
    ) -> PResult<(FetchDirection, i64, FetchDirectionKeywords, ParseLoc)> {
        self.record_completion_tokens(&[
            TokenKind::Next,
            TokenKind::Prior,
            TokenKind::FirstP,
            TokenKind::LastP,
            TokenKind::AbsoluteP,
            TokenKind::RelativeP,
            TokenKind::All,
            TokenKind::Forward,
            TokenKind::Backward,
            TokenKind::From,
            TokenKind::InP,
        ]);
        if self.consume(TokenKind::Next) {
            return Ok((FetchDirection::Forward, 1, FetchDirectionKeywords::Next, -1));
        }
        if self.consume(TokenKind::Prior) {
            return Ok((
                FetchDirection::Backward,
                1,
                FetchDirectionKeywords::Prior,
                -1,
            ));
        }
        if self.consume(TokenKind::FirstP) {
            return Ok((
                FetchDirection::Absolute,
                1,
                FetchDirectionKeywords::First,
                -1,
            ));
        }
        if self.consume(TokenKind::LastP) {
            return Ok((
                FetchDirection::Absolute,
                -1,
                FetchDirectionKeywords::Last,
                -1,
            ));
        }
        if self.consume(TokenKind::AbsoluteP) {
            let location = self.location() as ParseLoc;
            return Ok((
                FetchDirection::Absolute,
                self.parse_signed_fetch_count()?,
                FetchDirectionKeywords::Absolute,
                location,
            ));
        }
        if self.consume(TokenKind::RelativeP) {
            let location = self.location() as ParseLoc;
            return Ok((
                FetchDirection::Relative,
                self.parse_signed_fetch_count()?,
                FetchDirectionKeywords::Relative,
                location,
            ));
        }
        if self.consume(TokenKind::All) {
            return Ok((
                FetchDirection::Forward,
                i64::MAX,
                FetchDirectionKeywords::All,
                -1,
            ));
        }
        if self.consume(TokenKind::Forward) {
            if self.consume(TokenKind::All) {
                return Ok((
                    FetchDirection::Forward,
                    i64::MAX,
                    FetchDirectionKeywords::ForwardAll,
                    -1,
                ));
            }
            let (count, location) = if self.at(TokenKind::IConst)
                || matches!(
                    self.peek_kind(),
                    TokenKind::Char('+') | TokenKind::Char('-')
                ) {
                let location = self.location() as ParseLoc;
                (self.parse_signed_fetch_count()?, location)
            } else {
                (1, -1)
            };
            return Ok((
                FetchDirection::Forward,
                count,
                FetchDirectionKeywords::Forward,
                location,
            ));
        }
        if self.consume(TokenKind::Backward) {
            if self.consume(TokenKind::All) {
                return Ok((
                    FetchDirection::Backward,
                    i64::MAX,
                    FetchDirectionKeywords::BackwardAll,
                    -1,
                ));
            }
            let (count, location) = if self.at(TokenKind::IConst)
                || matches!(
                    self.peek_kind(),
                    TokenKind::Char('+') | TokenKind::Char('-')
                ) {
                let location = self.location() as ParseLoc;
                (self.parse_signed_fetch_count()?, location)
            } else {
                (1, -1)
            };
            return Ok((
                FetchDirection::Backward,
                count,
                FetchDirectionKeywords::Backward,
                location,
            ));
        }
        if self.at(TokenKind::IConst)
            || matches!(
                self.peek_kind(),
                TokenKind::Char('+') | TokenKind::Char('-')
            )
        {
            let location = self.location() as ParseLoc;
            return Ok((
                FetchDirection::Forward,
                self.parse_signed_fetch_count()?,
                FetchDirectionKeywords::None,
                location,
            ));
        }
        Ok((FetchDirection::Forward, 1, FetchDirectionKeywords::None, -1))
    }

    pub(super) fn parse_signed_fetch_count(&mut self) -> PResult<i64> {
        let sign = if self.consume(TokenKind::Char('-')) {
            -1i64
        } else {
            self.consume(TokenKind::Char('+'));
            1
        };
        let token = self.expect(TokenKind::IConst)?;
        match token.value {
            Some(TokenValue::Integer(value)) => Ok(sign * i64::from(value)),
            _ => Err(ParseError::ranged(token.range, "expected an integer count")),
        }
    }
}
