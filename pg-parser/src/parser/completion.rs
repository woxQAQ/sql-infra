use super::*;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GrammarSlot {
    Relation,
    Table,
    View,
    MaterializedView,
    ForeignTable,
    Column,
    Attribute,
    Function,
    Procedure,
    Routine,
    Aggregate,
    Type,
    Domain,
    Schema,
    Sequence,
    Index,
    Constraint,
    Collation,
    Operator,
    OperatorClass,
    OperatorFamily,
    Role,
    Database,
    AccessMethod,
    Conversion,
    EventTrigger,
    Extension,
    ForeignDataWrapper,
    ForeignServer,
    Language,
    Policy,
    PropertyGraph,
    Publication,
    Rule,
    Statistics,
    Subscription,
    Tablespace,
    TextSearchConfiguration,
    TextSearchDictionary,
    TextSearchParser,
    TextSearchTemplate,
    Trigger,
    AnyName,
}

pub(super) const fn object_type_slot(object_type: ObjectType) -> GrammarSlot {
    match object_type {
        ObjectType::Table => GrammarSlot::Table,
        ObjectType::View => GrammarSlot::View,
        ObjectType::Matview => GrammarSlot::MaterializedView,
        ObjectType::ForeignTable => GrammarSlot::ForeignTable,
        ObjectType::Column => GrammarSlot::Column,
        ObjectType::Attribute => GrammarSlot::Attribute,
        ObjectType::Function => GrammarSlot::Function,
        ObjectType::Procedure => GrammarSlot::Procedure,
        ObjectType::Routine => GrammarSlot::Routine,
        ObjectType::Aggregate => GrammarSlot::Aggregate,
        ObjectType::Type => GrammarSlot::Type,
        ObjectType::Domain => GrammarSlot::Domain,
        ObjectType::Sequence => GrammarSlot::Sequence,
        ObjectType::Index => GrammarSlot::Index,
        ObjectType::Domconstraint | ObjectType::Tabconstraint => GrammarSlot::Constraint,
        ObjectType::Collation => GrammarSlot::Collation,
        ObjectType::Operator => GrammarSlot::Operator,
        ObjectType::Opclass => GrammarSlot::OperatorClass,
        ObjectType::Opfamily => GrammarSlot::OperatorFamily,
        ObjectType::Schema => GrammarSlot::Schema,
        ObjectType::Role => GrammarSlot::Role,
        ObjectType::Database => GrammarSlot::Database,
        ObjectType::AccessMethod => GrammarSlot::AccessMethod,
        ObjectType::Conversion => GrammarSlot::Conversion,
        ObjectType::EventTrigger => GrammarSlot::EventTrigger,
        ObjectType::Extension => GrammarSlot::Extension,
        ObjectType::Fdw => GrammarSlot::ForeignDataWrapper,
        ObjectType::ForeignServer => GrammarSlot::ForeignServer,
        ObjectType::Language => GrammarSlot::Language,
        ObjectType::Policy => GrammarSlot::Policy,
        ObjectType::Propgraph => GrammarSlot::PropertyGraph,
        ObjectType::Publication | ObjectType::PublicationNamespace | ObjectType::PublicationRel => {
            GrammarSlot::Publication
        }
        ObjectType::Rule => GrammarSlot::Rule,
        ObjectType::StatisticExt => GrammarSlot::Statistics,
        ObjectType::Subscription => GrammarSlot::Subscription,
        ObjectType::Tablespace => GrammarSlot::Tablespace,
        ObjectType::Tsconfiguration => GrammarSlot::TextSearchConfiguration,
        ObjectType::Tsdictionary => GrammarSlot::TextSearchDictionary,
        ObjectType::Tsparser => GrammarSlot::TextSearchParser,
        ObjectType::Tstemplate => GrammarSlot::TextSearchTemplate,
        ObjectType::Trigger => GrammarSlot::Trigger,
        ObjectType::Transform | ObjectType::UserMapping => GrammarSlot::AnyName,
        _ => GrammarSlot::AnyName,
    }
}

