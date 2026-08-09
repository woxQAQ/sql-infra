use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use pg_parser::KEYWORDS;
use pg_parser::KeywordCategory;
use pg_parser::TextSize;
use pg_parser::collect_expectations;
use pg_parser::lex;

use super::smoke::CASES;

fn names_between(source: &str, start: &str, end: &str) -> BTreeSet<String> {
    let body = source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing {start:?}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing {end:?}"))
        .0;
    body.lines()
        .filter_map(|line| {
            let name = line.trim().split(['(', ',', ' ']).next()?;
            (!name.is_empty() && name.ends_with("Stmt")).then(|| name.to_owned())
        })
        .collect()
}

fn collect_rust_sources(path: &Path, output: &mut String) {
    for entry in fs::read_dir(path).unwrap_or_else(|error| panic!("read {path:?}: {error}")) {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            collect_rust_sources(&path, output);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            output.push_str(
                &fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {path:?}: {error}")),
            );
        }
    }
}

fn parser_source() -> String {
    let source_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut parser = fs::read_to_string(source_dir.join("parser.rs"))
        .expect("read parser.rs implementation source");
    collect_rust_sources(&source_dir.join("parser"), &mut parser);
    parser
}

fn contains_identifier(source: &str, identifier: &str) -> bool {
    source
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .any(|word| word == identifier)
}

fn contains_braced_constructor(source: &str, name: &str) -> bool {
    source.match_indices(name).any(|(index, _)| {
        let before = source[..index].chars().next_back();
        let after = &source[index + name.len()..];
        let has_identifier_boundary =
            before.is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_');
        has_identifier_boundary && after.trim_start().starts_with('{')
    })
}

fn contains_node_constructor(source: &str, name: &str) -> bool {
    source.contains(&format!("Node::{name}("))
        || source.contains(&format!("node!({name} {{"))
        || source.contains(&format!("node!({name}::"))
}

#[test]
fn completion_collection_handles_every_smoke_statement_token_boundary() {
    for case in CASES {
        let tokens = lex(case.sql)
            .unwrap_or_else(|error| panic!("failed to lex smoke case {:?}: {error}", case.sql));
        let mut points = tokens
            .iter()
            .flat_map(|token| [token.range.start(), token.range.end()])
            .collect::<Vec<_>>();
        points.sort_unstable();
        points.dedup();

        for point in points {
            collect_expectations(case.sql, point).unwrap_or_else(|error| {
                panic!(
                    "completion collection failed for {:?} at byte {}: {error}",
                    case.sql,
                    usize::from(point)
                )
            });
        }

        let complete = collect_expectations(
            case.sql,
            TextSize::try_from(case.sql.len()).expect("smoke SQL length fits TextSize"),
        )
        .unwrap_or_else(|error| {
            panic!(
                "completion collection failed for complete smoke case {:?}: {error}",
                case.sql
            )
        });
        assert!(
            !complete.tokens.contains(&pg_parser::TokenKind::Char(';')),
            "complete smoke case published the statement terminator for {:?}: {:?}",
            case.sql,
            complete.tokens
        );
        assert!(
            complete.slots.iter().all(|slot| matches!(
                slot,
                pg_parser::GrammarSlot::Alias | pg_parser::GrammarSlot::AnyName
            )),
            "complete smoke case published a stale object slot for {:?}: {:?}",
            case.sql,
            complete.slots
        );
    }
}

