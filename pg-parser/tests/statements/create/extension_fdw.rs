use super::*;

#[test]
fn create_extension_language_and_subscription_follow_raw_grammar_nodes() {
    let extension = parse_node!(
        "create extension if not exists postgis with schema extensions version '3.5' cascade",
        CreateExtensionStmt
    );
    assert_eq!(extension.extname.as_deref(), Some("postgis"));
    assert!(extension.if_not_exists);
    assert_eq!(extension.options.len(), 3);
    let schema_option = expect_node!(&extension.options[0], DefElem);
    assert!(schema_option.defnamespace.is_none());

    let language_extension = parse_node!("create or replace language plpgsql", CreateExtensionStmt);
    assert_eq!(language_extension.extname.as_deref(), Some("plpgsql"));
    assert!(language_extension.if_not_exists);

    let modified_language_extension = parse_node!(
        "create or replace trusted procedural language plpgsql",
        CreateExtensionStmt
    );
    assert_eq!(
        modified_language_extension.extname.as_deref(),
        Some("plpgsql")
    );
    assert!(modified_language_extension.if_not_exists);

    let language = parse_node!(
        "create trusted language plsample handler app.plsample_handler inline app.plsample_inline validator app.plsample_validator",
        CreatePLangStmt
    );
    assert!(language.pltrusted);
    assert_eq!(language.plname.as_deref(), Some("plsample"));
    assert_eq!(language.plhandler.len(), 2);
    assert_eq!(language.plinline.len(), 2);
    assert_eq!(language.plvalidator.len(), 2);

    let no_validator = parse_node!(
        "create or replace trusted procedural language plsample handler app.plsample_handler no validator",
        CreatePLangStmt
    );
    assert!(no_validator.replace);
    assert!(no_validator.pltrusted);
    assert!(no_validator.plinline.is_empty());
    assert!(no_validator.plvalidator.is_empty());

    let connection = parse_node!(
        "create subscription item_sub connection 'host=db.example dbname=app' publication item_pub, audit_pub with (enabled = true)",
        CreateSubscriptionStmt
    );
    assert_eq!(connection.subname.as_deref(), Some("item_sub"));
    assert!(connection.conninfo.is_some());
    assert!(connection.servername.is_none());
    assert_eq!(connection.publication.len(), 2);
    assert_eq!(connection.options.len(), 1);

    let server = parse_node!(
        "create subscription item_server_sub server logical_srv publication item_pub",
        CreateSubscriptionStmt
    );
    assert_eq!(server.servername.as_deref(), Some("logical_srv"));
    assert!(server.conninfo.is_none());
}

#[test]
fn create_fdw_cast_conversion_and_transform_populate_all_fields() {
    let fdw = parse_node!(
        "create foreign data wrapper app_fdw handler app.fdw_handler validator app.fdw_validator no connection options (host 'db.example', fetch_size '1000')",
        CreateFdwStmt
    );
    assert_eq!(fdw.fdwname.as_deref(), Some("app_fdw"));
    assert_eq!(fdw.func_options.len(), 3);
    assert_eq!(fdw.options.len(), 2);

    let cast = parse_node!(
        "create cast (app.source_value as app.target_value) with function app.cast_value(app.source_value) as assignment",
        CreateCastStmt
    );
    assert!(cast.sourcetype.is_some());
    assert!(cast.targettype.is_some());
    assert!(cast.func.is_some());
    assert_eq!(cast.context, pg_parser::CoercionContext::Assignment);
    assert!(!cast.inout);

    let unspecified_cast = parse_node!(
        "create cast (app.source_value as app.target_value) with function app.cast_value as implicit",
        CreateCastStmt
    );
    assert!(
        unspecified_cast
            .func
            .as_deref()
            .expect("cast function")
            .args_unspecified
    );

    let inout = parse_node!(
        "create cast (json as jsonb) with inout as implicit",
        CreateCastStmt
    );
    assert!(inout.inout);
    assert_eq!(inout.context, pg_parser::CoercionContext::Implicit);

    let without_function = parse_node!(
        "create cast (app.binary_value as bytea) without function",
        CreateCastStmt
    );
    assert!(without_function.func.is_none());
    assert!(!without_function.inout);
    assert_eq!(
        without_function.context,
        pg_parser::CoercionContext::Explicit
    );

    let conversion = parse_node!(
        "create default conversion app.utf8_to_latin for 'UTF8' to 'LATIN1' from app.convert_encoding",
        CreateConversionStmt
    );
    assert!(conversion.def);
    assert_eq!(conversion.conversion_name.len(), 2);
    assert_eq!(conversion.for_encoding_name.as_deref(), Some("UTF8"));
    assert_eq!(conversion.to_encoding_name.as_deref(), Some("LATIN1"));
    assert_eq!(conversion.func_name.len(), 2);

    let transform = parse_node!(
        "create or replace transform for app.custom_type language plpgsql (from sql with function app.from_sql(app.custom_type), to sql with function app.to_sql(app.custom_type))",
        CreateTransformStmt
    );
    assert!(transform.replace);
    assert!(transform.type_name.is_some());
    assert_eq!(transform.lang.as_deref(), Some("plpgsql"));
    assert!(transform.fromsql.is_some());
    assert!(transform.tosql.is_some());

    let unspecified_transform = parse_node!(
        "create transform for app.unspecified_type language sql
         (from sql with function app.from_sql)",
        CreateTransformStmt
    );
    assert!(
        unspecified_transform
            .fromsql
            .as_deref()
            .expect("FROM SQL function")
            .args_unspecified
    );

    let from_only = parse_node!(
        "create transform for app.from_only language sql (from sql with function app.from_sql(app.from_only))",
        CreateTransformStmt
    );
    assert!(from_only.fromsql.is_some());
    assert!(from_only.tosql.is_none());

    let to_only = parse_node!(
        "create transform for app.to_only language sql (to sql with function app.to_sql(app.to_only))",
        CreateTransformStmt
    );
    assert!(to_only.fromsql.is_none());
    assert!(to_only.tosql.is_some());

    let reverse_order = parse_node!(
        "create transform for app.reverse_type language sql (to sql with function app.to_sql(app.reverse_type), from sql with function app.from_sql(app.reverse_type))",
        CreateTransformStmt
    );
    assert!(reverse_order.fromsql.is_some());
    assert!(reverse_order.tosql.is_some());
}