pub(super) fn definition_value_slot(object_type: ObjectType, name: &str) -> Option<GrammarSlot> {
    match (object_type, name) {
        (ObjectType::Operator, "function" | "procedure" | "restrict" | "join") => {
            Some(GrammarSlot::Function)
        }
        (ObjectType::Operator, "leftarg" | "rightarg") => Some(GrammarSlot::Type),
        (ObjectType::Operator, "commutator" | "negator") => Some(GrammarSlot::Operator),
        (
            ObjectType::Aggregate,
            "sfunc" | "finalfunc" | "combinefunc" | "serialfunc" | "deserialfunc" | "msfunc"
            | "minvfunc" | "mfinalfunc",
        ) => Some(GrammarSlot::Function),
        (ObjectType::Aggregate, "stype" | "mstype") => Some(GrammarSlot::Type),
        (ObjectType::Aggregate, "sortop") => Some(GrammarSlot::Operator),
        (
            ObjectType::Type,
            "input" | "output" | "receive" | "send" | "typmod_in" | "typmod_out" | "analyze"
            | "subscript",
        ) => Some(GrammarSlot::Function),
        (ObjectType::Type, "element") => Some(GrammarSlot::Type),
        (ObjectType::Type, "collation") => Some(GrammarSlot::Collation),
        (ObjectType::Tsconfiguration, "parser") => Some(GrammarSlot::TextSearchParser),
        (ObjectType::Tsconfiguration, "copy") => Some(GrammarSlot::TextSearchConfiguration),
        (ObjectType::Tsdictionary, "template") => Some(GrammarSlot::TextSearchTemplate),
        (ObjectType::Tsparser, "start" | "gettoken" | "end" | "lextypes" | "headline") => {
            Some(GrammarSlot::Function)
        }
        (ObjectType::Tstemplate, "init" | "lexize") => Some(GrammarSlot::Function),
        _ => None,
    }
}

