use super::*;

impl Parser {
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-createlanguage.html
    // CREATE [ OR REPLACE ] [ TRUSTED ] [ PROCEDURAL ] LANGUAGE name
    //     HANDLER call_handler [ INLINE inline_handler ] [ VALIDATOR valfunction ]
    // CREATE [ OR REPLACE ] [ TRUSTED ] [ PROCEDURAL ] LANGUAGE name
    pub(super) fn parse_create_language(
        &mut self,
        replace: bool,
        pltrusted: bool,
    ) -> PResult<Node> {
        self.expect(TokenKind::Language)?;
        self.record_completion_slot(completion::GrammarSlot::Language);
        let plname = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("CREATE LANGUAGE requires a name"))?,
        );
        if !self.consume(TokenKind::Handler) {
            return Ok(Node::CreateExtensionStmt(CreateExtensionStmt {
                node_tag: NodeTag::CreateExtensionStmt,
                extname: plname,
                if_not_exists: replace,
                options: Vec::new(),
            }));
        }
        self.record_completion_slot(completion::GrammarSlot::Function);
        let plhandler = self.parse_name_list();
        if plhandler.is_empty() {
            return Err(self.error_here("HANDLER requires a function name"));
        }
        let plinline = if self.consume(TokenKind::InlineP) {
            self.record_completion_slot(completion::GrammarSlot::Function);
            let name = self.parse_name_list();
            if name.is_empty() {
                return Err(self.error_here("INLINE requires a function name"));
            }
            name
        } else {
            Vec::new()
        };
        let plvalidator = if self.consume(TokenKind::Validator) {
            self.record_completion_slot(completion::GrammarSlot::Function);
            let name = self.parse_name_list();
            if name.is_empty() {
                return Err(self.error_here("VALIDATOR requires a function name"));
            }
            name
        } else if self.consume(TokenKind::No) {
            self.expect(TokenKind::Validator)?;
            Vec::new()
        } else {
            Vec::new()
        };
        Ok(Node::CreatePLangStmt(CreatePLangStmt {
            node_tag: NodeTag::CreatePLangStmt,
            replace,
            plname,
            plhandler,
            plinline,
            plvalidator,
            pltrusted,
        }))
    }
}
