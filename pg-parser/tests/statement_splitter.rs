struct SplitCase {
    name: &'static str,
    sql: &'static str,
    syntax: &'static [&'static str],
    full: &'static [&'static str],
}

#[test]
fn splits_statement_locs() {
    let cases = [
        SplitCase {
            name: "empty input",
            sql: "",
            syntax: &[],
            full: &[],
        },
        SplitCase {
            name: "trivia and empty statements",
            sql: "  -- comment;\n /* outer; /* inner; */ end */ ; ; \t",
            syntax: &[],
            full: &[],
        },
        SplitCase {
            name: "terminated and unterminated statements",
            sql: "  select 1; \n select 2  ",
            syntax: &["select 1", "select 2  "],
            full: &["select 1;", "select 2  "],
        },
        SplitCase {
            name: "standard and national strings",
            sql: "select 'a;''b', N'n;''value'; select 2;",
            syntax: &["select 'a;''b', N'n;''value'", "select 2"],
            full: &["select 'a;''b', N'n;''value';", "select 2;"],
        },
        SplitCase {
            name: "empty escape string and doubled quote",
            sql: "select E'', E'one'';two'; select 2;",
            syntax: &["select E'', E'one'';two'", "select 2"],
            full: &["select E'', E'one'';two';", "select 2;"],
        },
        SplitCase {
            name: "escape string with escaped quote",
            sql: r#"select E'before \'; after'; select 2;"#,
            syntax: &[r#"select E'before \'; after'"#, "select 2"],
            full: &[r#"select E'before \'; after';"#, "select 2;"],
        },
        SplitCase {
            name: "lowercase escape string with escaped backslash",
            sql: r#"select e'before \\; after'; select 2;"#,
            syntax: &[r#"select e'before \\; after'"#, "select 2"],
            full: &[r#"select e'before \\; after';"#, "select 2;"],
        },
        SplitCase {
            name: "escape string with C-style control escapes",
            sql: r#"select E'\a\b\f\n\r\t\v;'; select 2;"#,
            syntax: &[r#"select E'\a\b\f\n\r\t\v;'"#, "select 2"],
            full: &[r#"select E'\a\b\f\n\r\t\v;';"#, "select 2;"],
        },
        SplitCase {
            name: "escape string with numeric and Unicode escapes",
            sql: r#"select E'\101\x41\u0041\U00000041;'; select 2;"#,
            syntax: &[r#"select E'\101\x41\u0041\U00000041;'"#, "select 2"],
            full: &[r#"select E'\101\x41\u0041\U00000041;';"#, "select 2;"],
        },
        SplitCase {
            name: "escape string with escaped physical newline",
            sql: r#"select E'first\
second;third'; select 2;"#,
            syntax: &[
                r#"select E'first\
second;third'"#,
                "select 2",
            ],
            full: &[
                r#"select E'first\
second;third';"#,
                "select 2;",
            ],
        },
        SplitCase {
            name: "escape string continued across adjacent literals",
            sql: "select E'first'\n'\\';second', 'third' -- continuation\n';fourth'; select 2;",
            syntax: &[
                "select E'first'\n'\\';second', 'third' -- continuation\n';fourth'",
                "select 2",
            ],
            full: &[
                "select E'first'\n'\\';second', 'third' -- continuation\n';fourth';",
                "select 2;",
            ],
        },
        SplitCase {
            name: "bit and hexadecimal strings",
            sql: "select B'10;01', b'0;1', X'AB;CD', x'0;F'; select 2;",
            syntax: &["select B'10;01', b'0;1', X'AB;CD', x'0;F'", "select 2"],
            full: &["select B'10;01', b'0;1', X'AB;CD', x'0;F';", "select 2;"],
        },
        SplitCase {
            name: "unicode strings and quoted identifiers",
            sql: r#"select U&'d\0061;ta', U&"a;b", "c;""d"; select 2;"#,
            syntax: &[r#"select U&'d\0061;ta', U&"a;b", "c;""d""#, "select 2"],
            full: &[r#"select U&'d\0061;ta', U&"a;b", "c;""d";"#, "select 2;"],
        },
        SplitCase {
            name: "dollar-quoted strings",
            sql: "select $$body; $tag$ still body; $$, $tag_1$begin; end$tag_1$; select 2;",
            syntax: &[
                "select $$body; $tag$ still body; $$, $tag_1$begin; end$tag_1$",
                "select 2",
            ],
            full: &[
                "select $$body; $tag$ still body; $$, $tag_1$begin; end$tag_1$;",
                "select 2;",
            ],
        },
        SplitCase {
            name: "line and nested block comments",
            sql: "select 1 -- ;\n + 2 /* outer; /* inner; */ end */ ; select 3;",
            syntax: &[
                "select 1 -- ;\n + 2 /* outer; /* inner; */ end */ ",
                "select 3",
            ],
            full: &[
                "select 1 -- ;\n + 2 /* outer; /* inner; */ end */ ;",
                "select 3;",
            ],
        },
        SplitCase {
            name: "parentheses and brackets",
            sql: "invalid grammar (a; func(b; c))[x; y]; select 2;",
            syntax: &["invalid grammar (a; func(b; c))[x; y]", "select 2"],
            full: &["invalid grammar (a; func(b; c))[x; y];", "select 2;"],
        },
        SplitCase {
            name: "positional parameters",
            sql: "select $1, $22; select $3;",
            syntax: &["select $1, $22", "select $3"],
            full: &["select $1, $22;", "select $3;"],
        },
        SplitCase {
            name: "begin atomic body with case expression",
            sql: "create function f() returns int language sql begin /* trivia */ atomic select case when true then 1 else 2 end; return 1; end; select 2;",
            syntax: &[
                "create function f() returns int language sql begin /* trivia */ atomic select case when true then 1 else 2 end; return 1; end",
                "select 2",
            ],
            full: &[
                "create function f() returns int language sql begin /* trivia */ atomic select case when true then 1 else 2 end; return 1; end;",
                "select 2;",
            ],
        },
        SplitCase {
            name: "nested begin atomic bodies",
            sql: "begin atomic begin -- separator\n atomic select 1; end; select 2; end; select 3;",
            syntax: &[
                "begin atomic begin -- separator\n atomic select 1; end; select 2; end",
                "select 3",
            ],
            full: &[
                "begin atomic begin -- separator\n atomic select 1; end; select 2; end;",
                "select 3;",
            ],
        },
        SplitCase {
            name: "ordinary begin statement",
            sql: "begin transaction; select 1; commit;",
            syntax: &["begin transaction", "select 1", "commit"],
            full: &["begin transaction;", "select 1;", "commit;"],
        },
        SplitCase {
            name: "quoted begin atomic words",
            sql: "select 'begin atomic; end', $$begin atomic; end$$, \"begin;atomic\"; select 2;",
            syntax: &[
                "select 'begin atomic; end', $$begin atomic; end$$, \"begin;atomic\"",
                "select 2",
            ],
            full: &[
                "select 'begin atomic; end', $$begin atomic; end$$, \"begin;atomic\";",
                "select 2;",
            ],
        },
        SplitCase {
            name: "UTF-8 text",
            sql: "选择 中文; select '数据;值';",
            syntax: &["选择 中文", "select '数据;值'"],
            full: &["选择 中文;", "select '数据;值';"],
        },
    ];

    for case in cases {
        let locs = pg_parser::split_statement_locs(case.sql)
            .unwrap_or_else(|error| panic!("{}: {error:?}", case.name));
        let syntax = locs
            .iter()
            .map(|loc| slice(case.sql, loc.syntax))
            .collect::<Vec<_>>();
        let full = locs
            .iter()
            .map(|loc| slice(case.sql, loc.full()))
            .collect::<Vec<_>>();

        assert_eq!(syntax, case.syntax, "{}: syntax locs", case.name);
        assert_eq!(full, case.full, "{}: full locs", case.name);
    }
}

struct ErrorCase {
    name: &'static str,
    sql: &'static str,
    message: &'static str,
    range_text: &'static str,
}

#[test]
fn reports_lexical_errors() {
    let cases = [
        ErrorCase {
            name: "unterminated standard string",
            sql: "select 'unterminated",
            message: "unterminated quoted string",
            range_text: "'unterminated",
        },
        ErrorCase {
            name: "unterminated escape string after dangling backslash",
            sql: "select E'unterminated\\",
            message: "unterminated quoted string",
            range_text: "E'unterminated\\",
        },
        ErrorCase {
            name: "unterminated escape string after escaped quote",
            sql: r#"select E'unterminated\'"#,
            message: "unterminated quoted string",
            range_text: r#"E'unterminated\'"#,
        },
        ErrorCase {
            name: "unterminated national string",
            sql: "select N'unterminated",
            message: "unterminated quoted string",
            range_text: "N'unterminated",
        },
        ErrorCase {
            name: "unterminated bit string",
            sql: "select B'0101",
            message: "unterminated bit string literal",
            range_text: "B'0101",
        },
        ErrorCase {
            name: "unterminated hexadecimal string",
            sql: "select X'CAFE",
            message: "unterminated hexadecimal string literal",
            range_text: "X'CAFE",
        },
        ErrorCase {
            name: "unterminated Unicode string",
            sql: "select U&'data",
            message: "unterminated quoted string",
            range_text: "U&'data",
        },
        ErrorCase {
            name: "unterminated quoted identifier",
            sql: "select \"identifier",
            message: "unterminated quoted identifier",
            range_text: "\"identifier",
        },
        ErrorCase {
            name: "unterminated Unicode quoted identifier",
            sql: "select U&\"identifier",
            message: "unterminated quoted identifier",
            range_text: "U&\"identifier",
        },
        ErrorCase {
            name: "zero-length quoted identifier",
            sql: "select \"\"",
            message: "zero-length delimited identifier",
            range_text: "\"\"",
        },
        ErrorCase {
            name: "unterminated untagged dollar string",
            sql: "select $$unterminated",
            message: "unterminated dollar-quoted string",
            range_text: "$$unterminated",
        },
        ErrorCase {
            name: "unterminated tagged dollar string",
            sql: "select $body$unterminated $other$",
            message: "unterminated dollar-quoted string",
            range_text: "$body$unterminated $other$",
        },
        ErrorCase {
            name: "unterminated nested block comment",
            sql: "select 1 /* outer /* inner */",
            message: "unterminated /* comment",
            range_text: "/* outer /* inner */",
        },
        ErrorCase {
            name: "trailing junk after positional parameter",
            sql: "select $12abc",
            message: "trailing junk after parameter",
            range_text: "$12",
        },
    ];

    for case in cases {
        let error = match pg_parser::split_statement_locs(case.sql) {
            Ok(locs) => panic!("{}: expected an error, got {locs:?}", case.name),
            Err(error) => error,
        };
        assert_eq!(error.message, case.message, "{}: error message", case.name);
        assert_eq!(
            slice(case.sql, error.loc),
            case.range_text,
            "{}: error loc",
            case.name
        );
    }
}

fn slice(sql: &str, loc: pg_parser::Loc) -> &str {
    &sql[usize::from(loc.start())..usize::from(loc.end())]
}
