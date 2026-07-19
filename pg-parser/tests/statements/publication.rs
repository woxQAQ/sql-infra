use pg_parser::{AlterPublicationAction, Node, PublicationObjSpecType};

use super::common::{assert_parse_errors, expect_node, parse_node};

#[test]
fn create_publication_stmt_populates_tables_columns_filters_and_schemas() {
    let sql = "create publication item_changes for table app.items (id, name) where (active = true), tables in schema public with (publish = 'insert, update')";
    let stmt = parse_node!(sql, CreatePublicationStmt);
    assert_eq!(stmt.pubname.as_deref(), Some("item_changes"));
    assert_eq!(stmt.pubobjects.len(), 2);
    assert_eq!(stmt.options.len(), 1);

    let table_spec = expect_node!(&stmt.pubobjects[0], PublicationObjSpec);
    assert_eq!(table_spec.pubobjtype, PublicationObjSpecType::Table);
    assert_eq!(table_spec.location, 0);
    let table = table_spec.pubtable.as_ref().expect("PublicationTable");
    assert_eq!(table.columns.len(), 2);
    assert!(table.where_clause.is_some());

    let schema_spec = expect_node!(&stmt.pubobjects[1], PublicationObjSpec);
    assert_eq!(
        schema_spec.pubobjtype,
        PublicationObjSpecType::TablesInSchema
    );
    assert_eq!(schema_spec.name.as_deref(), Some("public"));
    assert_eq!(
        schema_spec.location as usize,
        sql.find("public with").unwrap()
    );

    let quoted = parse_node!(
        "create publication \"select\" for tables in schema \"from\"",
        CreatePublicationStmt
    );
    assert_eq!(quoted.pubname.as_deref(), Some("select"));
    assert!(matches!(
        quoted.pubobjects.as_slice(),
        [Node::PublicationObjSpec(schema)] if schema.name.as_deref() == Some("from")
    ));

    let inheritance = parse_node!(
        "create publication inheritance_changes for table only (app.parents), app.children *",
        CreatePublicationStmt
    );
    assert!(matches!(
        inheritance.pubobjects.as_slice(),
        [Node::PublicationObjSpec(parent), Node::PublicationObjSpec(children)]
            if matches!(parent.pubtable.as_deref().and_then(|table| table.relation.as_deref()), Some(relation) if !relation.inh && relation.alias.is_none())
                && matches!(children.pubtable.as_deref().and_then(|table| table.relation.as_deref()), Some(relation) if relation.inh && relation.alias.is_none())
    ));
}

#[test]
fn create_publication_stmt_populates_all_objects_and_exceptions() {
    let sql = "create publication everything for all tables except (table only (audit.log), archive.items *), all sequences";
    let stmt = parse_node!(sql, CreatePublicationStmt);
    assert!(stmt.for_all_tables);
    assert!(stmt.for_all_sequences);
    assert_eq!(stmt.pubobjects.len(), 2);
    let exception = expect_node!(&stmt.pubobjects[0], PublicationObjSpec);
    assert_eq!(exception.pubobjtype, PublicationObjSpecType::ExceptTable);
    assert_eq!(
        exception.location as usize,
        sql.find("only (audit.log)").unwrap()
    );
    assert!(
        exception
            .pubtable
            .as_ref()
            .is_some_and(|table| table.except)
    );
    assert!(matches!(
        exception
            .pubtable
            .as_deref()
            .and_then(|table| table.relation.as_deref()),
        Some(relation) if !relation.inh
    ));
    let inherited_exception = expect_node!(&stmt.pubobjects[1], PublicationObjSpec);
    assert_eq!(
        inherited_exception.location as usize,
        sql.find("archive.items").unwrap()
    );
    assert!(matches!(
        inherited_exception
            .pubtable
            .as_deref()
            .and_then(|table| table.relation.as_deref()),
        Some(relation) if relation.inh
    ));

    let schemas_sql =
        "create publication schema_changes for tables in schema public, audit, current_schema";
    let schemas = parse_node!(schemas_sql, CreatePublicationStmt);
    assert!(matches!(
        schemas.pubobjects.as_slice(),
        [
            Node::PublicationObjSpec(public),
            Node::PublicationObjSpec(audit),
            Node::PublicationObjSpec(current),
        ] if public.pubobjtype == PublicationObjSpecType::TablesInSchema
            && public.name.as_deref() == Some("public")
            && audit.pubobjtype == PublicationObjSpecType::TablesInSchema
            && audit.name.as_deref() == Some("audit")
            && current.pubobjtype == PublicationObjSpecType::TablesInCurSchema
            && current.name.is_none()
    ));
    let [
        Node::PublicationObjSpec(public),
        Node::PublicationObjSpec(audit),
        Node::PublicationObjSpec(current),
    ] = schemas.pubobjects.as_slice()
    else {
        panic!("expected three schema PublicationObjSpec nodes");
    };
    assert_eq!(
        public.location as usize,
        schemas_sql.rfind("public").unwrap()
    );
    assert_eq!(audit.location as usize, schemas_sql.find("audit").unwrap());
    assert_eq!(
        current.location as usize,
        schemas_sql.find("current_schema").unwrap()
    );
}

#[test]
fn alter_publication_stmt_populates_actions_objects_and_options() {
    let add = parse_node!(
        "alter publication item_changes add table app.other",
        AlterPublicationStmt
    );
    assert_eq!(add.pubname.as_deref(), Some("item_changes"));
    assert_eq!(add.action, AlterPublicationAction::AddObjects);
    assert_eq!(add.pubobjects.len(), 1);

    let set = parse_node!(
        "alter publication item_changes set (publish = 'insert')",
        AlterPublicationStmt
    );
    assert_eq!(set.action, AlterPublicationAction::SetObjects);
    assert_eq!(set.options.len(), 1);

    let all = parse_node!(
        "alter publication item_changes set all tables except (table audit.log), all sequences",
        AlterPublicationStmt
    );
    assert_eq!(all.action, AlterPublicationAction::SetObjects);
    assert!(all.for_all_tables);
    assert!(all.for_all_sequences);
    assert!(matches!(
        all.pubobjects.as_slice(),
        [Node::PublicationObjSpec(exception)]
            if exception.pubobjtype == PublicationObjSpecType::ExceptTable
    ));
}

#[test]
fn publication_object_lists_reject_duplicates_mixing_and_missing_prefixes() {
    assert_parse_errors(&[
        "create publication p for all tables, all tables",
        "create publication p for all sequences, all sequences",
        "create publication p for all tables, table app.items",
        "create publication p for table app.items, all sequences",
        "create publication p for table app.items,",
        "create publication p for tables in schema public,",
        "create publication p for all tables except (table app.items,)",
        "create publication p for app.items",
        "create publication p for tables in schema public, audit(id)",
        "create publication p for tables in schema public, audit where (true)",
        "create publication p for table app.items item_alias",
        "create publication p for all tables except (app.items item_alias)",
        "alter publication p add all tables",
        "alter publication p drop all sequences",
        "alter publication p add (publish = 'insert')",
        "alter publication p drop (publish = 'insert')",
        "alter publication p add table app.items with (publish = 'insert')",
        "alter publication p set table app.items with (publish = 'insert')",
        "alter publication p set table app.items,",
        "alter publication p add table app.items, current_schema",
    ]);
}