/// The fixed phrase a clause-boundary token begins when it appears as an
/// expression follow token. These keywords open exactly one multi-word unit
/// in the grammar wherever an expression can end.
pub(super) const fn follow_phrase(kind: TokenKind) -> Option<&'static [TokenKind]> {
    match kind {
        TokenKind::GroupP => Some(&[TokenKind::GroupP, TokenKind::By]),
        TokenKind::Order => Some(&[TokenKind::Order, TokenKind::By]),
        TokenKind::Partition => Some(&[TokenKind::Partition, TokenKind::By]),
        TokenKind::Within => Some(&[TokenKind::Within, TokenKind::GroupP]),
        _ => None,
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ParserExpectations {
    pub tokens: Vec<TokenKind>,
    /// Fixed multi-token units that are grammatical at the point, e.g.
    /// `GROUP BY` or `IF NOT EXISTS`. Each phrase's head token also appears
    /// in `tokens`; a phrase does not claim the head has no other
    /// continuation.
    pub phrases: Vec<&'static [TokenKind]>,
    pub slots: Vec<GrammarSlot>,
}

#[derive(Debug, Default)]
pub(super) struct CompletionCollector {
    expectations: ParserExpectations,
}

pub(super) type SharedCollector = std::rc::Rc<std::cell::RefCell<CompletionCollector>>;

impl CompletionCollector {
    pub(super) fn tokens(&mut self, kinds: &[TokenKind]) {
        for kind in kinds {
            if matches!(
                kind,
                TokenKind::Eof | TokenKind::Completion | TokenKind::Char(';')
            ) || self.expectations.tokens.contains(kind)
            {
                continue;
            }
            self.expectations.tokens.push(*kind);
        }
    }

    pub(super) fn phrase(&mut self, phrase: &'static [TokenKind]) {
        debug_assert!(phrase.len() > 1, "a single-token phrase is just a token");
        self.tokens(&phrase[..1]);
        if !self.expectations.phrases.contains(&phrase) {
            self.expectations.phrases.push(phrase);
        }
    }

    pub(super) fn slot(&mut self, slot: GrammarSlot) {
        if !self.expectations.slots.contains(&slot) {
            self.expectations.slots.push(slot);
        }
    }
}

/// Collect grammar candidates at a UTF-8 byte offset.
///
/// A token intersecting the point is treated as the editor prefix and removed
/// from the parser input. Callers normally pass the replacement-range start.
pub fn collect_expectations(
    source: &str,
    point: TextSize,
) -> Result<ParserExpectations, crate::lexer::LexError> {
    let point_usize = usize::from(point).min(source.len());
    let point = TextSize::try_from(point_usize).expect("point was bounded by source length");
    let mut tokens = crate::lexer::lex_for_completion(source, point)?.tokens;

    if let Some(index) = tokens.iter().position(|token| {
        token.kind != TokenKind::Eof
            && token.range.start() <= point
            && (point < token.range.end()
                || (token.kind == TokenKind::Incomplete && point == token.range.end()))
    }) {
        tokens.remove(index);
    }
    let insertion = tokens
        .iter()
        .position(|token| token.range.start() >= point)
        .unwrap_or_else(|| tokens.len().saturating_sub(1));
    tokens.insert(
        insertion,
        Token::synthetic(TokenKind::Completion, point_usize),
    );

    let mut parser = Parser {
        tokens,
        pos: 0,
        completion: Some(std::rc::Rc::new(std::cell::RefCell::new(
            CompletionCollector::default(),
        ))),
    };
    let _outcome = parser.parse_with_ranges_controlled();
    let collector = parser
        .completion
        .as_ref()
        .expect("completion parser owns a collector")
        .borrow();
    Ok(collector.expectations.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_statement_starters() {
        let candidates = collect_expectations("", TextSize::ZERO).unwrap();
        let actual = candidates
            .tokens
            .into_iter()
            .collect::<std::collections::HashSet<_>>();
        let expected = STATEMENT_FAMILIES
            .iter()
            .flat_map(|family| family.starters())
            .copied()
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(actual, expected);
    }

    #[test]
    fn every_statement_family_collects_through_its_complete_sample() {
        for family in STATEMENT_FAMILIES {
            let source = family.coverage_sample();
            let tokens = crate::lex(source).unwrap_or_else(|error| {
                panic!("invalid completion coverage sample {source:?}: {error}")
            });
            let mut points = tokens
                .iter()
                .flat_map(|token| [token.range.start(), token.range.end()])
                .collect::<Vec<_>>();
            points.sort_unstable();
            points.dedup();
            for point in points {
                collect_expectations(source, point).unwrap_or_else(|error| {
                    panic!(
                        "completion failed for family sample {source:?} at byte {}: {error}",
                        usize::from(point)
                    )
                });
            }

            let complete = collect_expectations(
                source,
                TextSize::try_from(source.len()).expect("sample length fits TextSize"),
            )
            .unwrap();
            assert!(
                !complete.tokens.contains(&TokenKind::Char(';')),
                "complete family sample published the statement terminator: {source:?}: {complete:?}"
            );
            assert!(
                complete
                    .slots
                    .iter()
                    .all(|slot| *slot == GrammarSlot::Operator),
                "complete family sample published a stale object slot: {source:?}: {complete:?}"
            );
        }
    }

    #[test]
    fn collects_select_and_from_slots() {
        let candidates = collect_expectations("SELECT ", TextSize::new(7)).unwrap();
        assert!(candidates.slots.contains(&GrammarSlot::Column));
        assert!(candidates.slots.contains(&GrammarSlot::Function));
        assert!(candidates.tokens.contains(&TokenKind::From));

        let candidates = collect_expectations("SELECT * FROM ", TextSize::new(14)).unwrap();
        assert!(candidates.slots.contains(&GrammarSlot::Relation));
        assert!(candidates.slots.contains(&GrammarSlot::Function));
    }

    #[test]
    fn publishes_fixed_phrases_as_units() {
        let cases: &[(&str, &[&'static [TokenKind]])] = &[
            (
                "SELECT * FROM t ",
                &[
                    &[TokenKind::GroupP, TokenKind::By],
                    &[TokenKind::Order, TokenKind::By],
                ],
            ),
            ("DROP TABLE ", &[&[TokenKind::IfP, TokenKind::Exists]]),
            (
                "CREATE TABLE ",
                &[&[TokenKind::IfP, TokenKind::Not, TokenKind::Exists]],
            ),
            (
                "CREATE TABLE t (c int ",
                &[
                    &[TokenKind::Not, TokenKind::NullP],
                    &[TokenKind::Primary, TokenKind::Key],
                ],
            ),
            (
                "CREATE TABLE t (CONSTRAINT c ",
                &[
                    &[TokenKind::Primary, TokenKind::Key],
                    &[TokenKind::Foreign, TokenKind::Key],
                ],
            ),
            (
                "SELECT sum(x) OVER (",
                &[
                    &[TokenKind::Partition, TokenKind::By],
                    &[TokenKind::Order, TokenKind::By],
                ],
            ),
            ("SELECT array_agg(x ", &[&[TokenKind::Order, TokenKind::By]]),
            ("SELECT rank() ", &[&[TokenKind::Within, TokenKind::GroupP]]),
        ];
        for (sql, phrases) in cases {
            let candidates =
                collect_expectations(sql, TextSize::try_from(sql.len()).unwrap()).unwrap();
            for phrase in *phrases {
                assert!(
                    candidates.phrases.contains(phrase),
                    "{sql}: {:?}",
                    candidates.phrases
                );
                assert!(
                    candidates.tokens.contains(&phrase[0]),
                    "{sql}: phrase head missing from tokens: {:?}",
                    candidates.tokens
                );
            }
        }
    }

    #[test]
    fn complete_expression_fragments_publish_outer_follow_tokens() {
        let select = collect_expectations("SELECT 1", TextSize::new(8)).unwrap();
        assert!(select.tokens.contains(&TokenKind::Char(',')));
        assert!(select.tokens.contains(&TokenKind::From));
        assert!(select.tokens.contains(&TokenKind::And));
        assert!(select.tokens.contains(&TokenKind::TypeCast));
        assert!(select.slots.contains(&GrammarSlot::Operator));

        let sql = "SELECT * FROM t WHERE true";
        let where_clause =
            collect_expectations(sql, TextSize::try_from(sql.len()).unwrap()).unwrap();
        assert!(where_clause.tokens.contains(&TokenKind::GroupP));
        assert!(where_clause.tokens.contains(&TokenKind::Order));
    }

    #[test]
    fn completed_names_and_restricted_calls_do_not_publish_stale_slots() {
        let drop_table = collect_expectations(
            "DROP TABLE target ",
            TextSize::try_from("DROP TABLE target ".len()).unwrap(),
        )
        .unwrap();
        assert!(!drop_table.slots.contains(&GrammarSlot::Table));
        assert!(!drop_table.tokens.contains(&TokenKind::Char(';')));

        let alter = "ALTER TABLE t ADD COLUMN c int ";
        let alter = collect_expectations(alter, TextSize::try_from(alter.len()).unwrap()).unwrap();
        assert!(!alter.slots.contains(&GrammarSlot::Type));
        assert!(!alter.tokens.contains(&TokenKind::Char(';')));

        let setting = "SET work_mem = '4MB' ";
        let setting =
            collect_expectations(setting, TextSize::try_from(setting.len()).unwrap()).unwrap();
        assert!(!setting.slots.contains(&GrammarSlot::AnyName));
        assert!(!setting.tokens.contains(&TokenKind::Default));
        assert!(setting.tokens.contains(&TokenKind::Char(',')));
        assert!(!setting.tokens.contains(&TokenKind::Char(';')));

        let call = "CALL f() ";
        let call = collect_expectations(call, TextSize::try_from(call.len()).unwrap()).unwrap();
        assert!(call.tokens.is_empty());
        assert!(call.slots.is_empty());

        let signature = "DROP FUNCTION f(int) ";
        let signature =
            collect_expectations(signature, TextSize::try_from(signature.len()).unwrap()).unwrap();
        assert!(!signature.slots.contains(&GrammarSlot::Type));
        assert!(!signature.slots.contains(&GrammarSlot::Function));
        assert!(!signature.tokens.contains(&TokenKind::Char(';')));
    }

    #[test]
    fn collects_slot_inside_an_expression_fragment() {
        let sql = "SELECT u.na FROM users AS u";
        let point = TextSize::try_from(sql.find("na").unwrap()).unwrap();
        let candidates = collect_expectations(sql, point).unwrap();
        assert!(candidates.slots.contains(&GrammarSlot::Column));

        let sql = "CREATE INDEX i ON t ((lower(x)) COLLATE c)";
        let point = TextSize::try_from(sql.find("x").unwrap()).unwrap();
        let candidates = collect_expectations(sql, point).unwrap();
        assert!(candidates.slots.contains(&GrammarSlot::Column));
        assert!(candidates.slots.contains(&GrammarSlot::Function));
    }

    #[test]
    fn propagates_completion_into_deferred_expression_fragments() {
        for sql in [
            "SELECT * FROM JSON_TABLE(",
            "SELECT * FROM XMLTABLE(",
            "SELECT * FROM XMLTABLE(XMLNAMESPACES(DEFAULT ",
            "SELECT * FROM XMLTABLE('/x' PASSING ",
            "SELECT * FROM XMLTABLE('/x' PASSING doc COLUMNS c text DEFAULT ",
            "SELECT * FROM ROWS FROM (lower(",
            "SELECT * FROM generate_series(",
            "SELECT * FROM JSON_TABLE(doc, '$' PASSING ",
            "SELECT * FROM JSON_TABLE(doc, '$' COLUMNS (c int DEFAULT ",
            "SELECT JSON_ARRAYAGG(value ORDER BY ",
            "SELECT * FROM t OFFSET lower(",
            "SELECT * FROM t FETCH FIRST lower(",
            "SELECT sum(x) OVER (PARTITION BY ",
            "CREATE INDEX i ON t ((lower(",
            "CREATE TABLE t (c int) PARTITION BY RANGE ((lower(",
            "CREATE TABLE t (EXCLUDE USING gist ((lower(",
            "ALTER TABLE t ADD COLUMN c int DEFAULT ",
            "CREATE FUNCTION f(x int DEFAULT ",
            "CREATE STATISTICS s ON (lower(",
            "INSERT INTO t VALUES (1) ON CONFLICT ((lower(",
            "UPDATE t SET a[lower(",
        ] {
            let candidates =
                collect_expectations(sql, TextSize::try_from(sql.len()).unwrap()).unwrap();
            assert!(
                candidates.slots.contains(&GrammarSlot::Column),
                "{sql}: {:?}",
                candidates.slots
            );
            assert!(
                candidates.slots.contains(&GrammarSlot::Function),
                "{sql}: {:?}",
                candidates.slots
            );
        }
    }

    #[test]
    fn propagates_completion_into_copy_option_fragments() {
        let option = "COPY source_table TO STDOUT WITH (";
        let option =
            collect_expectations(option, TextSize::try_from(option.len()).unwrap()).unwrap();
        assert!(option.tokens.contains(&TokenKind::Format));
        assert!(option.slots.contains(&GrammarSlot::AnyName));

        let columns = "COPY source_table TO STDOUT WITH (force_quote (";
        let columns =
            collect_expectations(columns, TextSize::try_from(columns.len()).unwrap()).unwrap();
        assert!(columns.slots.contains(&GrammarSlot::Column));
        assert!(!columns.slots.contains(&GrammarSlot::AnyName));
    }

    #[test]
    fn xmltable_column_fragment_shares_the_expression_collector() {
        let mut tokens = crate::lex("c text DEFAULT ").unwrap();
        let eof = tokens.pop().unwrap();
        tokens.push(Token::synthetic(TokenKind::Completion, eof.location()));
        let collector = std::rc::Rc::new(std::cell::RefCell::new(CompletionCollector::default()));
        let _ = xmltable_column_from_tokens_with_completion(tokens, Some(collector.clone()));
        let slots = &collector.borrow().expectations.slots;
        assert!(slots.contains(&GrammarSlot::Column), "{slots:?}");
        assert!(slots.contains(&GrammarSlot::Function), "{slots:?}");
    }

    #[test]
    fn collects_json_array_query_suffixes_after_the_nested_query() {
        let format_sql = "SELECT JSON_ARRAY(SELECT 1 FORMAT ";
        let format =
            collect_expectations(format_sql, TextSize::try_from(format_sql.len()).unwrap())
                .unwrap();
        assert!(format.tokens.contains(&TokenKind::Json));

        let returning_sql = "SELECT JSON_ARRAY(SELECT 1 RETURNING ";
        let returning = collect_expectations(
            returning_sql,
            TextSize::try_from(returning_sql.len()).unwrap(),
        )
        .unwrap();
        assert!(returning.slots.contains(&GrammarSlot::Type));
    }

    #[test]
    fn recovers_an_unterminated_token_at_the_point() {
        let sql = "SELECT \"na";
        let candidates = collect_expectations(sql, TextSize::new(7)).unwrap();
        assert!(candidates.slots.contains(&GrammarSlot::Column));
    }

    #[test]
    fn collects_dml_and_ddl_slots() {
        let candidates = collect_expectations("UPDATE accounts SET ", TextSize::new(20)).unwrap();
        assert!(candidates.slots.contains(&GrammarSlot::Column));

        let sql = "CREATE TABLE t (c )";
        let point = TextSize::try_from(sql.find(')').unwrap()).unwrap();
        let candidates = collect_expectations(sql, point).unwrap();
        assert!(candidates.slots.contains(&GrammarSlot::Type));
    }

    #[test]
    fn collects_create_alter_and_drop_families() {
        let create = collect_expectations("CREATE ", TextSize::new(7)).unwrap();
        assert!(create.tokens.contains(&TokenKind::Table));
        assert!(create.tokens.contains(&TokenKind::Function));

        let alter = collect_expectations("ALTER ", TextSize::new(6)).unwrap();
        assert!(alter.tokens.contains(&TokenKind::Table));
        assert!(alter.tokens.contains(&TokenKind::Role));

        let drop = collect_expectations("DROP ", TextSize::new(5)).unwrap();
        assert!(drop.tokens.contains(&TokenKind::Table));
        assert!(drop.tokens.contains(&TokenKind::Function));
    }

    #[test]
    fn classifies_common_object_name_positions() {
        let cases = [
            ("ALTER TABLE ", GrammarSlot::Table),
            ("ALTER TABLE t DROP COLUMN ", GrammarSlot::Column),
            ("DROP FUNCTION ", GrammarSlot::Function),
            ("COMMENT ON COLUMN t.", GrammarSlot::Column),
            ("GRANT SELECT ON TABLE t TO ", GrammarSlot::Role),
        ];
        for (sql, slot) in cases {
            let point = TextSize::try_from(sql.len()).unwrap();
            let candidates = collect_expectations(sql, point).unwrap();
            assert!(
                candidates.slots.contains(&slot),
                "{sql}: {:?}",
                candidates.slots
            );
        }

        for sql in ["DROP FUNCTION f(", "ALTER FUNCTION f(", "DROP OPERATOR +("] {
            let candidates =
                collect_expectations(sql, TextSize::try_from(sql.len()).unwrap()).unwrap();
            assert!(
                candidates.slots.contains(&GrammarSlot::Type),
                "{sql}: {:?}",
                candidates.slots
            );
            assert!(
                !candidates.slots.contains(&GrammarSlot::Function)
                    && !candidates.slots.contains(&GrammarSlot::Operator),
                "{sql}: {:?}",
                candidates.slots
            );
        }
    }

    #[test]
    fn publishes_catalog_slots_for_ddl_object_names() {
        let cases = [
            ("CREATE TABLE ", GrammarSlot::Table),
            ("CREATE INDEX ", GrammarSlot::Index),
            ("CREATE SCHEMA ", GrammarSlot::Schema),
            ("CREATE DATABASE ", GrammarSlot::Database),
            ("CREATE SEQUENCE ", GrammarSlot::Sequence),
            ("CREATE TYPE ", GrammarSlot::Type),
            ("CREATE COLLATION ", GrammarSlot::Collation),
            ("CREATE OPERATOR ", GrammarSlot::Operator),
            ("CREATE OPERATOR CLASS ", GrammarSlot::OperatorClass),
            ("CREATE ROLE ", GrammarSlot::Role),
            ("ALTER INDEX ", GrammarSlot::Index),
            ("ALTER SEQUENCE ", GrammarSlot::Sequence),
            ("ALTER DATABASE ", GrammarSlot::Database),
            ("ALTER SCHEMA ", GrammarSlot::Schema),
            ("ALTER COLLATION ", GrammarSlot::Collation),
            ("ALTER ROLE ", GrammarSlot::Role),
            ("DROP VIEW ", GrammarSlot::View),
            ("DROP INDEX ", GrammarSlot::Index),
            ("DROP SCHEMA ", GrammarSlot::Schema),
            ("DROP SEQUENCE ", GrammarSlot::Sequence),
            ("DROP TYPE ", GrammarSlot::Type),
            ("DROP COLLATION ", GrammarSlot::Collation),
            ("DROP OPERATOR ", GrammarSlot::Operator),
            ("DROP ROLE ", GrammarSlot::Role),
        ];
        for (sql, slot) in cases {
            let candidates =
                collect_expectations(sql, TextSize::try_from(sql.len()).unwrap()).unwrap();
            assert!(
                candidates.slots.contains(&slot),
                "{sql}: {:?}",
                candidates.slots
            );
        }

        let index_target = collect_expectations(
            "CREATE INDEX i ON ",
            TextSize::try_from("CREATE INDEX i ON ".len()).unwrap(),
        )
        .unwrap();
        assert!(index_target.slots.contains(&GrammarSlot::MaterializedView));
    }

    #[test]
    fn publishes_catalog_slots_inside_ddl_and_expression_clauses() {
        let cases = [
            ("SELECT 1::", GrammarSlot::Type),
            ("SELECT 1 COLLATE ", GrammarSlot::Collation),
            ("CREATE INDEX i ON ", GrammarSlot::Table),
            ("CREATE INDEX i ON t USING ", GrammarSlot::AccessMethod),
            (
                "CREATE TABLE t (c int) TABLESPACE ",
                GrammarSlot::Tablespace,
            ),
            (
                "CREATE FOREIGN TABLE t (c int) SERVER ",
                GrammarSlot::ForeignServer,
            ),
            ("ALTER DATABASE db SET TABLESPACE ", GrammarSlot::Tablespace),
            ("DO LANGUAGE ", GrammarSlot::Language),
            ("CREATE INDEX i ON t (c COLLATE ", GrammarSlot::Collation),
            ("CREATE TABLE t (c int REFERENCES ", GrammarSlot::Table),
            ("CREATE TABLE t (c int CONSTRAINT ", GrammarSlot::Constraint),
            ("ALTER TABLE t ALTER COLUMN c TYPE ", GrammarSlot::Type),
            (
                "ALTER TABLE t ALTER COLUMN c TYPE text COLLATE ",
                GrammarSlot::Collation,
            ),
            ("COMMENT ON TYPE ", GrammarSlot::Type),
            ("COMMENT ON OPERATOR CLASS ", GrammarSlot::OperatorClass),
            ("CREATE POLICY p ON ", GrammarSlot::Table),
            ("DROP POLICY p ON ", GrammarSlot::Table),
            ("GRANT role_a TO ", GrammarSlot::Role),
        ];
        for (sql, slot) in cases {
            let candidates =
                collect_expectations(sql, TextSize::try_from(sql.len()).unwrap()).unwrap();
            assert!(
                candidates.slots.contains(&slot),
                "{sql}: {:?}",
                candidates.slots
            );
        }
    }

    #[test]
    fn publishes_catalog_slots_across_utility_object_positions() {
        let cases = [
            ("CREATE ACCESS METHOD ", GrammarSlot::AccessMethod),
            (
                "CREATE ACCESS METHOD am TYPE TABLE HANDLER ",
                GrammarSlot::Function,
            ),
            ("CREATE EXTENSION ", GrammarSlot::Extension),
            ("CREATE EXTENSION ext WITH SCHEMA ", GrammarSlot::Schema),
            ("CREATE SERVER ", GrammarSlot::ForeignServer),
            ("CREATE USER MAPPING FOR ", GrammarSlot::Role),
            (
                "CREATE USER MAPPING FOR role SERVER ",
                GrammarSlot::ForeignServer,
            ),
            ("CREATE LANGUAGE lang HANDLER ", GrammarSlot::Function),
            ("CREATE POLICY p ON t TO ", GrammarSlot::Role),
            ("CREATE PUBLICATION p FOR TABLE ", GrammarSlot::Table),
            (
                "CREATE PUBLICATION p FOR TABLES IN SCHEMA ",
                GrammarSlot::Schema,
            ),
            ("CREATE STATISTICS s ON c FROM ", GrammarSlot::Table),
            ("CREATE TABLE t (LIKE ", GrammarSlot::Table),
            ("CREATE TRIGGER trg BEFORE INSERT ON ", GrammarSlot::Table),
            ("CREATE RULE r AS ON SELECT TO ", GrammarSlot::Table),
            ("CREATE CAST (", GrammarSlot::Type),
            ("DROP CAST (", GrammarSlot::Type),
            ("CREATE CONVERSION ", GrammarSlot::Conversion),
            (
                "CREATE CONVERSION c FOR 'UTF8' TO 'LATIN1' FROM ",
                GrammarSlot::Function,
            ),
            ("CREATE TRANSFORM FOR int LANGUAGE ", GrammarSlot::Language),
            (
                "CREATE TRANSFORM FOR int LANGUAGE sql (FROM SQL WITH FUNCTION ",
                GrammarSlot::Function,
            ),
            ("CREATE POLICY ", GrammarSlot::Policy),
            ("ALTER POLICY ", GrammarSlot::Policy),
            ("CREATE PROPERTY GRAPH ", GrammarSlot::PropertyGraph),
            ("ALTER PROPERTY GRAPH ", GrammarSlot::PropertyGraph),
            (
                "CREATE PROPERTY GRAPH g VERTEX TABLES (",
                GrammarSlot::Table,
            ),
            (
                "CREATE SUBSCRIPTION s CONNECTION 'host=x' PUBLICATION ",
                GrammarSlot::Publication,
            ),
            (
                "ALTER SUBSCRIPTION s SET PUBLICATION ",
                GrammarSlot::Publication,
            ),
            ("CREATE EVENT TRIGGER ", GrammarSlot::EventTrigger),
            (
                "CREATE EVENT TRIGGER trg ON ddl_command_start EXECUTE FUNCTION ",
                GrammarSlot::Function,
            ),
            ("CREATE TRIGGER ", GrammarSlot::Trigger),
            ("CREATE TRIGGER trg BEFORE UPDATE OF ", GrammarSlot::Column),
            (
                "CREATE TRIGGER trg BEFORE INSERT ON t EXECUTE FUNCTION ",
                GrammarSlot::Function,
            ),
            ("CREATE RULE ", GrammarSlot::Rule),
            (
                "CREATE TEXT SEARCH PARSER p (START = ",
                GrammarSlot::Function,
            ),
            (
                "CREATE TEXT SEARCH CONFIGURATION c (PARSER = ",
                GrammarSlot::TextSearchParser,
            ),
            (
                "CREATE TEXT SEARCH DICTIONARY d (TEMPLATE = ",
                GrammarSlot::TextSearchTemplate,
            ),
            ("DECLARE ", GrammarSlot::AnyName),
            ("CLOSE ", GrammarSlot::AnyName),
            ("FETCH FROM ", GrammarSlot::AnyName),
            ("MOVE IN ", GrammarSlot::AnyName),
            ("PREPARE ", GrammarSlot::AnyName),
            ("EXECUTE ", GrammarSlot::AnyName),
            ("DEALLOCATE ", GrammarSlot::AnyName),
            ("SET ROLE ", GrammarSlot::Role),
            ("SET SESSION AUTHORIZATION ", GrammarSlot::Role),
            ("SAVEPOINT ", GrammarSlot::AnyName),
            ("RELEASE SAVEPOINT ", GrammarSlot::AnyName),
            ("ROLLBACK TO SAVEPOINT ", GrammarSlot::AnyName),
            ("LISTEN ", GrammarSlot::AnyName),
            ("UNLISTEN ", GrammarSlot::AnyName),
            ("NOTIFY ", GrammarSlot::AnyName),
            ("CREATE OPERATOR @@ (PROCEDURE = ", GrammarSlot::Function),
            (
                "ALTER OPERATOR @@ (int, int) SET (RESTRICT = ",
                GrammarSlot::Function,
            ),
            (
                "ALTER OPERATOR @@ (int, int) SET (COMMUTATOR = ",
                GrammarSlot::Operator,
            ),
            ("GRANT USAGE ON SCHEMA ", GrammarSlot::Schema),
            ("REINDEX INDEX ", GrammarSlot::Index),
            ("REINDEX SCHEMA ", GrammarSlot::Schema),
            ("REINDEX DATABASE ", GrammarSlot::Database),
            ("VACUUM t (", GrammarSlot::Column),
            ("CREATE FUNCTION ", GrammarSlot::Function),
            ("CREATE FUNCTION f(arg ", GrammarSlot::Type),
            ("CREATE FUNCTION f() RETURNS ", GrammarSlot::Type),
            (
                "CREATE FUNCTION f() RETURNS int LANGUAGE ",
                GrammarSlot::Language,
            ),
            (
                "CREATE FUNCTION f() RETURNS int SUPPORT ",
                GrammarSlot::Function,
            ),
            ("CREATE TABLESPACE ", GrammarSlot::Tablespace),
            ("CREATE TABLESPACE ts OWNER ", GrammarSlot::Role),
            ("CREATE STATISTICS ", GrammarSlot::Statistics),
            ("ALTER STATISTICS ", GrammarSlot::Statistics),
            (
                "CREATE TEXT SEARCH DICTIONARY ",
                GrammarSlot::TextSearchDictionary,
            ),
            ("CREATE SEQUENCE s AS ", GrammarSlot::Type),
            ("CREATE SEQUENCE s OWNED BY ", GrammarSlot::Column),
        ];
        for (sql, slot) in cases {
            let candidates =
                collect_expectations(sql, TextSize::try_from(sql.len()).unwrap()).unwrap();
            assert!(
                candidates.slots.contains(&slot),
                "{sql}: {:?}",
                candidates.slots
            );
        }
    }

    #[test]
    fn completion_marker_uses_typed_parser_control() {
        let parser = Parser {
            tokens: vec![
                Token::synthetic(TokenKind::Completion, 0),
                Token::synthetic(TokenKind::Eof, 0),
            ],
            pos: 0,
            completion: Some(std::rc::Rc::new(std::cell::RefCell::new(
                CompletionCollector::default(),
            ))),
        };
        assert!(matches!(
            parser.error_here("not a syntax error"),
            ParserExit::Completion(_)
        ));
    }

    #[test]
    fn every_dispatched_statement_family_has_completion_boundary_coverage() {
        for family in STATEMENT_FAMILIES {
            let sql = family.coverage_sample();
            let tokens = lex(sql).unwrap_or_else(|error| {
                panic!("failed to lex {:?} sample {sql:?}: {error}", family)
            });
            let first = tokens[0].kind;
            let second = tokens.get(1).map_or(TokenKind::Eof, |token| token.kind);
            assert_eq!(
                classify_statement(first, second),
                Some(*family),
                "coverage sample does not dispatch to its registered family: {sql:?}"
            );
            parse_one(sql).unwrap_or_else(|error| {
                panic!("registered completion sample does not parse: {sql:?}: {error}")
            });

            let mut points = tokens
                .iter()
                .flat_map(|token| [token.range.start(), token.range.end()])
                .collect::<Vec<_>>();
            points.sort_unstable();
            points.dedup();
            for point in points {
                collect_expectations(sql, point).unwrap_or_else(|error| {
                    panic!(
                        "completion collection failed for {:?} at byte {}: {error}",
                        family,
                        usize::from(point)
                    )
                });
            }
        }
    }
}
