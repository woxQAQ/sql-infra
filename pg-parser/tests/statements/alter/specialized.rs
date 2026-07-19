use super::*;

#[test]
fn alter_text_search_dictionary_and_configuration_populate_mapping_fields() {
    let dictionary = parse_node!(
        "alter text search dictionary app.english (stopwords = 'english', accept = false)",
        AlterTsDictionaryStmt
    );
    assert_eq!(dictionary.dictname.len(), 2);
    assert_eq!(dictionary.options.len(), 2);

    let add = parse_node!(
        "alter text search configuration app.english add mapping for asciiword, word with app.simple, public.english_stem",
        AlterTsConfigurationStmt
    );
    assert_eq!(add.cfgname.len(), 2);
    assert_eq!(add.kind, AlterTsConfigType::AddMapping);
    assert_eq!(add.tokentype.len(), 2);
    assert_eq!(add.dicts.len(), 2);
    assert!(!add.override_);
    assert!(!add.replace);

    let replace = parse_node!(
        "alter text search configuration app.english alter mapping for word replace public.english_stem with app.custom_stem",
        AlterTsConfigurationStmt
    );
    assert_eq!(replace.kind, AlterTsConfigType::ReplaceDictForToken);
    assert_eq!(replace.tokentype.len(), 1);
    assert_eq!(replace.dicts.len(), 2);
    assert!(replace.replace);

    let override_mapping = parse_node!(
        "alter text search configuration app.english alter mapping for word with app.simple",
        AlterTsConfigurationStmt
    );
    assert_eq!(
        override_mapping.kind,
        AlterTsConfigType::AlterMappingForToken
    );
    assert!(override_mapping.override_);
    assert!(!override_mapping.replace);

    let replace_all = parse_node!(
        "alter text search configuration app.english alter mapping replace public.english_stem with app.custom_stem",
        AlterTsConfigurationStmt
    );
    assert_eq!(replace_all.kind, AlterTsConfigType::ReplaceDict);
    assert!(replace_all.tokentype.is_empty());
    assert_eq!(replace_all.dicts.len(), 2);
    assert!(replace_all.replace);

    let drop = parse_node!(
        "alter text search configuration app.english drop mapping if exists for email, url",
        AlterTsConfigurationStmt
    );
    assert_eq!(drop.kind, AlterTsConfigType::DropMapping);
    assert!(drop.missing_ok);
    assert_eq!(drop.tokentype.len(), 2);
}

#[test]
fn alter_property_graph_populates_table_label_and_property_actions() {
    let add = parse_node!(
        "alter property graph social add vertex tables (users as u key (id) label person properties (name as display_name)) add edge tables (follows as f source u destination u no properties)",
        AlterPropGraphStmt
    );
    assert_eq!(
        add.pgname
            .as_deref()
            .and_then(|name| name.relname.as_deref()),
        Some("social")
    );
    assert_eq!(add.add_vertex_tables.len(), 1);
    assert_eq!(add.add_edge_tables.len(), 1);
    let vertex = expect_node!(&add.add_vertex_tables[0], PropGraphVertex);
    assert_eq!(vertex.vkey.len(), 1);
    assert_eq!(vertex.labels.len(), 1);
    let edge = expect_node!(&add.add_edge_tables[0], PropGraphEdge);
    assert_eq!(
        edge.etable
            .as_deref()
            .and_then(|table| table.relname.as_deref()),
        Some("follows")
    );
    assert_eq!(edge.esrcvertex.as_deref(), Some("u"));
    assert_eq!(edge.edestvertex.as_deref(), Some("u"));

    let labels = parse_node!(
        "alter property graph social alter vertex table u add label employee properties (salary as pay) add label active no properties",
        AlterPropGraphStmt
    );
    assert_eq!(labels.element_kind, AlterPropGraphElementKind::Vertex);
    assert_eq!(labels.element_alias.as_deref(), Some("u"));
    assert_eq!(labels.add_labels.len(), 2);

    let properties = parse_node!(
        "alter property graph social alter edge table f alter label follows drop properties (weight, since) cascade",
        AlterPropGraphStmt
    );
    assert_eq!(properties.element_kind, AlterPropGraphElementKind::Edge);
    assert_eq!(properties.alter_label.as_deref(), Some("follows"));
    assert_eq!(properties.drop_properties.len(), 2);
    assert_eq!(properties.drop_behavior, DropBehavior::Cascade);

    let drop_tables = parse_node!(
        "alter property graph social drop vertex tables (u, company) cascade",
        AlterPropGraphStmt
    );
    assert_eq!(drop_tables.drop_vertex_tables.len(), 2);
    assert!(drop_tables.drop_edge_tables.is_empty());
    assert_eq!(drop_tables.drop_behavior, DropBehavior::Cascade);

    let drop_edges = parse_node!(
        "alter property graph social drop edge tables (f) restrict",
        AlterPropGraphStmt
    );
    assert_eq!(drop_edges.drop_edge_tables.len(), 1);
    assert!(drop_edges.drop_vertex_tables.is_empty());

    let drop_label = parse_node!(
        "alter property graph social alter vertex table u drop label employee restrict",
        AlterPropGraphStmt
    );
    assert_eq!(drop_label.drop_label.as_deref(), Some("employee"));
    assert_eq!(drop_label.drop_behavior, DropBehavior::Restrict);

    let sql = "alter property graph social alter edge table f alter label follows add properties (weight as strength)";
    let add_properties = parse_node!(sql, AlterPropGraphStmt);
    let properties = add_properties
        .add_properties
        .as_deref()
        .expect("ADD PROPERTIES payload");
    assert_eq!(properties.properties.len(), 1);
    assert_eq!(
        properties.location,
        sql.find("add properties").unwrap() as i32
    );
}