#[test]
fn create_foreign_server_mapping_tablespace_and_access_method_are_strict() {
    let table = parse_node!(
        "create foreign table if not exists app.remote_orders (
             id bigint options (column_name 'remote_id'),
             payload text
         ) server foreign_srv options (schema_name 'public', table_name 'orders')",
        CreateForeignTableStmt
    );
    assert!(table.base.if_not_exists);
    assert_eq!(table.base.table_elts.len(), 2);
    assert_eq!(
        table
            .base
            .relation
            .as_deref()
            .and_then(|relation| relation.schemaname.as_deref()),
        Some("app")
    );
    let id = expect_node!(&table.base.table_elts[0], ColumnDef);
    assert_eq!(id.fdwoptions.len(), 1);
    assert_eq!(table.servername.as_deref(), Some("foreign_srv"));
    assert_eq!(table.options.len(), 2);

    let server = parse_node!(
        "create server if not exists foreign_srv type 'postgres_fdw' version '16' foreign data wrapper postgres_fdw options (host 'db.example', port '5432')",
        CreateForeignServerStmt
    );
    assert_eq!(server.servername.as_deref(), Some("foreign_srv"));
    assert_eq!(server.servertype.as_deref(), Some("postgres_fdw"));
    assert_eq!(server.version.as_deref(), Some("16"));
    assert_eq!(server.fdwname.as_deref(), Some("postgres_fdw"));
    assert_eq!(server.options.len(), 2);

    let mapping = parse_node!(
        "create user mapping if not exists for current_user server foreign_srv options (user 'remote_user', password 'secret')",
        CreateUserMappingStmt
    );
    assert!(mapping.if_not_exists);
    assert!(mapping.user.is_some());
    assert_eq!(mapping.servername.as_deref(), Some("foreign_srv"));
    assert_eq!(mapping.options.len(), 2);

    let tablespace = parse_node!(
        "create tablespace fast_space owner app_owner location '/srv/postgres/fast' with (random_page_cost = 1.1, storage.provider = custom)",
        CreateTableSpaceStmt
    );
    assert_eq!(tablespace.tablespacename.as_deref(), Some("fast_space"));
    assert!(tablespace.owner.is_some());
    assert_eq!(tablespace.location.as_deref(), Some("/srv/postgres/fast"));
    assert_eq!(tablespace.options.len(), 2);
    assert!(matches!(
        tablespace.options.as_slice(),
        [Node::DefElem(cost), Node::DefElem(provider)]
            if cost.defnamespace.is_none()
                && cost.defname.as_deref() == Some("random_page_cost")
                && matches!(cost.arg.as_deref(), Some(Node::Float(_)))
                && provider.defnamespace.as_deref() == Some("storage")
                && provider.defname.as_deref() == Some("provider")
    ));

    let access_method = parse_node!(
        "create access method app_heap type table handler app.heap_handler",
        CreateAmStmt
    );
    assert_eq!(access_method.amname.as_deref(), Some("app_heap"));
    assert_eq!(access_method.amtype, b't');
    assert_eq!(access_method.handler_name.len(), 2);

    let quoted = parse_node!(
        "create access method \"select\" type table handler app.select",
        CreateAmStmt
    );
    assert_eq!(quoted.amname.as_deref(), Some("select"));
    assert!(matches!(
        quoted.handler_name.as_slice(),
        [Node::String(schema), Node::String(name)]
            if schema.sval.as_deref() == Some("app") && name.sval.as_deref() == Some("select")
    ));

    let index_am = parse_node!(
        "create access method app_index type index handler app.index_handler",
        CreateAmStmt
    );
    assert_eq!(index_am.amtype, b'i');
}
