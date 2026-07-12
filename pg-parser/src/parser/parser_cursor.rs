//! 解析器游标原语（parser cursor primitives）。
//!
//! 这里实现的是手写递归下降解析器的最底层机械装置：围绕 `Parser { tokens, pos }`
//! 提供的一组「向前看 / 匹配 / 消费」原语。所有上层产生式（`parse_statement`、
//! `parse_create`、`parse_select` …）都通过这些原语读写游标，本身不直接操作
//! `pos`，从而把语法分派和游标推进这两件事彻底解耦。
//!
//! 设计上属于 LL(1) 风格的预测递归下降：通常只看 `peek_kind()`（LA(1)）即可决定
//! 走哪条产生式分支；个别歧义场景通过 `peek_kind_n` / `has_top_level_token_before`
//! 做少量额外向前看，绝不回溯。

use super::*;

impl Parser {
    /// 从当前位置开始，收集 token 直至遇到 `stops` 中的某个**顶层** token，把途中
    /// 消费的 token 全部克隆返回。
    ///
    /// 「顶层」由括号配平计数 `depth` 判定：只有 `depth == 0` 时的停止 token 才真正
    /// 停下，嵌套在 `()` / `[]` 内的同名 token 会被原样吞掉。这样可以在不知道子
    /// 产生式结构的前提下，把一段 SQL 片段整体「舀」出来交给下游（如 fragment
    /// parser、延迟解析的函数体等）处理。
    ///
    /// # 关键字二词组合的特例
    ///
    /// 有几个停止关键字本身也可能作为合法的子句内 token 出现，必须配合前一个 token
    /// 才能区分。这里通过 `out.last()` 做配对识别，避免误把子句内的 token 当成
    /// 边界停止：
    /// - `GROUP` 紧跟在 `WITHIN` 之后 → `WITHIN GROUP`，非边界
    /// - `FOR`   紧跟在 `COLLATION` 之后 → `COLLATION FOR`，非边界
    /// - `FROM`  紧跟在 `DISTINCT` 之后 → `DISTINCT FROM`，非边界
    /// - `NOT`   紧跟在 `IS` 之后 → `IS NOT` 谓词，非边界
    pub(super) fn take_until_top_level(&mut self, stops: &[TokenKind]) -> Vec<Token> {
        let mut out = Vec::new();
        // 括号嵌套深度，仅当 depth == 0 时才认为处于「顶层」。
        let mut depth = 0usize;
        while !self.at(TokenKind::Eof) {
            let kind = self.peek_kind();
            // 识别需要看前一个 token 才能消歧的二词组合（见方法文档）。
            let within_group = kind == TokenKind::GroupP
                && out.last().map(|token: &Token| token.kind) == Some(TokenKind::Within);
            let collation_for = kind == TokenKind::For
                && out.last().map(|token: &Token| token.kind) == Some(TokenKind::Collation);
            let distinct_from = kind == TokenKind::From
                && out.last().map(|token: &Token| token.kind) == Some(TokenKind::Distinct);
            let is_not_predicate = kind == TokenKind::Not
                && out.last().map(|token: &Token| token.kind) == Some(TokenKind::Is);
            // 顶层且命中停止词、又不是上述特例组合时，停下并不消费该 token。
            if depth == 0
                && stops.contains(&kind)
                && !within_group
                && !collation_for
                && !distinct_from
                && !is_not_predicate
            {
                break;
            }
            // 用括号配平维护 depth。注意闭括号若在 depth==0 且本身是停止词，要在
            // 减深度之前提前 break，否则会把它错误吞进 out。
            match kind {
                TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                TokenKind::Char(')') | TokenKind::Char(']') => {
                    if depth == 0 && stops.contains(&kind) {
                        break;
                    }
                    depth = depth.saturating_sub(1);
                }
                _ => {}
            }
            out.push(self.advance().clone());
        }
        out
    }

    /// 当前是否处于语句结束位置（`;` 或 EOF）。
    pub(super) fn at_statement_end(&self) -> bool {
        self.at(TokenKind::Char(';')) || self.at(TokenKind::Eof)
    }

    /// 断言当前处于语句结束位置，否则报「语句后出现多余 token」错误。不消费 token。
    pub(super) fn expect_statement_end(&self) -> PResult<()> {
        if self.at_statement_end() {
            Ok(())
        } else {
            Err(self.error_here(format!(
                "unexpected token {:?} after statement",
                self.peek_kind()
            )))
        }
    }