#[test]
fn completion_publishes_every_reserved_keyword_in_smoke_statements() {
    let reserved = KEYWORDS
        .iter()
        .filter(|keyword| keyword.category == KeywordCategory::Reserved)
        .map(|keyword| keyword.kind)
        .collect::<Vec<_>>();
    let mut missing = Vec::new();

    for case in CASES {
        let tokens = lex(case.sql)
            .unwrap_or_else(|error| panic!("failed to lex smoke case {:?}: {error}", case.sql));
        for token in tokens.iter().filter(|token| reserved.contains(&token.kind)) {
            let expectations = collect_expectations(case.sql, token.range.start())
                .unwrap_or_else(|error| panic!("completion failed for {:?}: {error}", case.sql));
            if !expectations.tokens.contains(&token.kind) {
                missing.push(format!(
                    "{:?} at byte {} in {:?}: {:?}",
                    token.kind,
                    usize::from(token.range.start()),
                    case.sql,
                    expectations
                ));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "reserved keyword completion gaps:\n{}",
        missing.join("\n")
    );
}

#[test]
fn completion_publishes_every_punctuation_token_in_smoke_statements() {
    let mut missing = Vec::new();

    for case in CASES {
        let tokens = lex(case.sql)
            .unwrap_or_else(|error| panic!("failed to lex smoke case {:?}: {error}", case.sql));
        for token in tokens.iter().filter(
            |token| matches!(token.kind, pg_parser::TokenKind::Char(character) if character != ';'),
        ) {
            let expectations = collect_expectations(case.sql, token.range.start())
                .unwrap_or_else(|error| panic!("completion failed for {:?}: {error}", case.sql));
            let operator_name = expectations
                .slots
                .contains(&pg_parser::GrammarSlot::Operator)
                && matches!(
                    token.kind,
                    pg_parser::TokenKind::Char(
                        '+' | '-'
                            | '*'
                            | '/'
                            | '%'
                            | '^'
                            | '<'
                            | '>'
                            | '='
                            | '~'
                            | '!'
                            | '@'
                            | '#'
                            | '&'
                            | '|'
                            | '?'
                            | '`'
                            | ':'
                    )
                );
            if !expectations.tokens.contains(&token.kind) && !operator_name {
                missing.push(format!(
                    "{:?} at byte {} in {:?}: {:?}",
                    token.kind,
                    usize::from(token.range.start()),
                    case.sql,
                    expectations
                ));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "punctuation completion gaps:\n{}",
        missing.join("\n")
    );
}

#[test]
fn every_ast_statement_has_a_node_variant() {
    let ast = include_str!("../../src/ast/mod.rs");
    let structs: BTreeSet<_> = ast
        .lines()
        .filter_map(|line| line.strip_prefix("pub struct "))
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| name.ends_with("Stmt"))
        .map(str::to_owned)
        .collect();
    let variants = names_between(ast, "pub enum Node {", "pub struct Alias");

    assert_eq!(structs, variants, "Stmt structs and Node variants drifted");
}

#[test]
fn raw_statement_constructors_are_audited() {
    let ast = include_str!("../../src/ast/mod.rs");
    let parser = parser_source();
    let structs: BTreeSet<_> = ast
        .lines()
        .filter_map(|line| line.strip_prefix("pub struct "))
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| name.ends_with("Stmt"))
        .map(str::to_owned)
        .collect();
    let constructors: BTreeSet<_> = structs
        .iter()
        .filter(|name| contains_node_constructor(&parser, name))
        .cloned()
        .collect();
    let missing: BTreeSet<_> = structs.difference(&constructors).cloned().collect();
    let expected_non_raw_or_wrapper =
        BTreeSet::from(["RawStmt".to_owned(), "SetOperationStmt".to_owned()]);

    assert_eq!(
        missing, expected_non_raw_or_wrapper,
        "new raw Stmt nodes require a parser constructor and statement-organized tests"
    );
}

#[test]
fn every_ast_struct_is_parsed_or_explicitly_classified_as_non_parser_output() {
    let ast = include_str!("../../src/ast/mod.rs");
    let parser = parser_source();
    let structs: BTreeSet<_> = ast
        .lines()
        .filter_map(|line| line.strip_prefix("pub struct "))
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_owned)
        .collect();
    let represented: BTreeSet<_> = structs
        .iter()
        .filter(|name| {
            contains_node_constructor(&parser, name)
                || contains_braced_constructor(&parser, name)
                || parser.contains(&format!("{name}::new("))
        })
        .cloned()
        .collect();
    let missing: BTreeSet<_> = structs.difference(&represented).cloned().collect();
    let expected_non_parser_output_nodes = BTreeSet::from([
        "Aggref".to_owned(),
        "AlternativeSubPlan".to_owned(),
        "ArrayCoerceExpr".to_owned(),
        "ArrayExpr".to_owned(),
        "CallContext".to_owned(),
        "CaseTestExpr".to_owned(),
        "CoerceToDomain".to_owned(),
        "CoerceToDomainValue".to_owned(),
        "CoerceViaIo".to_owned(),
        "CollateExpr".to_owned(),
        "Const".to_owned(),
        "ConvertRowtypeExpr".to_owned(),
        // PostgreSQL expression-base marker; concrete expressions use Node variants.
        "Expr".to_owned(),
        "FieldSelect".to_owned(),
        "FieldStore".to_owned(),
        "ForPortionOfExpr".to_owned(),
        "FromExpr".to_owned(),
        "FuncExpr".to_owned(),
        "GraphLabelRef".to_owned(),
        "GraphPropertyRef".to_owned(),
        "InferenceElem".to_owned(),
        "InlineCodeBlock".to_owned(),
        "JsonConstructorExpr".to_owned(),
        "JsonExpr".to_owned(),
        "JsonTablePath".to_owned(),
        "JsonTablePathScan".to_owned(),
        "JsonTablePlan".to_owned(),
        "JsonTableSiblingJoin".to_owned(),
        "MergeAction".to_owned(),
        "NextValueExpr".to_owned(),
        "OnConflictExpr".to_owned(),
        "OpExpr".to_owned(),
        "Param".to_owned(),
        "PartitionRangeDatum".to_owned(),
        // A transient gram.y helper consumed by preprocess_pub_all_objtype_list.
        "PublicationAllObjSpec".to_owned(),
        "Query".to_owned(),
        "RangeTblEntry".to_owned(),
        "RangeTblFunction".to_owned(),
        "RangeTblRef".to_owned(),
        "RelabelType".to_owned(),
        "ReturningExpr".to_owned(),
        "RowCompareExpr".to_owned(),
        "RowMarkClause".to_owned(),
        "RtePermissionInfo".to_owned(),
        "ScalarArrayOpExpr".to_owned(),
        "SetOperationStmt".to_owned(),
        "SortGroupClause".to_owned(),
        "SubPlan".to_owned(),
        "SubscriptingRef".to_owned(),
        "TableFunc".to_owned(),
        "TableSampleClause".to_owned(),
        "TargetEntry".to_owned(),
        "Var".to_owned(),
        "WindowClause".to_owned(),
        "WindowFunc".to_owned(),
        "WindowFuncRunCondition".to_owned(),
        "WithCheckOption".to_owned(),
    ]);
    assert_eq!(
        missing, expected_non_parser_output_nodes,
        "AST structs require parser representation or an explicit analysis/planner/executor/transient classification"
    );
}

#[test]
fn every_parser_statement_constructor_has_statement_organized_coverage() {
    let ast = include_str!("../../src/ast/mod.rs");
    let parser = parser_source();
    let structs: BTreeSet<_> = ast
        .lines()
        .filter_map(|line| line.strip_prefix("pub struct "))
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| name.ends_with("Stmt"))
        .map(str::to_owned)
        .collect();
    let constructors: BTreeSet<_> = structs
        .iter()
        .filter(|name| contains_node_constructor(&parser, name))
        .cloned()
        .collect();
    let mut covered: BTreeSet<_> = CASES
        .iter()
        .map(|case| case.expected_name.to_owned())
        .collect();
    // These are grammar-produced nested statement nodes, not top-level statements.
    covered.insert("ReplicaIdentityStmt".to_owned());
    covered.insert("ReturnStmt".to_owned());
    covered.insert("PlAssignStmt".to_owned());

    assert_eq!(
        constructors, covered,
        "each parser-produced Stmt requires a smoke case or a nested-node test"
    );
}

