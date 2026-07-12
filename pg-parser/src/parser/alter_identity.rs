use super::*;

#[derive(Default)]
struct AlterIdentity {
    object_type: ObjectType,
    relation: Option<Box<RangeVar>>,
    object: Option<Box<Node>>,
    subname: Option<std::string::String>,
    missing_ok: bool,
    location: usize,
}

impl Parser {
    fn parse_alter_object_kind(&mut self) -> PResult<ObjectType> {
        let kind = match self.peek_kind() {
            TokenKind::Access if self.peek_kind_n(1) == TokenKind::Method => {
                self.advance();
                self.advance();
                ObjectType::AccessMethod
            }
            TokenKind::Aggregate => ObjectType::Aggregate,
            TokenKind::Collation => ObjectType::Collation,
            TokenKind::ConversionP => ObjectType::Conversion,
            TokenKind::Database => ObjectType::Database,
            TokenKind::DomainP => ObjectType::Domain,
            TokenKind::Event if self.peek_kind_n(1) == TokenKind::Trigger => {
                self.advance();
                self.advance();
                ObjectType::EventTrigger
            }
            TokenKind::Extension => ObjectType::Extension,
            TokenKind::Foreign if self.peek_kind_n(1) == TokenKind::DataP => {
                self.advance();
                self.expect(TokenKind::DataP)?;
                self.expect(TokenKind::Wrapper)?;
                ObjectType::Fdw
            }
            TokenKind::Foreign if self.peek_kind_n(1) == TokenKind::Table => {
                self.advance();
                self.advance();
                ObjectType::ForeignTable
            }
            TokenKind::Function => ObjectType::Function,
            TokenKind::GroupP | TokenKind::Role | TokenKind::User => ObjectType::Role,
            TokenKind::Index => ObjectType::Index,
            TokenKind::Language => {
                self.advance();
                ObjectType::Language
            }
            TokenKind::LargeP if self.peek_kind_n(1) == TokenKind::ObjectP => {
                self.advance();
                self.advance();
                ObjectType::Largeobject
            }
            TokenKind::Materialized if self.peek_kind_n(1) == TokenKind::View => {
                self.advance();
                self.advance();
                ObjectType::Matview
            }
            TokenKind::Operator if self.peek_kind_n(1) == TokenKind::Class => {
                self.advance();
                self.advance();
                ObjectType::Opclass
            }
            TokenKind::Operator if self.peek_kind_n(1) == TokenKind::Family => {
                self.advance();
                self.advance();
                ObjectType::Opfamily
            }
            TokenKind::Operator => ObjectType::Operator,
            TokenKind::Policy => ObjectType::Policy,
            TokenKind::Procedure => ObjectType::Procedure,
            TokenKind::Property if self.peek_kind_n(1) == TokenKind::Graph => {
                self.advance();
                self.advance();
                ObjectType::Propgraph
            }
            TokenKind::Publication => ObjectType::Publication,
            TokenKind::Routine => ObjectType::Routine,
            TokenKind::Rule => ObjectType::Rule,
            TokenKind::Schema => ObjectType::Schema,
            TokenKind::Sequence => ObjectType::Sequence,
            TokenKind::Server => ObjectType::ForeignServer,
            TokenKind::Statistics => ObjectType::StatisticExt,
            TokenKind::Subscription => ObjectType::Subscription,
            TokenKind::Table => ObjectType::Table,
            TokenKind::Tablespace => ObjectType::Tablespace,
            TokenKind::TextP if self.peek_kind_n(1) == TokenKind::Search => {
                self.advance();
                self.advance();
                match self.advance().kind {
                    TokenKind::Parser => ObjectType::Tsparser,
                    TokenKind::Dictionary => ObjectType::Tsdictionary,
                    TokenKind::Template => ObjectType::Tstemplate,
                    TokenKind::Configuration => ObjectType::Tsconfiguration,
                    _ => return Err(self.error_here("invalid TEXT SEARCH object type")),
                }
            }
            TokenKind::Trigger => ObjectType::Trigger,
            TokenKind::TypeP => ObjectType::Type,
            TokenKind::View => ObjectType::View,
            TokenKind::Procedural => {
                self.advance();
                self.expect(TokenKind::Language)?;
                ObjectType::Language
            }
            other => {
                return Err(self.error_here(format!("unsupported ALTER object type {other:?}")));
            }
        };
        if !matches!(
            kind,
            ObjectType::AccessMethod
                | ObjectType::EventTrigger
                | ObjectType::Fdw
                | ObjectType::ForeignTable
                | ObjectType::Largeobject
                | ObjectType::Matview
                | ObjectType::Opclass
                | ObjectType::Opfamily
                | ObjectType::Propgraph
                | ObjectType::Tsparser
                | ObjectType::Tsdictionary
                | ObjectType::Tstemplate
                | ObjectType::Tsconfiguration
                | ObjectType::Language
        ) {
            self.advance();
        }
        Ok(kind)
    }