    /// LA(1) 谓词：当前 token 是否等于 `kind`。不改变游标。
    pub(super) fn at(&self, kind: TokenKind) -> bool {
        self.peek_kind() == kind
    }

    /// LA(1) 谓词：当前 token 是否属于 `kinds` 中的任意一种。不改变游标。
    pub(super) fn at_any(&self, kinds: &[TokenKind]) -> bool {
        kinds.contains(&self.peek_kind())
    }

    /// 不动游标地向前探测：在到达 `stops` 中任一**顶层** token 之前，能否先遇到
    /// `needle`。
    ///
    /// 与 [`take_until_top_level`](Self::take_until_top_level) 一样用括号配平维护
    /// 「顶层」概念，但只读不消费，用于在分派产生式时做更激进的向前看判断
    /// （例如先确认后续顶层存在某个关键字再决定如何解析）。遇到任一停止词先于
    /// needle 时返回 `false`；扫描到 EOF 也返回 `false`。
    pub(super) fn has_top_level_token_before(
        &self,
        needle: TokenKind,
        stops: &[TokenKind],
    ) -> bool {
        let mut depth = 0usize;
        for token in &self.tokens[self.pos..] {
            match token.kind {
                TokenKind::Char('(') | TokenKind::Char('[') => depth += 1,
                TokenKind::Char(')') | TokenKind::Char(']') => {
                    depth = depth.saturating_sub(1);
                }
                kind if depth == 0 && kind == needle => return true,
                kind if depth == 0 && stops.contains(&kind) => return false,
                _ => {}
            }
        }
        false
    }

    /// 可选匹配：若当前 token 是 `kind` 则吃掉并返回 `true`，否则游标不动返回
    /// `false`。对应产生式中的「可选 / 看到一个就消费」语义。
    pub(super) fn consume(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// 必须匹配：当前 token 是 `kind` 则吃掉并返回该 token，否则构造带期望/实际
    /// token 对比的语法错误。对应产生式中的「强制出现」语义。
    pub(super) fn expect(&mut self, kind: TokenKind) -> PResult<Token> {
        if self.at(kind) {
            Ok(self.advance().clone())
        } else {
            Err(self.error_here(format!("expected {:?}, found {:?}", kind, self.peek_kind())))
        }
    }

    /// 无条件吃掉当前 token 并返回对其的引用。EOF 时不越界，停留在末尾，返回最后
    /// 一个 token。这是所有「消费」动作的最底层实现，`consume` / `expect` 都依赖它。
    pub(super) fn advance(&mut self) -> &Token {
        if !self.at(TokenKind::Eof) {
            self.pos += 1;
        }
        &self.tokens[self.pos.saturating_sub(1)]
    }

    /// 返回当前 token 的引用（LA(1)），不消费。
    pub(super) fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    /// 返回当前 token 的 `TokenKind`，是最常用的向前看入口。
    pub(super) fn peek_kind(&self) -> TokenKind {
        self.peek().kind
    }

    /// 返回从当前起第 `n` 个 token 的 `TokenKind`（LA(n+1)）。越界时返回 `Eof`，
    /// 供少数需要多看几步才能消歧的产生式使用。
    pub(super) fn peek_kind_n(&self, n: usize) -> TokenKind {
        self.tokens
            .get(self.pos + n)
            .map(|token| token.kind)
            .unwrap_or(TokenKind::Eof)
    }

    /// 当前 token 在源码中的字节偏移，常用作 AST 节点的 `location`。
    pub(super) fn location(&self) -> usize {
        self.peek().location
    }

    /// 上一个被消费 token 的字节偏移。用于在已经 `advance` 之后仍想给刚解析出的
    /// 节点标注起始位置的场景；游标在起始位置时回退为当前 [`location`](Self::location)。
    pub(super) fn previous_location(&self) -> usize {
        self.tokens
            .get(self.pos.saturating_sub(1))
            .map(|token| token.location)
            .unwrap_or(self.location())
    }

    /// 以当前 token 位置为锚点构造一个 `ParseError`，是解析器所有报错的统一入口。
    pub(super) fn error_here(&self, message: impl Into<std::string::String>) -> ParseError {
        ParseError::new(self.location(), message)
    }
}