#[test]
fn every_raw_statement_type_appears_in_non_smoke_statement_tests() {
    let ast = include_str!("../../src/ast/mod.rs");
    let test_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/statements");
    let mut tests = String::new();
    for entry in fs::read_dir(&test_dir).expect("statement test directory") {
        let path = entry.expect("statement test entry").path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("rs")
            && !matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some("coverage.rs" | "smoke.rs")
            )
        {
            tests.push_str(&fs::read_to_string(&path).expect("statement test source"));
        }
    }

    let analysis_only_statements = BTreeSet::from(["SetOperationStmt"]);
    let missing = ast
        .lines()
        .filter_map(|line| line.strip_prefix("pub struct "))
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| name.ends_with("Stmt") && !analysis_only_statements.contains(name))
        .filter(|name| !contains_identifier(&tests, name))
        .collect::<Vec<_>>();

    assert!(
        missing.is_empty(),
        "raw Stmt types require non-smoke statement-organized semantic tests: {missing:?}"
    );
}

#[test]
fn every_raw_statement_field_is_exercised_by_statement_tests() {
    let ast = include_str!("../../src/ast/mod.rs");
    let test_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/statements");
    let mut test_blocks = Vec::new();
    for entry in fs::read_dir(&test_dir).expect("statement test directory") {
        let path = entry.expect("statement test entry").path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("rs")
            && !matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some("coverage.rs" | "smoke.rs")
            )
        {
            let source = fs::read_to_string(&path).expect("statement test source");
            test_blocks.extend(source.split("#[test]").skip(1).map(str::to_owned));
        }
    }

    let analysis_only_statements = BTreeSet::from(["SetOperationStmt"]);
    let analysis_only_fields = BTreeSet::from([
        // Filled by sequence ownership processing, not the raw grammar.
        "CreateSeqStmt.owner_id".to_owned(),
        // Constraint transformation metadata; CREATE INDEX grammar leaves these false.
        "IndexStmt.deferrable".to_owned(),
        "IndexStmt.initdeferred".to_owned(),
        // Present in the shared AST type but no ALTER PROPERTY GRAPH production sets it.
        "AlterPropGraphStmt.missing_ok".to_owned(),
    ]);
    let mut missing = BTreeSet::new();
    for section in ast.split("pub struct ").skip(1) {
        let Some((name, body)) = section.split_once(" {") else {
            continue;
        };
        if !name.ends_with("Stmt") || analysis_only_statements.contains(name) {
            continue;
        }
        let body = if body.starts_with('}') {
            ""
        } else {
            body.split_once("\n}").expect("struct body").0
        };
        let related_tests = test_blocks
            .iter()
            .filter(|source| contains_identifier(source, name))
            .collect::<Vec<_>>();
        for field in body.lines().filter_map(|line| {
            line.trim()
                .strip_prefix("pub ")?
                .split_once(':')
                .map(|(field, _)| field.trim())
        }) {
            if !related_tests
                .iter()
                .any(|source| contains_identifier(source, field))
            {
                missing.insert(format!("{name}.{field}"));
            }
        }
    }

    assert_eq!(
        missing, analysis_only_fields,
        "raw Stmt fields require a related statement test or explicit non-raw classification"
    );
}