    fn parse_alter_identity(&mut self, action_stops: &[TokenKind]) -> PResult<AlterIdentity> {
        let object_type = self.parse_alter_object_kind()?;
        let missing_ok = self.consume_if_exists()?;
        let mut identity = AlterIdentity {
            object_type,
            missing_ok,
            location: self.location(),
            ..AlterIdentity::default()
        };
        if relation_object_type(object_type) {
            let relation = if matches!(object_type, ObjectType::Table | ObjectType::ForeignTable) {
                self.parse_relation_expr(false)?
            } else {
                self.try_parse_qualified_range_var()
                    .ok_or_else(|| self.error_here("ALTER object requires a relation name"))?
            };
            identity.relation = Some(Box::new(relation));
            return Ok(identity);
        }
        if matches!(
            object_type,
            ObjectType::Policy | ObjectType::Rule | ObjectType::Trigger
        ) {
            let name = self
                .consume_col_id()
                .ok_or_else(|| self.error_here("ALTER object requires a name"))?;
            self.expect(TokenKind::On)?;
            identity.relation = Some(Box::new(
                self.try_parse_qualified_range_var()
                    .ok_or_else(|| self.error_here("ON requires a relation name"))?,
            ));
            identity.subname = Some(name.clone());
            identity.object = Some(Box::new(name_list_node(vec![make_string_node(name)])));
            return Ok(identity);
        }
        if matches!(
            object_type,
            ObjectType::Aggregate
                | ObjectType::Function
                | ObjectType::Procedure
                | ObjectType::Routine
                | ObjectType::Operator
        ) {
            let object = if object_type == ObjectType::Operator {
                self.parse_operator_with_args_until(action_stops)?
            } else if object_type == ObjectType::Aggregate {
                self.parse_aggregate_with_args_until(action_stops)?
            } else {
                self.parse_object_with_args_until(action_stops)?
            };
            identity.object = Some(Box::new(Node::ObjectWithArgs(object)));
            return Ok(identity);
        }
        if matches!(object_type, ObjectType::Opclass | ObjectType::Opfamily) {
            let mut names = self.parse_name_list_until_keywords(&[TokenKind::Using]);
            if names.is_empty() {
                return Err(self.error_here("operator class or family requires a name"));
            }
            self.expect(TokenKind::Using)?;
            let amname = self
                .consume_col_id()
                .ok_or_else(|| self.error_here("USING requires an access method"))?;
            names.insert(0, make_string_node(amname));
            identity.object = Some(Box::new(name_list_node(names)));
            return Ok(identity);
        }
        if object_type == ObjectType::Largeobject {
            identity.object = Some(Box::new(self.parse_numeric_only()?));
            return Ok(identity);
        }
        if object_type == ObjectType::Role {
            let name = self
                .consume_role_id()?
                .ok_or_else(|| self.error_here("ALTER ROLE requires a role name"))?;
            identity.object = Some(Box::new(make_string_node(name.clone())));
            identity.subname = Some(name);
            return Ok(identity);
        }
        if matches!(
            object_type,
            ObjectType::Collation
                | ObjectType::Conversion
                | ObjectType::Domain
                | ObjectType::StatisticExt
                | ObjectType::Tsparser
                | ObjectType::Tsdictionary
                | ObjectType::Tstemplate
                | ObjectType::Tsconfiguration
                | ObjectType::Type
        ) {
            let names = self.parse_name_list_until_keywords(action_stops);
            if names.is_empty() {
                return Err(self.error_here("ALTER object requires a qualified name"));
            }
            identity.object = Some(Box::new(name_list_node(names)));
            return Ok(identity);
        }
        let name = self
            .consume_col_id()
            .ok_or_else(|| self.error_here("ALTER object requires a name"))?;
        identity.object = Some(Box::new(make_string_node(name.clone())));
        if matches!(
            object_type,
            ObjectType::Database | ObjectType::Role | ObjectType::Schema | ObjectType::Tablespace
        ) {
            identity.subname = Some(name);
        }
        Ok(identity)
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alteraggregate.html
    // ALTER AGGREGATE name ( aggregate_signature ) RENAME TO new_name
    // ALTER AGGREGATE name ( aggregate_signature )
    //                 OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER AGGREGATE name ( aggregate_signature ) SET SCHEMA new_schema
    //
    // where aggregate_signature is:
    //
    // * |
    // [ argmode ] [ argname ] argtype [ , ... ] |
    // [ [ argmode ] [ argname ] argtype [ , ... ] ] ORDER BY [ argmode ] [ argname ] argtype [ , ... ]
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altercollation.html
    // ALTER COLLATION name REFRESH VERSION
    //
    // ALTER COLLATION name RENAME TO new_name
    // ALTER COLLATION name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER COLLATION name SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterconversion.html
    // ALTER CONVERSION name RENAME TO new_name
    // ALTER CONVERSION name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER CONVERSION name SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterdatabase.html
    // ALTER DATABASE name [ [ WITH ] option [ ... ] ]
    //
    // where option can be:
    //
    //     ALLOW_CONNECTIONS allowconn
    //     CONNECTION LIMIT connlimit
    //     IS_TEMPLATE istemplate
    //
    // ALTER DATABASE name RENAME TO new_name
    //
    // ALTER DATABASE name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //
    // ALTER DATABASE name SET TABLESPACE new_tablespace
    //
    // ALTER DATABASE name REFRESH COLLATION VERSION
    //
    // ALTER DATABASE name SET configuration_parameter { TO | = } { value | DEFAULT }
    // ALTER DATABASE name SET configuration_parameter FROM CURRENT
    // ALTER DATABASE name RESET configuration_parameter
    // ALTER DATABASE name RESET ALL
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterdomain.html
    // ALTER DOMAIN name
    //     { SET DEFAULT expression | DROP DEFAULT }
    // ALTER DOMAIN name
    //     { SET | DROP } NOT NULL
    // ALTER DOMAIN name
    //     ADD domain_constraint [ NOT VALID ]
    // ALTER DOMAIN name
    //     DROP CONSTRAINT [ IF EXISTS ] constraint_name [ RESTRICT | CASCADE ]
    // ALTER DOMAIN name
    //      RENAME CONSTRAINT constraint_name TO new_constraint_name
    // ALTER DOMAIN name
    //     VALIDATE CONSTRAINT constraint_name
    // ALTER DOMAIN name
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER DOMAIN name
    //     RENAME TO new_name
    // ALTER DOMAIN name
    //     SET SCHEMA new_schema
    //
    // where domain_constraint is:
    //
    // [ CONSTRAINT constraint_name ]
    // { NOT NULL | CHECK (expression) }
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterforeigndatawrapper.html
    // ALTER FOREIGN DATA WRAPPER name
    //     [ HANDLER handler_function | NO HANDLER ]
    //     [ VALIDATOR validator_function | NO VALIDATOR ]
    //     [ OPTIONS ( [ ADD | SET | DROP ] option ['value'] [, ... ]) ]
    // ALTER FOREIGN DATA WRAPPER name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER FOREIGN DATA WRAPPER name RENAME TO new_name
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterfunction.html
    // ALTER FUNCTION name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     action [ ... ] [ RESTRICT ]
    // ALTER FUNCTION name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     RENAME TO new_name
    // ALTER FUNCTION name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER FUNCTION name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     SET SCHEMA new_schema
    // ALTER FUNCTION name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     [ NO ] DEPENDS ON EXTENSION extension_name
    //
    // where action is one of:
    //
    //     CALLED ON NULL INPUT | RETURNS NULL ON NULL INPUT | STRICT
    //     IMMUTABLE | STABLE | VOLATILE
    //     [ NOT ] LEAKPROOF
    //     [ EXTERNAL ] SECURITY INVOKER | [ EXTERNAL ] SECURITY DEFINER
    //     PARALLEL { UNSAFE | RESTRICTED | SAFE }
    //     COST execution_cost
    //     ROWS result_rows
    //     SUPPORT support_function
    //     SET configuration_parameter { TO | = } { value | DEFAULT }
    //     SET configuration_parameter FROM CURRENT
    //     RESET configuration_parameter
    //     RESET ALL
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altergroup.html
    // ALTER GROUP role_specification ADD USER user_name [, ... ]
    // ALTER GROUP role_specification DROP USER user_name [, ... ]
    //
    // where role_specification can be:
    //
    //     role_name
    //   | CURRENT_ROLE
    //   | CURRENT_USER
    //   | SESSION_USER
    //
    // ALTER GROUP group_name RENAME TO new_name
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterlanguage.html
    // ALTER [ PROCEDURAL ] LANGUAGE name RENAME TO new_name
    // ALTER [ PROCEDURAL ] LANGUAGE name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alteropclass.html
    // ALTER OPERATOR CLASS name USING index_method
    //     RENAME TO new_name
    //
    // ALTER OPERATOR CLASS name USING index_method
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //
    // ALTER OPERATOR CLASS name USING index_method
    //     SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alteropfamily.html
    // ALTER OPERATOR FAMILY name USING index_method ADD
    //   {  OPERATOR strategy_number operator_name ( op_type, op_type )
    //               [ FOR SEARCH | FOR ORDER BY sort_family_name ]
    //    | FUNCTION support_number [ ( op_type [ , op_type ] ) ]
    //               function_name [ ( argument_type [, ...] ) ]
    //   } [, ... ]
    //
    // ALTER OPERATOR FAMILY name USING index_method DROP
    //   {  OPERATOR strategy_number ( op_type [ , op_type ] )
    //    | FUNCTION support_number ( op_type [ , op_type ] )
    //   } [, ... ]
    //
    // ALTER OPERATOR FAMILY name USING index_method
    //     RENAME TO new_name
    //
    // ALTER OPERATOR FAMILY name USING index_method
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //
    // ALTER OPERATOR FAMILY name USING index_method
    //     SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterpolicy.html
    // ALTER POLICY name ON table_name RENAME TO new_name
    //
    // ALTER POLICY name ON table_name
    //     [ TO { role_name | PUBLIC | CURRENT_ROLE | CURRENT_USER | SESSION_USER } [, ...] ]
    //     [ USING ( using_expression ) ]
    //     [ WITH CHECK ( check_expression ) ]
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterprocedure.html
    // ALTER PROCEDURE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     action [ ... ] [ RESTRICT ]
    // ALTER PROCEDURE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     RENAME TO new_name
    // ALTER PROCEDURE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER PROCEDURE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     SET SCHEMA new_schema
    // ALTER PROCEDURE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     [ NO ] DEPENDS ON EXTENSION extension_name
    //
    // where action is one of:
    //
    //     [ EXTERNAL ] SECURITY INVOKER | [ EXTERNAL ] SECURITY DEFINER
    //     SET configuration_parameter { TO | = } { value | DEFAULT }
    //     SET configuration_parameter FROM CURRENT
    //     RESET configuration_parameter
    //     RESET ALL
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterpublication.html
    // ALTER PUBLICATION name ADD publication_object [, ...]
    // ALTER PUBLICATION name SET publication_object [, ...]
    // ALTER PUBLICATION name DROP publication_object [, ...]
    // ALTER PUBLICATION name SET ( publication_parameter [= value] [, ... ] )
    // ALTER PUBLICATION name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER PUBLICATION name RENAME TO new_name
    //
    // where publication_object is one of:
    //
    //     TABLE [ ONLY ] table_name [ * ] [ ( column_name [, ... ] ) ] [ WHERE ( expression ) ] [, ... ]
    //     TABLES IN SCHEMA { schema_name | CURRENT_SCHEMA } [, ... ]
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterrole.html
    // ALTER ROLE role_specification [ WITH ] option [ ... ]
    //
    // where option can be:
    //
    //       SUPERUSER | NOSUPERUSER
    //     | CREATEDB | NOCREATEDB
    //     | CREATEROLE | NOCREATEROLE
    //     | INHERIT | NOINHERIT
    //     | LOGIN | NOLOGIN
    //     | REPLICATION | NOREPLICATION
    //     | BYPASSRLS | NOBYPASSRLS
    //     | CONNECTION LIMIT connlimit
    //     | [ ENCRYPTED ] PASSWORD 'password' | PASSWORD NULL
    //     | VALID UNTIL 'timestamp'
    //
    // ALTER ROLE name RENAME TO new_name
    //
    // ALTER ROLE { role_specification | ALL } [ IN DATABASE database_name ] SET configuration_parameter { TO | = } { value | DEFAULT }
    // ALTER ROLE { role_specification | ALL } [ IN DATABASE database_name ] SET configuration_parameter FROM CURRENT
    // ALTER ROLE { role_specification | ALL } [ IN DATABASE database_name ] RESET configuration_parameter
    // ALTER ROLE { role_specification | ALL } [ IN DATABASE database_name ] RESET ALL
    //
    // where role_specification can be:
    //
    //     role_name
    //   | CURRENT_ROLE
    //   | CURRENT_USER
    //   | SESSION_USER
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterroutine.html
    // ALTER ROUTINE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     action [ ... ] [ RESTRICT ]
    // ALTER ROUTINE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     RENAME TO new_name
    // ALTER ROUTINE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER ROUTINE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     SET SCHEMA new_schema
    // ALTER ROUTINE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     [ NO ] DEPENDS ON EXTENSION extension_name
    //
    // where action is one of:
    //
    //     IMMUTABLE | STABLE | VOLATILE
    //     [ NOT ] LEAKPROOF
    //     [ EXTERNAL ] SECURITY INVOKER | [ EXTERNAL ] SECURITY DEFINER
    //     PARALLEL { UNSAFE | RESTRICTED | SAFE }
    //     COST execution_cost
    //     ROWS result_rows
    //     SET configuration_parameter { TO | = } { value | DEFAULT }
    //     SET configuration_parameter FROM CURRENT
    //     RESET configuration_parameter
    //     RESET ALL
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterschema.html
    // ALTER SCHEMA name RENAME TO new_name
    // ALTER SCHEMA name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterserver.html
    // ALTER SERVER name [ VERSION 'new_version' ]
    //     [ OPTIONS ( [ ADD | SET | DROP ] option ['value'] [, ... ] ) ]
    // ALTER SERVER name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER SERVER name RENAME TO new_name
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altersubscription.html
    // ALTER SUBSCRIPTION name CONNECTION 'conninfo'
    // ALTER SUBSCRIPTION name SET PUBLICATION publication_name [, ...] [ WITH ( publication_option [= value] [, ... ] ) ]
    // ALTER SUBSCRIPTION name ADD PUBLICATION publication_name [, ...] [ WITH ( publication_option [= value] [, ... ] ) ]
    // ALTER SUBSCRIPTION name DROP PUBLICATION publication_name [, ...] [ WITH ( publication_option [= value] [, ... ] ) ]
    // ALTER SUBSCRIPTION name REFRESH PUBLICATION [ WITH ( refresh_option [= value] [, ... ] ) ]
    // ALTER SUBSCRIPTION name ENABLE
    // ALTER SUBSCRIPTION name DISABLE
    // ALTER SUBSCRIPTION name SET ( subscription_parameter [= value] [, ... ] )
    // ALTER SUBSCRIPTION name SKIP ( skip_option = value )
    // ALTER SUBSCRIPTION name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER SUBSCRIPTION name RENAME TO new_name
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altertable.html
    // ALTER TABLE [ IF EXISTS ] [ ONLY ] name [ * ]
    //     action [, ... ]
    // ALTER TABLE [ IF EXISTS ] [ ONLY ] name [ * ]
    //     RENAME [ COLUMN ] column_name TO new_column_name
    // ALTER TABLE [ IF EXISTS ] [ ONLY ] name [ * ]
    //     RENAME CONSTRAINT constraint_name TO new_constraint_name
    // ALTER TABLE [ IF EXISTS ] name
    //     RENAME TO new_name
    // ALTER TABLE [ IF EXISTS ] name
    //     SET SCHEMA new_schema
    // ALTER TABLE ALL IN TABLESPACE name [ OWNED BY role_name [, ... ] ]
    //     SET TABLESPACE new_tablespace [ NOWAIT ]
    // ALTER TABLE [ IF EXISTS ] name
    //     ATTACH PARTITION partition_name { FOR VALUES partition_bound_spec | DEFAULT }
    // ALTER TABLE [ IF EXISTS ] name
    //     DETACH PARTITION partition_name [ CONCURRENTLY | FINALIZE ]
    //
    // where action is one of:
    //
    //     ADD [ COLUMN ] [ IF NOT EXISTS ] column_name data_type [ COLLATE collation ] [ column_constraint [ ... ] ]
    //     DROP [ COLUMN ] [ IF EXISTS ] column_name [ RESTRICT | CASCADE ]
    //     ALTER [ COLUMN ] column_name [ SET DATA ] TYPE data_type [ COLLATE collation ] [ USING expression ]
    //     ALTER [ COLUMN ] column_name SET DEFAULT expression
    //     ALTER [ COLUMN ] column_name DROP DEFAULT
    //     ALTER [ COLUMN ] column_name { SET | DROP } NOT NULL
    //     ALTER [ COLUMN ] column_name SET EXPRESSION AS ( expression )
    //     ALTER [ COLUMN ] column_name DROP EXPRESSION [ IF EXISTS ]
    //     ALTER [ COLUMN ] column_name ADD GENERATED { ALWAYS | BY DEFAULT } AS IDENTITY [ ( sequence_options ) ]
    //     ALTER [ COLUMN ] column_name { SET GENERATED { ALWAYS | BY DEFAULT } | SET sequence_option | RESTART [ [ WITH ] restart ] } [...]
    //     ALTER [ COLUMN ] column_name DROP IDENTITY [ IF EXISTS ]
    //     ALTER [ COLUMN ] column_name SET STATISTICS { integer | DEFAULT }
    //     ALTER [ COLUMN ] column_name SET ( attribute_option = value [, ... ] )
    //     ALTER [ COLUMN ] column_name RESET ( attribute_option [, ... ] )
    //     ALTER [ COLUMN ] column_name SET STORAGE { PLAIN | EXTERNAL | EXTENDED | MAIN | DEFAULT }
    //     ALTER [ COLUMN ] column_name SET COMPRESSION compression_method
    //     ADD table_constraint [ NOT VALID ]
    //     ADD table_constraint_using_index
    //     ALTER CONSTRAINT constraint_name [ DEFERRABLE | NOT DEFERRABLE ] [ INITIALLY DEFERRED | INITIALLY IMMEDIATE ] [ ENFORCED | NOT ENFORCED ]
    //     ALTER CONSTRAINT constraint_name [ INHERIT | NO INHERIT ]
    //     VALIDATE CONSTRAINT constraint_name
    //     DROP CONSTRAINT [ IF EXISTS ]  constraint_name [ RESTRICT | CASCADE ]
    //     DISABLE TRIGGER [ trigger_name | ALL | USER ]
    //     ENABLE TRIGGER [ trigger_name | ALL | USER ]
    //     ENABLE REPLICA TRIGGER trigger_name
    //     ENABLE ALWAYS TRIGGER trigger_name
    //     DISABLE RULE rewrite_rule_name
    //     ENABLE RULE rewrite_rule_name
    //     ENABLE REPLICA RULE rewrite_rule_name
    //     ENABLE ALWAYS RULE rewrite_rule_name
    //     DISABLE ROW LEVEL SECURITY
    //     ENABLE ROW LEVEL SECURITY
    //     FORCE ROW LEVEL SECURITY
    //     NO FORCE ROW LEVEL SECURITY
    //     CLUSTER ON index_name
    //     SET WITHOUT CLUSTER
    //     SET WITHOUT OIDS
    //     SET ACCESS METHOD { new_access_method | DEFAULT }
    //     SET TABLESPACE new_tablespace
    //     SET { LOGGED | UNLOGGED }
    //     SET ( storage_parameter [= value] [, ... ] )
    //     RESET ( storage_parameter [, ... ] )
    //     INHERIT parent_table
    //     NO INHERIT parent_table
    //     OF type_name
    //     NOT OF
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //     REPLICA IDENTITY { DEFAULT | USING INDEX index_name | FULL | NOTHING }
    //
    // and partition_bound_spec is:
    //
    // IN ( partition_bound_expr [, ...] ) |
    // FROM ( { partition_bound_expr | MINVALUE | MAXVALUE } [, ...] )
    //   TO ( { partition_bound_expr | MINVALUE | MAXVALUE } [, ...] ) |
    // WITH ( MODULUS numeric_literal, REMAINDER numeric_literal )
    //
    // and column_constraint is:
    //
    // [ CONSTRAINT constraint_name ]
    // { NOT NULL [ NO INHERIT ] |
    //   NULL |
    //   CHECK ( expression ) [ NO INHERIT ] |
    //   DEFAULT default_expr |
    //   GENERATED ALWAYS AS ( generation_expr ) [ STORED | VIRTUAL ] |
    //   GENERATED { ALWAYS | BY DEFAULT } AS IDENTITY [ ( sequence_options ) ] |
    //   UNIQUE [ NULLS [ NOT ] DISTINCT ] index_parameters |
    //   PRIMARY KEY index_parameters |
    //   REFERENCES reftable [ ( refcolumn ) ] [ MATCH FULL | MATCH PARTIAL | MATCH SIMPLE ]
    //     [ ON DELETE referential_action ] [ ON UPDATE referential_action ] }
    // [ DEFERRABLE | NOT DEFERRABLE ] [ INITIALLY DEFERRED | INITIALLY IMMEDIATE ] [ ENFORCED | NOT ENFORCED ]
    //
    // and table_constraint is:
    //
    // [ CONSTRAINT constraint_name ]
    // { CHECK ( expression ) [ NO INHERIT ] |
    //   NOT NULL column_name [ NO INHERIT ] |
    //   UNIQUE [ NULLS [ NOT ] DISTINCT ] ( column_name [, ... ] [, column_name WITHOUT OVERLAPS ] ) index_parameters |
    //   PRIMARY KEY ( column_name [, ... ] [, column_name WITHOUT OVERLAPS ] ) index_parameters |
    //   EXCLUDE [ USING index_method ] ( exclude_element WITH operator [, ... ] ) index_parameters [ WHERE ( predicate ) ] |
    //   FOREIGN KEY ( column_name [, ... ] [, PERIOD column_name ] ) REFERENCES reftable [ ( refcolumn [, ... ]  [, PERIOD refcolumn ] ) ]
    //     [ MATCH FULL | MATCH PARTIAL | MATCH SIMPLE ] [ ON DELETE referential_action ] [ ON UPDATE referential_action ] }
    // [ DEFERRABLE | NOT DEFERRABLE ] [ INITIALLY DEFERRED | INITIALLY IMMEDIATE ] [ ENFORCED | NOT ENFORCED ]
    //
    // and table_constraint_using_index is:
    //
    //     [ CONSTRAINT constraint_name ]
    //     { UNIQUE | PRIMARY KEY } USING INDEX index_name
    //     [ DEFERRABLE | NOT DEFERRABLE ] [ INITIALLY DEFERRED | INITIALLY IMMEDIATE ]
    //
    // index_parameters in UNIQUE, PRIMARY KEY, and EXCLUDE constraints are:
    //
    // [ INCLUDE ( column_name [, ... ] ) ]
    // [ WITH ( storage_parameter [= value] [, ... ] ) ]
    // [ USING INDEX TABLESPACE tablespace_name ]
    //
    // exclude_element in an EXCLUDE constraint is:
    //
    // { column_name | ( expression ) } [ COLLATE collation ] [ opclass [ ( opclass_parameter = value [, ... ] ) ] ] [ ASC | DESC ] [ NULLS { FIRST | LAST } ]
    //
    // referential_action in a FOREIGN KEY/REFERENCES constraint is:
    //
    // { NO ACTION | RESTRICT | CASCADE | SET NULL [ ( column_name [, ... ] ) ] | SET DEFAULT [ ( column_name [, ... ] ) ] }
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altersequence.html
    // ALTER SEQUENCE [ IF EXISTS ] name
    //     [ AS data_type ]
    //     [ INCREMENT [ BY ] increment ]
    //     [ MINVALUE minvalue | NO MINVALUE ] [ MAXVALUE maxvalue | NO MAXVALUE ]
    //     [ [ NO ] CYCLE ]
    //     [ START [ WITH ] start ]
    //     [ RESTART [ [ WITH ] restart ] ]
    //     [ CACHE cache ]
    //     [ OWNED BY { table_name.column_name | NONE } ]
    // ALTER SEQUENCE [ IF EXISTS ] name SET { LOGGED | UNLOGGED }
    // ALTER SEQUENCE [ IF EXISTS ] name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER SEQUENCE [ IF EXISTS ] name RENAME TO new_name
    // ALTER SEQUENCE [ IF EXISTS ] name SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterview.html
    // ALTER VIEW [ IF EXISTS ] name ALTER [ COLUMN ] column_name SET DEFAULT expression
    // ALTER VIEW [ IF EXISTS ] name ALTER [ COLUMN ] column_name DROP DEFAULT
    // ALTER VIEW [ IF EXISTS ] name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER VIEW [ IF EXISTS ] name RENAME [ COLUMN ] column_name TO new_column_name
    // ALTER VIEW [ IF EXISTS ] name RENAME TO new_name
    // ALTER VIEW [ IF EXISTS ] name SET SCHEMA new_schema
    // ALTER VIEW [ IF EXISTS ] name SET ( view_option_name [= view_option_value] [, ... ] )
    // ALTER VIEW [ IF EXISTS ] name RESET ( view_option_name [, ... ] )
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altermaterializedview.html
    // ALTER MATERIALIZED VIEW [ IF EXISTS ] name
    //     action [, ... ]
    // ALTER MATERIALIZED VIEW name
    //     [ NO ] DEPENDS ON EXTENSION extension_name
    // ALTER MATERIALIZED VIEW [ IF EXISTS ] name
    //     RENAME [ COLUMN ] column_name TO new_column_name
    // ALTER MATERIALIZED VIEW [ IF EXISTS ] name
    //     RENAME TO new_name
    // ALTER MATERIALIZED VIEW [ IF EXISTS ] name
    //     SET SCHEMA new_schema
    // ALTER MATERIALIZED VIEW ALL IN TABLESPACE name [ OWNED BY role_name [, ... ] ]
    //     SET TABLESPACE new_tablespace [ NOWAIT ]
    //
    // where action is one of:
    //
    //     ALTER [ COLUMN ] column_name SET STATISTICS integer
    //     ALTER [ COLUMN ] column_name SET ( attribute_option = value [, ... ] )
    //     ALTER [ COLUMN ] column_name RESET ( attribute_option [, ... ] )
    //     ALTER [ COLUMN ] column_name SET STORAGE { PLAIN | EXTERNAL | EXTENDED | MAIN | DEFAULT }
    //     ALTER [ COLUMN ] column_name SET COMPRESSION compression_method
    //     CLUSTER ON index_name
    //     SET WITHOUT CLUSTER
    //     SET ACCESS METHOD new_access_method
    //     SET TABLESPACE new_tablespace
    //     SET ( storage_parameter [= value] [, ... ] )
    //     RESET ( storage_parameter [, ... ] )
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterindex.html
    // ALTER INDEX [ IF EXISTS ] name RENAME TO new_name
    // ALTER INDEX [ IF EXISTS ] name SET TABLESPACE tablespace_name
    // ALTER INDEX name ATTACH PARTITION index_name
    // ALTER INDEX name [ NO ] DEPENDS ON EXTENSION extension_name
    // ALTER INDEX [ IF EXISTS ] name SET ( storage_parameter [= value] [, ... ] )
    // ALTER INDEX [ IF EXISTS ] name RESET ( storage_parameter [, ... ] )
    // ALTER INDEX [ IF EXISTS ] name ALTER [ COLUMN ] column_number
    //     SET STATISTICS integer
    // ALTER INDEX ALL IN TABLESPACE name [ OWNED BY role_name [, ... ] ]
    //     SET TABLESPACE new_tablespace [ NOWAIT ]
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterforeigntable.html
    // ALTER FOREIGN TABLE [ IF EXISTS ] [ ONLY ] name [ * ]
    //     action [, ... ]
    // ALTER FOREIGN TABLE [ IF EXISTS ] [ ONLY ] name [ * ]
    //     RENAME [ COLUMN ] column_name TO new_column_name
    // ALTER FOREIGN TABLE [ IF EXISTS ] name
    //     RENAME TO new_name
    // ALTER FOREIGN TABLE [ IF EXISTS ] name
    //     SET SCHEMA new_schema
    //
    // where action is one of:
    //
    //     ADD [ COLUMN ] column_name data_type [ COLLATE collation ] [ column_constraint [ ... ] ]
    //     DROP [ COLUMN ] [ IF EXISTS ] column_name [ RESTRICT | CASCADE ]
    //     ALTER [ COLUMN ] column_name [ SET DATA ] TYPE data_type [ COLLATE collation ]
    //     ALTER [ COLUMN ] column_name SET DEFAULT expression
    //     ALTER [ COLUMN ] column_name DROP DEFAULT
    //     ALTER [ COLUMN ] column_name { SET | DROP } NOT NULL
    //     ALTER [ COLUMN ] column_name SET STATISTICS integer
    //     ALTER [ COLUMN ] column_name SET ( attribute_option = value [, ... ] )
    //     ALTER [ COLUMN ] column_name RESET ( attribute_option [, ... ] )
    //     ALTER [ COLUMN ] column_name SET STORAGE { PLAIN | EXTERNAL | EXTENDED | MAIN | DEFAULT }
    //     ALTER [ COLUMN ] column_name OPTIONS ( [ ADD | SET | DROP ] option ['value'] [, ... ])
    //     ADD table_constraint [ NOT VALID ]
    //     VALIDATE CONSTRAINT constraint_name
    //     DROP CONSTRAINT [ IF EXISTS ]  constraint_name [ RESTRICT | CASCADE ]
    //     DISABLE TRIGGER [ trigger_name | ALL | USER ]
    //     ENABLE TRIGGER [ trigger_name | ALL | USER ]
    //     ENABLE REPLICA TRIGGER trigger_name
    //     ENABLE ALWAYS TRIGGER trigger_name
    //     SET WITHOUT OIDS
    //     INHERIT parent_table
    //     NO INHERIT parent_table
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //     OPTIONS ( [ ADD | SET | DROP ] option ['value'] [, ... ])
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterrule.html
    // ALTER RULE name ON table_name RENAME TO new_name
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altertrigger.html
    // ALTER TRIGGER name ON table_name RENAME TO new_name
    // ALTER TRIGGER name ON table_name [ NO ] DEPENDS ON EXTENSION extension_name
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altereventtrigger.html
    // ALTER EVENT TRIGGER name DISABLE
    // ALTER EVENT TRIGGER name ENABLE [ REPLICA | ALWAYS ]
    // ALTER EVENT TRIGGER name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER EVENT TRIGGER name RENAME TO new_name
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altertablespace.html
    // ALTER TABLESPACE name RENAME TO new_name
    // ALTER TABLESPACE name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER TABLESPACE name SET ( tablespace_option = value [, ... ] )
    // ALTER TABLESPACE name RESET ( tablespace_option [, ... ] )
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterstatistics.html
    // ALTER STATISTICS name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER STATISTICS name RENAME TO new_name
    // ALTER STATISTICS name SET SCHEMA new_schema
    // ALTER STATISTICS name SET STATISTICS { new_target | DEFAULT }
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altertsparser.html
    // ALTER TEXT SEARCH PARSER name RENAME TO new_name
    // ALTER TEXT SEARCH PARSER name SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altertsdictionary.html
    // ALTER TEXT SEARCH DICTIONARY name (
    //     option [ = value ] [, ... ]
    // )
    // ALTER TEXT SEARCH DICTIONARY name RENAME TO new_name
    // ALTER TEXT SEARCH DICTIONARY name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER TEXT SEARCH DICTIONARY name SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altertstemplate.html
    // ALTER TEXT SEARCH TEMPLATE name RENAME TO new_name
    // ALTER TEXT SEARCH TEMPLATE name SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altertsconfig.html
    // ALTER TEXT SEARCH CONFIGURATION name
    //     ADD MAPPING FOR token_type [, ... ] WITH dictionary_name [, ... ]
    // ALTER TEXT SEARCH CONFIGURATION name
    //     ALTER MAPPING FOR token_type [, ... ] WITH dictionary_name [, ... ]
    // ALTER TEXT SEARCH CONFIGURATION name
    //     ALTER MAPPING REPLACE old_dictionary WITH new_dictionary
    // ALTER TEXT SEARCH CONFIGURATION name
    //     ALTER MAPPING FOR token_type [, ... ] REPLACE old_dictionary WITH new_dictionary
    // ALTER TEXT SEARCH CONFIGURATION name
    //     DROP MAPPING [ IF EXISTS ] FOR token_type [, ... ]
    // ALTER TEXT SEARCH CONFIGURATION name RENAME TO new_name
    // ALTER TEXT SEARCH CONFIGURATION name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER TEXT SEARCH CONFIGURATION name SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altertype.html
    // ALTER TYPE name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER TYPE name RENAME TO new_name
    // ALTER TYPE name SET SCHEMA new_schema
    // ALTER TYPE name RENAME ATTRIBUTE attribute_name TO new_attribute_name [ CASCADE | RESTRICT ]
    // ALTER TYPE name action [, ... ]
    // ALTER TYPE name ADD VALUE [ IF NOT EXISTS ] new_enum_value [ { BEFORE | AFTER } neighbor_enum_value ]
    // ALTER TYPE name RENAME VALUE existing_enum_value TO new_enum_value
    // ALTER TYPE name SET ( property = value [, ... ] )
    //
    // where action is one of:
    //
    //     ADD ATTRIBUTE attribute_name data_type [ COLLATE collation ] [ CASCADE | RESTRICT ]
    //     DROP ATTRIBUTE [ IF EXISTS ] attribute_name [ CASCADE | RESTRICT ]
    //     ALTER ATTRIBUTE attribute_name [ SET DATA ] TYPE data_type [ COLLATE collation ] [ CASCADE | RESTRICT ]
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alteruser.html
    // ALTER USER role_specification [ WITH ] option [ ... ]
    //
    // where option can be:
    //
    //       SUPERUSER | NOSUPERUSER
    //     | CREATEDB | NOCREATEDB
    //     | CREATEROLE | NOCREATEROLE
    //     | INHERIT | NOINHERIT
    //     | LOGIN | NOLOGIN
    //     | REPLICATION | NOREPLICATION
    //     | BYPASSRLS | NOBYPASSRLS
    //     | CONNECTION LIMIT connlimit
    //     | [ ENCRYPTED ] PASSWORD 'password' | PASSWORD NULL
    //     | VALID UNTIL 'timestamp'
    //
    // ALTER USER name RENAME TO new_name
    //
    // ALTER USER { role_specification | ALL } [ IN DATABASE database_name ] SET configuration_parameter { TO | = } { value | DEFAULT }
    // ALTER USER { role_specification | ALL } [ IN DATABASE database_name ] SET configuration_parameter FROM CURRENT
    // ALTER USER { role_specification | ALL } [ IN DATABASE database_name ] RESET configuration_parameter
    // ALTER USER { role_specification | ALL } [ IN DATABASE database_name ] RESET ALL
    //
    // where role_specification can be:
    //
    //     role_name
    //   | CURRENT_ROLE
    //   | CURRENT_USER
    //   | SESSION_USER
    pub(super) fn parse_rename(&mut self) -> PResult<Node> {
        let mut identity = self.parse_alter_identity(&[TokenKind::Rename])?;
        if !matches!(
            identity.object_type,
            ObjectType::Aggregate
                | ObjectType::Collation
                | ObjectType::Conversion
                | ObjectType::Database
                | ObjectType::Domain
                | ObjectType::Fdw
                | ObjectType::Function
                | ObjectType::Role
                | ObjectType::Language
                | ObjectType::Opclass
                | ObjectType::Opfamily
                | ObjectType::Policy
                | ObjectType::Procedure
                | ObjectType::Propgraph
                | ObjectType::Publication
                | ObjectType::Routine
                | ObjectType::Schema
                | ObjectType::ForeignServer
                | ObjectType::Subscription
                | ObjectType::Table
                | ObjectType::Sequence
                | ObjectType::View
                | ObjectType::Matview
                | ObjectType::Index
                | ObjectType::ForeignTable
                | ObjectType::Rule
                | ObjectType::Trigger
                | ObjectType::EventTrigger
                | ObjectType::Tablespace
                | ObjectType::StatisticExt
                | ObjectType::Tsparser
                | ObjectType::Tsdictionary
                | ObjectType::Tstemplate
                | ObjectType::Tsconfiguration
                | ObjectType::Type
        ) {
            return Err(self.error_here("this object type does not support RENAME"));
        }
        if identity.missing_ok
            && !matches!(
                identity.object_type,
                ObjectType::Table
                    | ObjectType::Sequence
                    | ObjectType::View
                    | ObjectType::Matview
                    | ObjectType::Index
                    | ObjectType::ForeignTable
                    | ObjectType::Policy
            )
        {
            return Err(self.error_here("IF EXISTS is not supported for this RENAME object"));
        }
        self.expect(TokenKind::Rename)?;
        let mut relation_type = ObjectType::default();
        let mut rename_type = identity.object_type;
        let mut behavior = DropBehavior::Restrict;
        if self.consume(TokenKind::Column) {
            if !matches!(
                identity.object_type,
                ObjectType::Table
                    | ObjectType::View
                    | ObjectType::Matview
                    | ObjectType::ForeignTable
            ) {
                return Err(self.error_here("RENAME COLUMN is not valid for this object type"));
            }
            rename_type = ObjectType::Column;
            relation_type = identity.object_type;
            identity.subname = Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("RENAME COLUMN requires a column name"))?,
            );
        } else if self.consume(TokenKind::Constraint) {
            rename_type = if identity.object_type == ObjectType::Domain {
                ObjectType::Domconstraint
            } else if identity.object_type == ObjectType::Table {
                ObjectType::Tabconstraint
            } else {
                return Err(
                    self.error_here("RENAME CONSTRAINT is only valid for a table or domain")
                );
            };
            identity.subname = Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("RENAME CONSTRAINT requires a name"))?,
            );
        } else if self.consume(TokenKind::Attribute) {
            if identity.object_type != ObjectType::Type {
                return Err(self.error_here("RENAME ATTRIBUTE is only valid for a type"));
            }
            rename_type = ObjectType::Attribute;
            relation_type = ObjectType::Type;
            identity.subname = Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("RENAME ATTRIBUTE requires a name"))?,
            );
            let Some(Node::AArrayExpr(names)) = identity.object.as_deref() else {
                return Err(self.error_here("type name is not representable as a relation"));
            };
            identity.relation = Some(Box::new(range_var_from_parts(
                list_to_names(&names.elements),
                identity.location,
            )));
            identity.object = None;
        } else if matches!(
            identity.object_type,
            ObjectType::Table | ObjectType::View | ObjectType::Matview | ObjectType::ForeignTable
        ) && !self.at(TokenKind::To)
        {
            // COLUMN is optional in PostgreSQL's RENAME [COLUMN] syntax.
            rename_type = ObjectType::Column;
            relation_type = identity.object_type;
            identity.subname = Some(
                self.consume_col_id()
                    .ok_or_else(|| self.error_here("RENAME requires a column name or TO"))?,
            );
        }
        self.expect(TokenKind::To)?;
        let newname = Some(if identity.object_type == ObjectType::Role {
            self.consume_role_id()?
                .ok_or_else(|| self.error_here("RENAME TO requires a new role name"))?
        } else {
            self.consume_col_id()
                .ok_or_else(|| self.error_here("RENAME TO requires a new name"))?
        });
        if rename_type == ObjectType::Attribute {
            behavior = self.parse_drop_behavior();
        }
        if matches!(
            identity.object_type,
            ObjectType::Database
                | ObjectType::Role
                | ObjectType::Schema
                | ObjectType::Tablespace
                | ObjectType::Policy
                | ObjectType::Rule
                | ObjectType::Trigger
        ) {
            identity.object = None;
        }
        self.expect_statement_end()?;
        Ok(Node::RenameStmt(RenameStmt {
            node_tag: NodeTag::RenameStmt,
            rename_type,
            relation_type,
            relation: identity.relation,
            object: identity.object,
            subname: identity.subname,
            newname,
            behavior,
            missing_ok: identity.missing_ok,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterfunction.html
    // ALTER FUNCTION name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     action [ ... ] [ RESTRICT ]
    // ALTER FUNCTION name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     RENAME TO new_name
    // ALTER FUNCTION name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER FUNCTION name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     SET SCHEMA new_schema
    // ALTER FUNCTION name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     [ NO ] DEPENDS ON EXTENSION extension_name
    //
    // where action is one of:
    //
    //     CALLED ON NULL INPUT | RETURNS NULL ON NULL INPUT | STRICT
    //     IMMUTABLE | STABLE | VOLATILE
    //     [ NOT ] LEAKPROOF
    //     [ EXTERNAL ] SECURITY INVOKER | [ EXTERNAL ] SECURITY DEFINER
    //     PARALLEL { UNSAFE | RESTRICTED | SAFE }
    //     COST execution_cost
    //     ROWS result_rows
    //     SUPPORT support_function
    //     SET configuration_parameter { TO | = } { value | DEFAULT }
    //     SET configuration_parameter FROM CURRENT
    //     RESET configuration_parameter
    //     RESET ALL
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterprocedure.html
    // ALTER PROCEDURE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     action [ ... ] [ RESTRICT ]
    // ALTER PROCEDURE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     RENAME TO new_name
    // ALTER PROCEDURE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER PROCEDURE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     SET SCHEMA new_schema
    // ALTER PROCEDURE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     [ NO ] DEPENDS ON EXTENSION extension_name
    //
    // where action is one of:
    //
    //     [ EXTERNAL ] SECURITY INVOKER | [ EXTERNAL ] SECURITY DEFINER
    //     SET configuration_parameter { TO | = } { value | DEFAULT }
    //     SET configuration_parameter FROM CURRENT
    //     RESET configuration_parameter
    //     RESET ALL
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterroutine.html
    // ALTER ROUTINE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     action [ ... ] [ RESTRICT ]
    // ALTER ROUTINE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     RENAME TO new_name
    // ALTER ROUTINE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER ROUTINE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     SET SCHEMA new_schema
    // ALTER ROUTINE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     [ NO ] DEPENDS ON EXTENSION extension_name
    //
    // where action is one of:
    //
    //     IMMUTABLE | STABLE | VOLATILE
    //     [ NOT ] LEAKPROOF
    //     [ EXTERNAL ] SECURITY INVOKER | [ EXTERNAL ] SECURITY DEFINER
    //     PARALLEL { UNSAFE | RESTRICTED | SAFE }
    //     COST execution_cost
    //     ROWS result_rows
    //     SET configuration_parameter { TO | = } { value | DEFAULT }
    //     SET configuration_parameter FROM CURRENT
    //     RESET configuration_parameter
    //     RESET ALL
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altertrigger.html
    // ALTER TRIGGER name ON table_name RENAME TO new_name
    // ALTER TRIGGER name ON table_name [ NO ] DEPENDS ON EXTENSION extension_name
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altermaterializedview.html
    // ALTER MATERIALIZED VIEW [ IF EXISTS ] name
    //     action [, ... ]
    // ALTER MATERIALIZED VIEW name
    //     [ NO ] DEPENDS ON EXTENSION extension_name
    // ALTER MATERIALIZED VIEW [ IF EXISTS ] name
    //     RENAME [ COLUMN ] column_name TO new_column_name
    // ALTER MATERIALIZED VIEW [ IF EXISTS ] name
    //     RENAME TO new_name
    // ALTER MATERIALIZED VIEW [ IF EXISTS ] name
    //     SET SCHEMA new_schema
    // ALTER MATERIALIZED VIEW ALL IN TABLESPACE name [ OWNED BY role_name [, ... ] ]
    //     SET TABLESPACE new_tablespace [ NOWAIT ]
    //
    // where action is one of:
    //
    //     ALTER [ COLUMN ] column_name SET STATISTICS integer
    //     ALTER [ COLUMN ] column_name SET ( attribute_option = value [, ... ] )
    //     ALTER [ COLUMN ] column_name RESET ( attribute_option [, ... ] )
    //     ALTER [ COLUMN ] column_name SET STORAGE { PLAIN | EXTERNAL | EXTENDED | MAIN | DEFAULT }
    //     ALTER [ COLUMN ] column_name SET COMPRESSION compression_method
    //     CLUSTER ON index_name
    //     SET WITHOUT CLUSTER
    //     SET ACCESS METHOD new_access_method
    //     SET TABLESPACE new_tablespace
    //     SET ( storage_parameter [= value] [, ... ] )
    //     RESET ( storage_parameter [, ... ] )
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterindex.html
    // ALTER INDEX [ IF EXISTS ] name RENAME TO new_name
    // ALTER INDEX [ IF EXISTS ] name SET TABLESPACE tablespace_name
    // ALTER INDEX name ATTACH PARTITION index_name
    // ALTER INDEX name [ NO ] DEPENDS ON EXTENSION extension_name
    // ALTER INDEX [ IF EXISTS ] name SET ( storage_parameter [= value] [, ... ] )
    // ALTER INDEX [ IF EXISTS ] name RESET ( storage_parameter [, ... ] )
    // ALTER INDEX [ IF EXISTS ] name ALTER [ COLUMN ] column_number
    //     SET STATISTICS integer
    // ALTER INDEX ALL IN TABLESPACE name [ OWNED BY role_name [, ... ] ]
    //     SET TABLESPACE new_tablespace [ NOWAIT ]
    pub(super) fn parse_alter_object_depends(&mut self) -> PResult<Node> {
        let mut identity = self.parse_alter_identity(&[TokenKind::No, TokenKind::Depends])?;
        if identity.missing_ok {
            return Err(self.error_here("IF EXISTS is not supported with DEPENDS ON EXTENSION"));
        }
        if !matches!(
            identity.object_type,
            ObjectType::Function
                | ObjectType::Procedure
                | ObjectType::Routine
                | ObjectType::Trigger
                | ObjectType::Matview
                | ObjectType::Index
        ) {
            return Err(self.error_here("this object type does not support DEPENDS ON EXTENSION"));
        }
        let remove = self.consume(TokenKind::No);
        self.expect(TokenKind::Depends)?;
        self.expect(TokenKind::On)?;
        self.expect(TokenKind::Extension)?;
        let extname = Some(Box::new(String::new(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("EXTENSION requires a name"))?,
        )));
        self.expect_statement_end()?;
        Ok(Node::AlterObjectDependsStmt(AlterObjectDependsStmt {
            node_tag: NodeTag::AlterObjectDependsStmt,
            object_type: identity.object_type,
            relation: identity.relation,
            object: identity.object.take(),
            extname,
            remove,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alteraggregate.html
    // ALTER AGGREGATE name ( aggregate_signature ) RENAME TO new_name
    // ALTER AGGREGATE name ( aggregate_signature )
    //                 OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER AGGREGATE name ( aggregate_signature ) SET SCHEMA new_schema
    //
    // where aggregate_signature is:
    //
    // * |
    // [ argmode ] [ argname ] argtype [ , ... ] |
    // [ [ argmode ] [ argname ] argtype [ , ... ] ] ORDER BY [ argmode ] [ argname ] argtype [ , ... ]
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altercollation.html
    // ALTER COLLATION name REFRESH VERSION
    //
    // ALTER COLLATION name RENAME TO new_name
    // ALTER COLLATION name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER COLLATION name SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterconversion.html
    // ALTER CONVERSION name RENAME TO new_name
    // ALTER CONVERSION name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER CONVERSION name SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterdomain.html
    // ALTER DOMAIN name
    //     { SET DEFAULT expression | DROP DEFAULT }
    // ALTER DOMAIN name
    //     { SET | DROP } NOT NULL
    // ALTER DOMAIN name
    //     ADD domain_constraint [ NOT VALID ]
    // ALTER DOMAIN name
    //     DROP CONSTRAINT [ IF EXISTS ] constraint_name [ RESTRICT | CASCADE ]
    // ALTER DOMAIN name
    //      RENAME CONSTRAINT constraint_name TO new_constraint_name
    // ALTER DOMAIN name
    //     VALIDATE CONSTRAINT constraint_name
    // ALTER DOMAIN name
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER DOMAIN name
    //     RENAME TO new_name
    // ALTER DOMAIN name
    //     SET SCHEMA new_schema
    //
    // where domain_constraint is:
    //
    // [ CONSTRAINT constraint_name ]
    // { NOT NULL | CHECK (expression) }
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterextension.html
    // ALTER EXTENSION name UPDATE [ TO new_version ]
    // ALTER EXTENSION name SET SCHEMA new_schema
    // ALTER EXTENSION name ADD member_object
    // ALTER EXTENSION name DROP member_object
    //
    // where member_object is:
    //
    //   ACCESS METHOD object_name |
    //   AGGREGATE aggregate_name ( aggregate_signature ) |
    //   CAST (source_type AS target_type) |
    //   COLLATION object_name |
    //   CONVERSION object_name |
    //   DOMAIN object_name |
    //   EVENT TRIGGER object_name |
    //   FOREIGN DATA WRAPPER object_name |
    //   FOREIGN TABLE object_name |
    //   FUNCTION function_name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ] |
    //   MATERIALIZED VIEW object_name |
    //   OPERATOR operator_name (left_type, right_type) |
    //   OPERATOR CLASS object_name USING index_method |
    //   OPERATOR FAMILY object_name USING index_method |
    //   [ PROCEDURAL ] LANGUAGE object_name |
    //   PROCEDURE procedure_name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ] |
    //   ROUTINE routine_name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ] |
    //   SCHEMA object_name |
    //   SEQUENCE object_name |
    //   SERVER object_name |
    //   TABLE object_name |
    //   TEXT SEARCH CONFIGURATION object_name |
    //   TEXT SEARCH DICTIONARY object_name |
    //   TEXT SEARCH PARSER object_name |
    //   TEXT SEARCH TEMPLATE object_name |
    //   TRANSFORM FOR type_name LANGUAGE lang_name |
    //   TYPE object_name |
    //   VIEW object_name
    //
    // and aggregate_signature is:
    //
    // * |
    // [ argmode ] [ argname ] argtype [ , ... ] |
    // [ [ argmode ] [ argname ] argtype [ , ... ] ] ORDER BY [ argmode ] [ argname ] argtype [ , ... ]
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterfunction.html
    // ALTER FUNCTION name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     action [ ... ] [ RESTRICT ]
    // ALTER FUNCTION name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     RENAME TO new_name
    // ALTER FUNCTION name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER FUNCTION name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     SET SCHEMA new_schema
    // ALTER FUNCTION name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     [ NO ] DEPENDS ON EXTENSION extension_name
    //
    // where action is one of:
    //
    //     CALLED ON NULL INPUT | RETURNS NULL ON NULL INPUT | STRICT
    //     IMMUTABLE | STABLE | VOLATILE
    //     [ NOT ] LEAKPROOF
    //     [ EXTERNAL ] SECURITY INVOKER | [ EXTERNAL ] SECURITY DEFINER
    //     PARALLEL { UNSAFE | RESTRICTED | SAFE }
    //     COST execution_cost
    //     ROWS result_rows
    //     SUPPORT support_function
    //     SET configuration_parameter { TO | = } { value | DEFAULT }
    //     SET configuration_parameter FROM CURRENT
    //     RESET configuration_parameter
    //     RESET ALL
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alteroperator.html
    // ALTER OPERATOR name ( { left_type | NONE } , right_type )
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //
    // ALTER OPERATOR name ( { left_type | NONE } , right_type )
    //     SET SCHEMA new_schema
    //
    // ALTER OPERATOR name ( { left_type | NONE } , right_type )
    //     SET ( {  RESTRICT = { res_proc | NONE }
    //            | JOIN = { join_proc | NONE }
    //            | COMMUTATOR = com_op
    //            | NEGATOR = neg_op
    //            | HASHES
    //            | MERGES
    //           } [, ... ] )
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alteropclass.html
    // ALTER OPERATOR CLASS name USING index_method
    //     RENAME TO new_name
    //
    // ALTER OPERATOR CLASS name USING index_method
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //
    // ALTER OPERATOR CLASS name USING index_method
    //     SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alteropfamily.html
    // ALTER OPERATOR FAMILY name USING index_method ADD
    //   {  OPERATOR strategy_number operator_name ( op_type, op_type )
    //               [ FOR SEARCH | FOR ORDER BY sort_family_name ]
    //    | FUNCTION support_number [ ( op_type [ , op_type ] ) ]
    //               function_name [ ( argument_type [, ...] ) ]
    //   } [, ... ]
    //
    // ALTER OPERATOR FAMILY name USING index_method DROP
    //   {  OPERATOR strategy_number ( op_type [ , op_type ] )
    //    | FUNCTION support_number ( op_type [ , op_type ] )
    //   } [, ... ]
    //
    // ALTER OPERATOR FAMILY name USING index_method
    //     RENAME TO new_name
    //
    // ALTER OPERATOR FAMILY name USING index_method
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //
    // ALTER OPERATOR FAMILY name USING index_method
    //     SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterprocedure.html
    // ALTER PROCEDURE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     action [ ... ] [ RESTRICT ]
    // ALTER PROCEDURE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     RENAME TO new_name
    // ALTER PROCEDURE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER PROCEDURE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     SET SCHEMA new_schema
    // ALTER PROCEDURE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     [ NO ] DEPENDS ON EXTENSION extension_name
    //
    // where action is one of:
    //
    //     [ EXTERNAL ] SECURITY INVOKER | [ EXTERNAL ] SECURITY DEFINER
    //     SET configuration_parameter { TO | = } { value | DEFAULT }
    //     SET configuration_parameter FROM CURRENT
    //     RESET configuration_parameter
    //     RESET ALL
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterroutine.html
    // ALTER ROUTINE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     action [ ... ] [ RESTRICT ]
    // ALTER ROUTINE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     RENAME TO new_name
    // ALTER ROUTINE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER ROUTINE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     SET SCHEMA new_schema
    // ALTER ROUTINE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     [ NO ] DEPENDS ON EXTENSION extension_name
    //
    // where action is one of:
    //
    //     IMMUTABLE | STABLE | VOLATILE
    //     [ NOT ] LEAKPROOF
    //     [ EXTERNAL ] SECURITY INVOKER | [ EXTERNAL ] SECURITY DEFINER
    //     PARALLEL { UNSAFE | RESTRICTED | SAFE }
    //     COST execution_cost
    //     ROWS result_rows
    //     SET configuration_parameter { TO | = } { value | DEFAULT }
    //     SET configuration_parameter FROM CURRENT
    //     RESET configuration_parameter
    //     RESET ALL
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altertable.html
    // ALTER TABLE [ IF EXISTS ] [ ONLY ] name [ * ]
    //     action [, ... ]
    // ALTER TABLE [ IF EXISTS ] [ ONLY ] name [ * ]
    //     RENAME [ COLUMN ] column_name TO new_column_name
    // ALTER TABLE [ IF EXISTS ] [ ONLY ] name [ * ]
    //     RENAME CONSTRAINT constraint_name TO new_constraint_name
    // ALTER TABLE [ IF EXISTS ] name
    //     RENAME TO new_name
    // ALTER TABLE [ IF EXISTS ] name
    //     SET SCHEMA new_schema
    // ALTER TABLE ALL IN TABLESPACE name [ OWNED BY role_name [, ... ] ]
    //     SET TABLESPACE new_tablespace [ NOWAIT ]
    // ALTER TABLE [ IF EXISTS ] name
    //     ATTACH PARTITION partition_name { FOR VALUES partition_bound_spec | DEFAULT }
    // ALTER TABLE [ IF EXISTS ] name
    //     DETACH PARTITION partition_name [ CONCURRENTLY | FINALIZE ]
    //
    // where action is one of:
    //
    //     ADD [ COLUMN ] [ IF NOT EXISTS ] column_name data_type [ COLLATE collation ] [ column_constraint [ ... ] ]
    //     DROP [ COLUMN ] [ IF EXISTS ] column_name [ RESTRICT | CASCADE ]
    //     ALTER [ COLUMN ] column_name [ SET DATA ] TYPE data_type [ COLLATE collation ] [ USING expression ]
    //     ALTER [ COLUMN ] column_name SET DEFAULT expression
    //     ALTER [ COLUMN ] column_name DROP DEFAULT
    //     ALTER [ COLUMN ] column_name { SET | DROP } NOT NULL
    //     ALTER [ COLUMN ] column_name SET EXPRESSION AS ( expression )
    //     ALTER [ COLUMN ] column_name DROP EXPRESSION [ IF EXISTS ]
    //     ALTER [ COLUMN ] column_name ADD GENERATED { ALWAYS | BY DEFAULT } AS IDENTITY [ ( sequence_options ) ]
    //     ALTER [ COLUMN ] column_name { SET GENERATED { ALWAYS | BY DEFAULT } | SET sequence_option | RESTART [ [ WITH ] restart ] } [...]
    //     ALTER [ COLUMN ] column_name DROP IDENTITY [ IF EXISTS ]
    //     ALTER [ COLUMN ] column_name SET STATISTICS { integer | DEFAULT }
    //     ALTER [ COLUMN ] column_name SET ( attribute_option = value [, ... ] )
    //     ALTER [ COLUMN ] column_name RESET ( attribute_option [, ... ] )
    //     ALTER [ COLUMN ] column_name SET STORAGE { PLAIN | EXTERNAL | EXTENDED | MAIN | DEFAULT }
    //     ALTER [ COLUMN ] column_name SET COMPRESSION compression_method
    //     ADD table_constraint [ NOT VALID ]
    //     ADD table_constraint_using_index
    //     ALTER CONSTRAINT constraint_name [ DEFERRABLE | NOT DEFERRABLE ] [ INITIALLY DEFERRED | INITIALLY IMMEDIATE ] [ ENFORCED | NOT ENFORCED ]
    //     ALTER CONSTRAINT constraint_name [ INHERIT | NO INHERIT ]
    //     VALIDATE CONSTRAINT constraint_name
    //     DROP CONSTRAINT [ IF EXISTS ]  constraint_name [ RESTRICT | CASCADE ]
    //     DISABLE TRIGGER [ trigger_name | ALL | USER ]
    //     ENABLE TRIGGER [ trigger_name | ALL | USER ]
    //     ENABLE REPLICA TRIGGER trigger_name
    //     ENABLE ALWAYS TRIGGER trigger_name
    //     DISABLE RULE rewrite_rule_name
    //     ENABLE RULE rewrite_rule_name
    //     ENABLE REPLICA RULE rewrite_rule_name
    //     ENABLE ALWAYS RULE rewrite_rule_name
    //     DISABLE ROW LEVEL SECURITY
    //     ENABLE ROW LEVEL SECURITY
    //     FORCE ROW LEVEL SECURITY
    //     NO FORCE ROW LEVEL SECURITY
    //     CLUSTER ON index_name
    //     SET WITHOUT CLUSTER
    //     SET WITHOUT OIDS
    //     SET ACCESS METHOD { new_access_method | DEFAULT }
    //     SET TABLESPACE new_tablespace
    //     SET { LOGGED | UNLOGGED }
    //     SET ( storage_parameter [= value] [, ... ] )
    //     RESET ( storage_parameter [, ... ] )
    //     INHERIT parent_table
    //     NO INHERIT parent_table
    //     OF type_name
    //     NOT OF
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //     REPLICA IDENTITY { DEFAULT | USING INDEX index_name | FULL | NOTHING }
    //
    // and partition_bound_spec is:
    //
    // IN ( partition_bound_expr [, ...] ) |
    // FROM ( { partition_bound_expr | MINVALUE | MAXVALUE } [, ...] )
    //   TO ( { partition_bound_expr | MINVALUE | MAXVALUE } [, ...] ) |
    // WITH ( MODULUS numeric_literal, REMAINDER numeric_literal )
    //
    // and column_constraint is:
    //
    // [ CONSTRAINT constraint_name ]
    // { NOT NULL [ NO INHERIT ] |
    //   NULL |
    //   CHECK ( expression ) [ NO INHERIT ] |
    //   DEFAULT default_expr |
    //   GENERATED ALWAYS AS ( generation_expr ) [ STORED | VIRTUAL ] |
    //   GENERATED { ALWAYS | BY DEFAULT } AS IDENTITY [ ( sequence_options ) ] |
    //   UNIQUE [ NULLS [ NOT ] DISTINCT ] index_parameters |
    //   PRIMARY KEY index_parameters |
    //   REFERENCES reftable [ ( refcolumn ) ] [ MATCH FULL | MATCH PARTIAL | MATCH SIMPLE ]
    //     [ ON DELETE referential_action ] [ ON UPDATE referential_action ] }
    // [ DEFERRABLE | NOT DEFERRABLE ] [ INITIALLY DEFERRED | INITIALLY IMMEDIATE ] [ ENFORCED | NOT ENFORCED ]
    //
    // and table_constraint is:
    //
    // [ CONSTRAINT constraint_name ]
    // { CHECK ( expression ) [ NO INHERIT ] |
    //   NOT NULL column_name [ NO INHERIT ] |
    //   UNIQUE [ NULLS [ NOT ] DISTINCT ] ( column_name [, ... ] [, column_name WITHOUT OVERLAPS ] ) index_parameters |
    //   PRIMARY KEY ( column_name [, ... ] [, column_name WITHOUT OVERLAPS ] ) index_parameters |
    //   EXCLUDE [ USING index_method ] ( exclude_element WITH operator [, ... ] ) index_parameters [ WHERE ( predicate ) ] |
    //   FOREIGN KEY ( column_name [, ... ] [, PERIOD column_name ] ) REFERENCES reftable [ ( refcolumn [, ... ]  [, PERIOD refcolumn ] ) ]
    //     [ MATCH FULL | MATCH PARTIAL | MATCH SIMPLE ] [ ON DELETE referential_action ] [ ON UPDATE referential_action ] }
    // [ DEFERRABLE | NOT DEFERRABLE ] [ INITIALLY DEFERRED | INITIALLY IMMEDIATE ] [ ENFORCED | NOT ENFORCED ]
    //
    // and table_constraint_using_index is:
    //
    //     [ CONSTRAINT constraint_name ]
    //     { UNIQUE | PRIMARY KEY } USING INDEX index_name
    //     [ DEFERRABLE | NOT DEFERRABLE ] [ INITIALLY DEFERRED | INITIALLY IMMEDIATE ]
    //
    // index_parameters in UNIQUE, PRIMARY KEY, and EXCLUDE constraints are:
    //
    // [ INCLUDE ( column_name [, ... ] ) ]
    // [ WITH ( storage_parameter [= value] [, ... ] ) ]
    // [ USING INDEX TABLESPACE tablespace_name ]
    //
    // exclude_element in an EXCLUDE constraint is:
    //
    // { column_name | ( expression ) } [ COLLATE collation ] [ opclass [ ( opclass_parameter = value [, ... ] ) ] ] [ ASC | DESC ] [ NULLS { FIRST | LAST } ]
    //
    // referential_action in a FOREIGN KEY/REFERENCES constraint is:
    //
    // { NO ACTION | RESTRICT | CASCADE | SET NULL [ ( column_name [, ... ] ) ] | SET DEFAULT [ ( column_name [, ... ] ) ] }
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterstatistics.html
    // ALTER STATISTICS name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER STATISTICS name RENAME TO new_name
    // ALTER STATISTICS name SET SCHEMA new_schema
    // ALTER STATISTICS name SET STATISTICS { new_target | DEFAULT }
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altertsparser.html
    // ALTER TEXT SEARCH PARSER name RENAME TO new_name
    // ALTER TEXT SEARCH PARSER name SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altertsdictionary.html
    // ALTER TEXT SEARCH DICTIONARY name (
    //     option [ = value ] [, ... ]
    // )
    // ALTER TEXT SEARCH DICTIONARY name RENAME TO new_name
    // ALTER TEXT SEARCH DICTIONARY name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER TEXT SEARCH DICTIONARY name SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altertstemplate.html
    // ALTER TEXT SEARCH TEMPLATE name RENAME TO new_name
    // ALTER TEXT SEARCH TEMPLATE name SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altertsconfig.html
    // ALTER TEXT SEARCH CONFIGURATION name
    //     ADD MAPPING FOR token_type [, ... ] WITH dictionary_name [, ... ]
    // ALTER TEXT SEARCH CONFIGURATION name
    //     ALTER MAPPING FOR token_type [, ... ] WITH dictionary_name [, ... ]
    // ALTER TEXT SEARCH CONFIGURATION name
    //     ALTER MAPPING REPLACE old_dictionary WITH new_dictionary
    // ALTER TEXT SEARCH CONFIGURATION name
    //     ALTER MAPPING FOR token_type [, ... ] REPLACE old_dictionary WITH new_dictionary
    // ALTER TEXT SEARCH CONFIGURATION name
    //     DROP MAPPING [ IF EXISTS ] FOR token_type [, ... ]
    // ALTER TEXT SEARCH CONFIGURATION name RENAME TO new_name
    // ALTER TEXT SEARCH CONFIGURATION name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER TEXT SEARCH CONFIGURATION name SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altersequence.html
    // ALTER SEQUENCE [ IF EXISTS ] name
    //     [ AS data_type ]
    //     [ INCREMENT [ BY ] increment ]
    //     [ MINVALUE minvalue | NO MINVALUE ] [ MAXVALUE maxvalue | NO MAXVALUE ]
    //     [ [ NO ] CYCLE ]
    //     [ START [ WITH ] start ]
    //     [ RESTART [ [ WITH ] restart ] ]
    //     [ CACHE cache ]
    //     [ OWNED BY { table_name.column_name | NONE } ]
    // ALTER SEQUENCE [ IF EXISTS ] name SET { LOGGED | UNLOGGED }
    // ALTER SEQUENCE [ IF EXISTS ] name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER SEQUENCE [ IF EXISTS ] name RENAME TO new_name
    // ALTER SEQUENCE [ IF EXISTS ] name SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterview.html
    // ALTER VIEW [ IF EXISTS ] name ALTER [ COLUMN ] column_name SET DEFAULT expression
    // ALTER VIEW [ IF EXISTS ] name ALTER [ COLUMN ] column_name DROP DEFAULT
    // ALTER VIEW [ IF EXISTS ] name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER VIEW [ IF EXISTS ] name RENAME [ COLUMN ] column_name TO new_column_name
    // ALTER VIEW [ IF EXISTS ] name RENAME TO new_name
    // ALTER VIEW [ IF EXISTS ] name SET SCHEMA new_schema
    // ALTER VIEW [ IF EXISTS ] name SET ( view_option_name [= view_option_value] [, ... ] )
    // ALTER VIEW [ IF EXISTS ] name RESET ( view_option_name [, ... ] )
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altermaterializedview.html
    // ALTER MATERIALIZED VIEW [ IF EXISTS ] name
    //     action [, ... ]
    // ALTER MATERIALIZED VIEW name
    //     [ NO ] DEPENDS ON EXTENSION extension_name
    // ALTER MATERIALIZED VIEW [ IF EXISTS ] name
    //     RENAME [ COLUMN ] column_name TO new_column_name
    // ALTER MATERIALIZED VIEW [ IF EXISTS ] name
    //     RENAME TO new_name
    // ALTER MATERIALIZED VIEW [ IF EXISTS ] name
    //     SET SCHEMA new_schema
    // ALTER MATERIALIZED VIEW ALL IN TABLESPACE name [ OWNED BY role_name [, ... ] ]
    //     SET TABLESPACE new_tablespace [ NOWAIT ]
    //
    // where action is one of:
    //
    //     ALTER [ COLUMN ] column_name SET STATISTICS integer
    //     ALTER [ COLUMN ] column_name SET ( attribute_option = value [, ... ] )
    //     ALTER [ COLUMN ] column_name RESET ( attribute_option [, ... ] )
    //     ALTER [ COLUMN ] column_name SET STORAGE { PLAIN | EXTERNAL | EXTENDED | MAIN | DEFAULT }
    //     ALTER [ COLUMN ] column_name SET COMPRESSION compression_method
    //     CLUSTER ON index_name
    //     SET WITHOUT CLUSTER
    //     SET ACCESS METHOD new_access_method
    //     SET TABLESPACE new_tablespace
    //     SET ( storage_parameter [= value] [, ... ] )
    //     RESET ( storage_parameter [, ... ] )
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterforeigntable.html
    // ALTER FOREIGN TABLE [ IF EXISTS ] [ ONLY ] name [ * ]
    //     action [, ... ]
    // ALTER FOREIGN TABLE [ IF EXISTS ] [ ONLY ] name [ * ]
    //     RENAME [ COLUMN ] column_name TO new_column_name
    // ALTER FOREIGN TABLE [ IF EXISTS ] name
    //     RENAME TO new_name
    // ALTER FOREIGN TABLE [ IF EXISTS ] name
    //     SET SCHEMA new_schema
    //
    // where action is one of:
    //
    //     ADD [ COLUMN ] column_name data_type [ COLLATE collation ] [ column_constraint [ ... ] ]
    //     DROP [ COLUMN ] [ IF EXISTS ] column_name [ RESTRICT | CASCADE ]
    //     ALTER [ COLUMN ] column_name [ SET DATA ] TYPE data_type [ COLLATE collation ]
    //     ALTER [ COLUMN ] column_name SET DEFAULT expression
    //     ALTER [ COLUMN ] column_name DROP DEFAULT
    //     ALTER [ COLUMN ] column_name { SET | DROP } NOT NULL
    //     ALTER [ COLUMN ] column_name SET STATISTICS integer
    //     ALTER [ COLUMN ] column_name SET ( attribute_option = value [, ... ] )
    //     ALTER [ COLUMN ] column_name RESET ( attribute_option [, ... ] )
    //     ALTER [ COLUMN ] column_name SET STORAGE { PLAIN | EXTERNAL | EXTENDED | MAIN | DEFAULT }
    //     ALTER [ COLUMN ] column_name OPTIONS ( [ ADD | SET | DROP ] option ['value'] [, ... ])
    //     ADD table_constraint [ NOT VALID ]
    //     VALIDATE CONSTRAINT constraint_name
    //     DROP CONSTRAINT [ IF EXISTS ]  constraint_name [ RESTRICT | CASCADE ]
    //     DISABLE TRIGGER [ trigger_name | ALL | USER ]
    //     ENABLE TRIGGER [ trigger_name | ALL | USER ]
    //     ENABLE REPLICA TRIGGER trigger_name
    //     ENABLE ALWAYS TRIGGER trigger_name
    //     SET WITHOUT OIDS
    //     INHERIT parent_table
    //     NO INHERIT parent_table
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //     OPTIONS ( [ ADD | SET | DROP ] option ['value'] [, ... ])
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altertype.html
    // ALTER TYPE name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER TYPE name RENAME TO new_name
    // ALTER TYPE name SET SCHEMA new_schema
    // ALTER TYPE name RENAME ATTRIBUTE attribute_name TO new_attribute_name [ CASCADE | RESTRICT ]
    // ALTER TYPE name action [, ... ]
    // ALTER TYPE name ADD VALUE [ IF NOT EXISTS ] new_enum_value [ { BEFORE | AFTER } neighbor_enum_value ]
    // ALTER TYPE name RENAME VALUE existing_enum_value TO new_enum_value
    // ALTER TYPE name SET ( property = value [, ... ] )
    //
    // where action is one of:
    //
    //     ADD ATTRIBUTE attribute_name data_type [ COLLATE collation ] [ CASCADE | RESTRICT ]
    //     DROP ATTRIBUTE [ IF EXISTS ] attribute_name [ CASCADE | RESTRICT ]
    //     ALTER ATTRIBUTE attribute_name [ SET DATA ] TYPE data_type [ COLLATE collation ] [ CASCADE | RESTRICT ]
    pub(super) fn parse_alter_object_schema(&mut self) -> PResult<Node> {
        let identity = self.parse_alter_identity(&[TokenKind::Set])?;
        if identity.missing_ok
            && !matches!(
                identity.object_type,
                ObjectType::Propgraph
                    | ObjectType::Table
                    | ObjectType::Sequence
                    | ObjectType::View
                    | ObjectType::Matview
                    | ObjectType::ForeignTable
            )
        {
            return Err(self.error_here("IF EXISTS is not supported for this SET SCHEMA object"));
        }
        if !matches!(
            identity.object_type,
            ObjectType::Aggregate
                | ObjectType::Collation
                | ObjectType::Conversion
                | ObjectType::Domain
                | ObjectType::Extension
                | ObjectType::Function
                | ObjectType::Operator
                | ObjectType::Opclass
                | ObjectType::Opfamily
                | ObjectType::Procedure
                | ObjectType::Propgraph
                | ObjectType::Routine
                | ObjectType::Table
                | ObjectType::StatisticExt
                | ObjectType::Tsparser
                | ObjectType::Tsdictionary
                | ObjectType::Tstemplate
                | ObjectType::Tsconfiguration
                | ObjectType::Sequence
                | ObjectType::View
                | ObjectType::Matview
                | ObjectType::ForeignTable
                | ObjectType::Type
        ) {
            return Err(self.error_here("this object type does not support SET SCHEMA"));
        }
        self.expect(TokenKind::Set)?;
        self.expect(TokenKind::Schema)?;
        let newschema = Some(
            self.consume_col_id()
                .ok_or_else(|| self.error_here("SET SCHEMA requires a schema name"))?,
        );
        self.expect_statement_end()?;
        Ok(Node::AlterObjectSchemaStmt(AlterObjectSchemaStmt {
            node_tag: NodeTag::AlterObjectSchemaStmt,
            object_type: identity.object_type,
            relation: identity.relation,
            object: identity.object,
            newschema,
            missing_ok: identity.missing_ok,
        }))
    }

    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alteraggregate.html
    // ALTER AGGREGATE name ( aggregate_signature ) RENAME TO new_name
    // ALTER AGGREGATE name ( aggregate_signature )
    //                 OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER AGGREGATE name ( aggregate_signature ) SET SCHEMA new_schema
    //
    // where aggregate_signature is:
    //
    // * |
    // [ argmode ] [ argname ] argtype [ , ... ] |
    // [ [ argmode ] [ argname ] argtype [ , ... ] ] ORDER BY [ argmode ] [ argname ] argtype [ , ... ]
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altercollation.html
    // ALTER COLLATION name REFRESH VERSION
    //
    // ALTER COLLATION name RENAME TO new_name
    // ALTER COLLATION name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER COLLATION name SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterconversion.html
    // ALTER CONVERSION name RENAME TO new_name
    // ALTER CONVERSION name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER CONVERSION name SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterdatabase.html
    // ALTER DATABASE name [ [ WITH ] option [ ... ] ]
    //
    // where option can be:
    //
    //     ALLOW_CONNECTIONS allowconn
    //     CONNECTION LIMIT connlimit
    //     IS_TEMPLATE istemplate
    //
    // ALTER DATABASE name RENAME TO new_name
    //
    // ALTER DATABASE name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //
    // ALTER DATABASE name SET TABLESPACE new_tablespace
    //
    // ALTER DATABASE name REFRESH COLLATION VERSION
    //
    // ALTER DATABASE name SET configuration_parameter { TO | = } { value | DEFAULT }
    // ALTER DATABASE name SET configuration_parameter FROM CURRENT
    // ALTER DATABASE name RESET configuration_parameter
    // ALTER DATABASE name RESET ALL
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterdomain.html
    // ALTER DOMAIN name
    //     { SET DEFAULT expression | DROP DEFAULT }
    // ALTER DOMAIN name
    //     { SET | DROP } NOT NULL
    // ALTER DOMAIN name
    //     ADD domain_constraint [ NOT VALID ]
    // ALTER DOMAIN name
    //     DROP CONSTRAINT [ IF EXISTS ] constraint_name [ RESTRICT | CASCADE ]
    // ALTER DOMAIN name
    //      RENAME CONSTRAINT constraint_name TO new_constraint_name
    // ALTER DOMAIN name
    //     VALIDATE CONSTRAINT constraint_name
    // ALTER DOMAIN name
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER DOMAIN name
    //     RENAME TO new_name
    // ALTER DOMAIN name
    //     SET SCHEMA new_schema
    //
    // where domain_constraint is:
    //
    // [ CONSTRAINT constraint_name ]
    // { NOT NULL | CHECK (expression) }
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterfunction.html
    // ALTER FUNCTION name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     action [ ... ] [ RESTRICT ]
    // ALTER FUNCTION name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     RENAME TO new_name
    // ALTER FUNCTION name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER FUNCTION name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     SET SCHEMA new_schema
    // ALTER FUNCTION name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     [ NO ] DEPENDS ON EXTENSION extension_name
    //
    // where action is one of:
    //
    //     CALLED ON NULL INPUT | RETURNS NULL ON NULL INPUT | STRICT
    //     IMMUTABLE | STABLE | VOLATILE
    //     [ NOT ] LEAKPROOF
    //     [ EXTERNAL ] SECURITY INVOKER | [ EXTERNAL ] SECURITY DEFINER
    //     PARALLEL { UNSAFE | RESTRICTED | SAFE }
    //     COST execution_cost
    //     ROWS result_rows
    //     SUPPORT support_function
    //     SET configuration_parameter { TO | = } { value | DEFAULT }
    //     SET configuration_parameter FROM CURRENT
    //     RESET configuration_parameter
    //     RESET ALL
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterlanguage.html
    // ALTER [ PROCEDURAL ] LANGUAGE name RENAME TO new_name
    // ALTER [ PROCEDURAL ] LANGUAGE name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterlargeobject.html
    // ALTER LARGE OBJECT large_object_oid OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alteroperator.html
    // ALTER OPERATOR name ( { left_type | NONE } , right_type )
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //
    // ALTER OPERATOR name ( { left_type | NONE } , right_type )
    //     SET SCHEMA new_schema
    //
    // ALTER OPERATOR name ( { left_type | NONE } , right_type )
    //     SET ( {  RESTRICT = { res_proc | NONE }
    //            | JOIN = { join_proc | NONE }
    //            | COMMUTATOR = com_op
    //            | NEGATOR = neg_op
    //            | HASHES
    //            | MERGES
    //           } [, ... ] )
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alteropclass.html
    // ALTER OPERATOR CLASS name USING index_method
    //     RENAME TO new_name
    //
    // ALTER OPERATOR CLASS name USING index_method
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //
    // ALTER OPERATOR CLASS name USING index_method
    //     SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alteropfamily.html
    // ALTER OPERATOR FAMILY name USING index_method ADD
    //   {  OPERATOR strategy_number operator_name ( op_type, op_type )
    //               [ FOR SEARCH | FOR ORDER BY sort_family_name ]
    //    | FUNCTION support_number [ ( op_type [ , op_type ] ) ]
    //               function_name [ ( argument_type [, ...] ) ]
    //   } [, ... ]
    //
    // ALTER OPERATOR FAMILY name USING index_method DROP
    //   {  OPERATOR strategy_number ( op_type [ , op_type ] )
    //    | FUNCTION support_number ( op_type [ , op_type ] )
    //   } [, ... ]
    //
    // ALTER OPERATOR FAMILY name USING index_method
    //     RENAME TO new_name
    //
    // ALTER OPERATOR FAMILY name USING index_method
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //
    // ALTER OPERATOR FAMILY name USING index_method
    //     SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterprocedure.html
    // ALTER PROCEDURE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     action [ ... ] [ RESTRICT ]
    // ALTER PROCEDURE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     RENAME TO new_name
    // ALTER PROCEDURE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER PROCEDURE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     SET SCHEMA new_schema
    // ALTER PROCEDURE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     [ NO ] DEPENDS ON EXTENSION extension_name
    //
    // where action is one of:
    //
    //     [ EXTERNAL ] SECURITY INVOKER | [ EXTERNAL ] SECURITY DEFINER
    //     SET configuration_parameter { TO | = } { value | DEFAULT }
    //     SET configuration_parameter FROM CURRENT
    //     RESET configuration_parameter
    //     RESET ALL
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterroutine.html
    // ALTER ROUTINE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     action [ ... ] [ RESTRICT ]
    // ALTER ROUTINE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     RENAME TO new_name
    // ALTER ROUTINE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER ROUTINE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     SET SCHEMA new_schema
    // ALTER ROUTINE name [ ( [ [ argmode ] [ argname ] argtype [, ...] ] ) ]
    //     [ NO ] DEPENDS ON EXTENSION extension_name
    //
    // where action is one of:
    //
    //     IMMUTABLE | STABLE | VOLATILE
    //     [ NOT ] LEAKPROOF
    //     [ EXTERNAL ] SECURITY INVOKER | [ EXTERNAL ] SECURITY DEFINER
    //     PARALLEL { UNSAFE | RESTRICTED | SAFE }
    //     COST execution_cost
    //     ROWS result_rows
    //     SET configuration_parameter { TO | = } { value | DEFAULT }
    //     SET configuration_parameter FROM CURRENT
    //     RESET configuration_parameter
    //     RESET ALL
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterschema.html
    // ALTER SCHEMA name RENAME TO new_name
    // ALTER SCHEMA name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altertype.html
    // ALTER TYPE name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER TYPE name RENAME TO new_name
    // ALTER TYPE name SET SCHEMA new_schema
    // ALTER TYPE name RENAME ATTRIBUTE attribute_name TO new_attribute_name [ CASCADE | RESTRICT ]
    // ALTER TYPE name action [, ... ]
    // ALTER TYPE name ADD VALUE [ IF NOT EXISTS ] new_enum_value [ { BEFORE | AFTER } neighbor_enum_value ]
    // ALTER TYPE name RENAME VALUE existing_enum_value TO new_enum_value
    // ALTER TYPE name SET ( property = value [, ... ] )
    //
    // where action is one of:
    //
    //     ADD ATTRIBUTE attribute_name data_type [ COLLATE collation ] [ CASCADE | RESTRICT ]
    //     DROP ATTRIBUTE [ IF EXISTS ] attribute_name [ CASCADE | RESTRICT ]
    //     ALTER ATTRIBUTE attribute_name [ SET DATA ] TYPE data_type [ COLLATE collation ] [ CASCADE | RESTRICT ]
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altertablespace.html
    // ALTER TABLESPACE name RENAME TO new_name
    // ALTER TABLESPACE name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER TABLESPACE name SET ( tablespace_option = value [, ... ] )
    // ALTER TABLESPACE name RESET ( tablespace_option [, ... ] )
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterstatistics.html
    // ALTER STATISTICS name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER STATISTICS name RENAME TO new_name
    // ALTER STATISTICS name SET SCHEMA new_schema
    // ALTER STATISTICS name SET STATISTICS { new_target | DEFAULT }
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altertsdictionary.html
    // ALTER TEXT SEARCH DICTIONARY name (
    //     option [ = value ] [, ... ]
    // )
    // ALTER TEXT SEARCH DICTIONARY name RENAME TO new_name
    // ALTER TEXT SEARCH DICTIONARY name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER TEXT SEARCH DICTIONARY name SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altertsconfig.html
    // ALTER TEXT SEARCH CONFIGURATION name
    //     ADD MAPPING FOR token_type [, ... ] WITH dictionary_name [, ... ]
    // ALTER TEXT SEARCH CONFIGURATION name
    //     ALTER MAPPING FOR token_type [, ... ] WITH dictionary_name [, ... ]
    // ALTER TEXT SEARCH CONFIGURATION name
    //     ALTER MAPPING REPLACE old_dictionary WITH new_dictionary
    // ALTER TEXT SEARCH CONFIGURATION name
    //     ALTER MAPPING FOR token_type [, ... ] REPLACE old_dictionary WITH new_dictionary
    // ALTER TEXT SEARCH CONFIGURATION name
    //     DROP MAPPING [ IF EXISTS ] FOR token_type [, ... ]
    // ALTER TEXT SEARCH CONFIGURATION name RENAME TO new_name
    // ALTER TEXT SEARCH CONFIGURATION name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER TEXT SEARCH CONFIGURATION name SET SCHEMA new_schema
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterforeigndatawrapper.html
    // ALTER FOREIGN DATA WRAPPER name
    //     [ HANDLER handler_function | NO HANDLER ]
    //     [ VALIDATOR validator_function | NO VALIDATOR ]
    //     [ OPTIONS ( [ ADD | SET | DROP ] option ['value'] [, ... ]) ]
    // ALTER FOREIGN DATA WRAPPER name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER FOREIGN DATA WRAPPER name RENAME TO new_name
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterserver.html
    // ALTER SERVER name [ VERSION 'new_version' ]
    //     [ OPTIONS ( [ ADD | SET | DROP ] option ['value'] [, ... ] ) ]
    // ALTER SERVER name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER SERVER name RENAME TO new_name
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altereventtrigger.html
    // ALTER EVENT TRIGGER name DISABLE
    // ALTER EVENT TRIGGER name ENABLE [ REPLICA | ALWAYS ]
    // ALTER EVENT TRIGGER name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER EVENT TRIGGER name RENAME TO new_name
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-alterpublication.html
    // ALTER PUBLICATION name ADD publication_object [, ...]
    // ALTER PUBLICATION name SET publication_object [, ...]
    // ALTER PUBLICATION name DROP publication_object [, ...]
    // ALTER PUBLICATION name SET ( publication_parameter [= value] [, ... ] )
    // ALTER PUBLICATION name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER PUBLICATION name RENAME TO new_name
    //
    // where publication_object is one of:
    //
    //     TABLE [ ONLY ] table_name [ * ] [ ( column_name [, ... ] ) ] [ WHERE ( expression ) ] [, ... ]
    //     TABLES IN SCHEMA { schema_name | CURRENT_SCHEMA } [, ... ]
    //
    // PostgreSQL 18 Synopsis
    // Source: https://www.postgresql.org/docs/18/sql-altersubscription.html
    // ALTER SUBSCRIPTION name CONNECTION 'conninfo'
    // ALTER SUBSCRIPTION name SET PUBLICATION publication_name [, ...] [ WITH ( publication_option [= value] [, ... ] ) ]
    // ALTER SUBSCRIPTION name ADD PUBLICATION publication_name [, ...] [ WITH ( publication_option [= value] [, ... ] ) ]
    // ALTER SUBSCRIPTION name DROP PUBLICATION publication_name [, ...] [ WITH ( publication_option [= value] [, ... ] ) ]
    // ALTER SUBSCRIPTION name REFRESH PUBLICATION [ WITH ( refresh_option [= value] [, ... ] ) ]
    // ALTER SUBSCRIPTION name ENABLE
    // ALTER SUBSCRIPTION name DISABLE
    // ALTER SUBSCRIPTION name SET ( subscription_parameter [= value] [, ... ] )
    // ALTER SUBSCRIPTION name SKIP ( skip_option = value )
    // ALTER SUBSCRIPTION name OWNER TO { new_owner | CURRENT_ROLE | CURRENT_USER | SESSION_USER }
    // ALTER SUBSCRIPTION name RENAME TO new_name
    pub(super) fn parse_alter_owner(&mut self) -> PResult<Node> {
        let identity = self.parse_alter_identity(&[TokenKind::Owner])?;
        if identity.missing_ok {
            return Err(self.error_here("IF EXISTS is not supported with OWNER TO"));
        }
        if !matches!(
            identity.object_type,
            ObjectType::Aggregate
                | ObjectType::Collation
                | ObjectType::Conversion
                | ObjectType::Database
                | ObjectType::Domain
                | ObjectType::Function
                | ObjectType::Language
                | ObjectType::Largeobject
                | ObjectType::Operator
                | ObjectType::Opclass
                | ObjectType::Opfamily
                | ObjectType::Procedure
                | ObjectType::Propgraph
                | ObjectType::Routine
                | ObjectType::Schema
                | ObjectType::Type
                | ObjectType::Tablespace
                | ObjectType::StatisticExt
                | ObjectType::Tsdictionary
                | ObjectType::Tsconfiguration
                | ObjectType::Fdw
                | ObjectType::ForeignServer
                | ObjectType::EventTrigger
                | ObjectType::Publication
                | ObjectType::Subscription
        ) {
            return Err(self.error_here("this object type does not support OWNER TO"));
        }
        self.expect(TokenKind::Owner)?;
        self.expect(TokenKind::To)?;
        let newowner =
            Some(Box::new(self.consume_role_spec().ok_or_else(|| {
                self.error_here("OWNER TO requires a role")
            })?));
        self.expect_statement_end()?;
        Ok(Node::AlterOwnerStmt(AlterOwnerStmt {
            node_tag: NodeTag::AlterOwnerStmt,
            object_type: identity.object_type,
            relation: identity.relation,
            object: identity.object,
            newowner,
        }))
    }
}
pub(super) fn relation_object_type(object_type: ObjectType) -> bool {
    matches!(
        object_type,
        ObjectType::Table
            | ObjectType::Sequence
            | ObjectType::View
            | ObjectType::Matview
            | ObjectType::Index
            | ObjectType::ForeignTable
            | ObjectType::Propgraph
    )
}