#[test]
fn every_parser_produced_nested_node_has_explicit_test_coverage() {
    let parser = parser_source();
    let constructors: BTreeSet<_> = parser
        .split("Node::")
        .skip(1)
        .filter_map(|tail| {
            let name = tail
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                .collect::<String>();
            tail[name.len()..].starts_with('(').then_some(name)
        })
        .filter(|name| !name.ends_with("Stmt"))
        .collect();

    let mut tests = String::new();
    collect_rust_sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/statements"),
        &mut tests,
    );
    let covered: BTreeSet<_> = constructors
        .iter()
        .filter(|name| tests.contains(&format!("Node::{name}")))
        .cloned()
        .collect();
    let missing: BTreeSet<_> = constructors.difference(&covered).cloned().collect();
    assert!(
        missing.is_empty(),
        "parser-produced nested nodes require explicit statement-organized tests: {missing:?}"
    );
}

#[test]
fn every_nested_raw_field_is_exercised_or_analysis_only() {
    let ast = include_str!("../../src/ast/mod.rs");
    let parser = parser_source();
    let test_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/statements");
    let mut tests = String::new();
    for entry in fs::read_dir(&test_dir).expect("statement test directory") {
        let path = entry.expect("statement test entry").path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("rs")
            && path.file_name().and_then(|name| name.to_str()) != Some("coverage.rs")
        {
            tests.push_str(&fs::read_to_string(&path).expect("statement test source"));
        }
    }

    let analysis_only = [
        "CaseExpr.casecollid",
        "CaseExpr.casetype",
        "CoalesceExpr.coalescecollid",
        "CoalesceExpr.coalescetype",
        "CommonTableExpr.ctecolcollations",
        "CommonTableExpr.ctecolnames",
        "CommonTableExpr.ctecoltypmods",
        "CommonTableExpr.ctecoltypes",
        "CommonTableExpr.cterecursive",
        "CommonTableExpr.cterefcount",
        "Constraint.cooked_expr",
        "Constraint.old_conpfeqop",
        "Constraint.old_pktable_oid",
        "CteCycleClause.cycle_mark_collation",
        "CteCycleClause.cycle_mark_neop",
        "CteCycleClause.cycle_mark_type",
        "CteCycleClause.cycle_mark_typmod",
        "GroupingFunc.agglevelsup",
        "GroupingFunc.refs",
        "IndexElem.indexcolname",
        "IntoClause.view_query",
        "JsonBehavior.coerce",
        "JsonIsPredicate.expr_base_type",
        "JsonReturning.typid",
        "JsonValueExpr.formatted_expr",
        "MinMaxExpr.inputcollid",
        "MinMaxExpr.minmaxcollid",
        "MinMaxExpr.minmaxtype",
        "RowExpr.row_typeid",
        "SetToDefault.type_id",
        "SetToDefault.type_mod",
        "TypeName.typemod",
        "VacuumRelation.oid",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<BTreeSet<_>>();

    let mut missing = BTreeSet::new();
    for section in ast.split("pub struct ").skip(1) {
        let Some((name, body)) = section.split_once(" {") else {
            continue;
        };
        if name.ends_with("Stmt")
            || !(contains_node_constructor(&parser, name)
                || contains_braced_constructor(&parser, name))
        {
            continue;
        }
        let body = if body.starts_with('}') {
            ""
        } else {
            body.split_once("\n}").expect("struct body").0
        };
        for field in body.lines().filter_map(|line| {
            line.trim()
                .strip_prefix("pub ")?
                .split_once(':')
                .map(|(field, _)| field.trim())
        }) {
            if !contains_identifier(&tests, field) {
                missing.insert(format!("{name}.{field}"));
            }
        }
    }

    assert_eq!(
        missing, analysis_only,
        "nested syntax fields require statement tests or an explicit analysis-only classification"
    );
}

#[test]
fn every_directly_constructed_ast_struct_has_explicit_test_coverage() {
    let ast = include_str!("../../src/ast/mod.rs");
    let parser = parser_source();
    let structs: BTreeSet<_> = ast
        .lines()
        .filter_map(|line| line.strip_prefix("pub struct "))
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_owned)
        .collect();
    let constructors: BTreeSet<_> = structs
        .into_iter()
        .filter(|name| contains_braced_constructor(&parser, name))
        .collect();

    let mut tests = String::new();
    collect_rust_sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/statements"),
        &mut tests,
    );
    let missing: BTreeSet<_> = constructors
        .into_iter()
        .filter(|name| !contains_identifier(&tests, name))
        .collect();
    assert!(
        missing.is_empty(),
        "directly constructed AST structs require explicit statement-organized field tests: {missing:?}"
    );
}
