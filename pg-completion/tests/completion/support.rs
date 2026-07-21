use pg_completion::{
    Catalog, CatalogItem, CatalogObjectIdentity, CatalogObjectKind, CompletionItem, CompletionKind,
    CompletionRequest, CompletionResult, MemoryCatalog, complete,
};
use pg_parser::TextSize;

fn standard_catalog() -> MemoryCatalog {
    let mut catalog = MemoryCatalog::new();
    catalog.add_schema("public");
    catalog.add_schema("audit");
    catalog.add_schema("pg_catalog");
    catalog.add_relation(
        "public",
        "users",
        CatalogObjectKind::Table,
        [
            ("id".into(), "integer".into()),
            ("name".into(), "text".into()),
            ("select".into(), "text".into()),
        ],
    );
    catalog.add_relation(
        "public",
        "orders",
        CatalogObjectKind::Table,
        [
            ("id".into(), "integer".into()),
            ("user_id".into(), "integer".into()),
            ("amount".into(), "numeric".into()),
        ],
    );
    catalog.add_relation(
        "audit",
        "active_users",
        CatalogObjectKind::View,
        [
            ("id".into(), "integer".into()),
            ("name".into(), "text".into()),
        ],
    );
    catalog.add_relation(
        "audit",
        "recent_orders",
        CatalogObjectKind::MaterializedView,
        [
            ("id".into(), "integer".into()),
            ("amount".into(), "numeric".into()),
        ],
    );
    catalog.add_relation(
        "public",
        "UserProfile",
        CatalogObjectKind::Table,
        [("DisplayName".into(), "text".into())],
    );
    catalog.add(
        CatalogItem::new(CatalogObjectIdentity::in_schema(
            CatalogObjectKind::Function,
            "pg_catalog",
            "count",
        ))
        .with_definition("count(any) -> bigint")
        .with_documentation("number of input rows"),
    );
    catalog.add_function(
        "public",
        "calculate_total",
        "calculate_total(numeric) -> numeric",
    );
    catalog.add_type("pg_catalog", "integer");
    catalog.add_type("public", "order_status");
    catalog
}

pub struct Fixture {
    pub catalog: MemoryCatalog,
    search_path: Vec<&'static str>,
}

impl Default for Fixture {
    fn default() -> Self {
        Self {
            catalog: standard_catalog(),
            search_path: vec!["public", "pg_catalog"],
        }
    }
}

impl Fixture {
    pub fn with_search_path(mut self, search_path: &[&'static str]) -> Self {
        self.search_path = search_path.to_vec();
        self
    }

    pub fn complete(&self, marked: &str) -> Completed {
        self.complete_with_catalog(marked, Some(&self.catalog))
    }

    pub fn complete_without_catalog(&self, marked: &str) -> Completed {
        self.complete_with_catalog(marked, None)
    }

    pub fn complete_with(&self, marked: &str, catalog: Option<&dyn Catalog>) -> Completed {
        self.complete_with_catalog(marked, catalog)
    }

    pub fn error_without_catalog(&self, sql: &str, cursor: usize) -> pg_parser::CompletionError {
        complete(
            CompletionRequest {
                sql,
                cursor: TextSize::try_from(cursor).expect("test cursor fits in TextSize"),
                search_path: &self.search_path,
            },
            None,
        )
        .expect_err("completion request should fail")
    }

    fn complete_with_catalog(&self, marked: &str, catalog: Option<&dyn Catalog>) -> Completed {
        let (sql, cursor) = marked_sql(marked);
        let result = complete(
            CompletionRequest {
                sql: &sql,
                cursor: TextSize::try_from(cursor).expect("test cursor fits in TextSize"),
                search_path: &self.search_path,
            },
            catalog,
        )
        .unwrap_or_else(|error| panic!("completion failed for {marked:?}: {error}"));
        Completed {
            marked: marked.to_owned(),
            sql,
            cursor,
            result,
        }
    }
}

pub struct Completed {
    marked: String,
    sql: String,
    cursor: usize,
    result: CompletionResult,
}

impl Completed {
    pub fn assert_has(&self, label: &str, kind: CompletionKind) -> &Self {
        assert!(
            self.find(label, kind).is_some(),
            "expected {label:?} ({kind:?}) for {:?}; got {:?}",
            self.marked,
            self.summary()
        );
        self
    }

    pub fn assert_lacks(&self, label: &str, kind: CompletionKind) -> &Self {
        assert!(
            self.find(label, kind).is_none(),
            "did not expect {label:?} ({kind:?}) for {:?}; got {:?}",
            self.marked,
            self.summary()
        );
        self
    }

    pub fn assert_has_all(&self, kind: CompletionKind, labels: &[&str]) -> &Self {
        for label in labels {
            self.assert_has(label, kind);
        }
        self
    }

    pub fn assert_lacks_kind(&self, kind: CompletionKind) -> &Self {
        let unexpected = self
            .result
            .items
            .iter()
            .filter(|item| item.kind == kind)
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>();
        assert!(
            unexpected.is_empty(),
            "did not expect {kind:?} candidates for {:?}; got {unexpected:?}",
            self.marked
        );
        self
    }

    pub fn assert_empty(&self) -> &Self {
        assert!(
            self.result.items.is_empty(),
            "expected no completion items for {:?}; got {:?}",
            self.marked,
            self.summary()
        );
        self
    }

    pub fn assert_count(&self, label: &str, kind: CompletionKind, expected: usize) -> &Self {
        let actual = self
            .result
            .items
            .iter()
            .filter(|item| item.label == label && item.kind == kind)
            .count();
        assert_eq!(actual, expected, "{:?}", self.marked);
        self
    }

    pub fn assert_insert_text(&self, label: &str, kind: CompletionKind, expected: &str) -> &Self {
        assert_eq!(
            self.find_required(label, kind).insert_text,
            expected,
            "{:?}",
            self.marked
        );
        self
    }

    pub fn assert_has_detail(&self, label: &str, kind: CompletionKind, expected: &str) -> &Self {
        self.assert_has_matching(label, kind, |item| item.detail.as_deref() == Some(expected))
    }

    pub fn assert_documentation(&self, label: &str, kind: CompletionKind, expected: &str) -> &Self {
        assert_eq!(
            self.find_required(label, kind).documentation.as_deref(),
            Some(expected),
            "{:?}",
            self.marked
        );
        self
    }

    pub fn assert_has_matching(
        &self,
        label: &str,
        kind: CompletionKind,
        predicate: impl Fn(&CompletionItem) -> bool,
    ) -> &Self {
        assert!(
            self.result
                .items
                .iter()
                .any(|item| item.label == label && item.kind == kind && predicate(item)),
            "expected matching {label:?} ({kind:?}) for {:?}; got {:?}",
            self.marked,
            self.summary()
        );
        self
    }

    pub fn assert_all_items(&self, predicate: impl Fn(&CompletionItem) -> bool) -> &Self {
        let unexpected = self
            .result
            .items
            .iter()
            .filter(|item| !predicate(item))
            .collect::<Vec<_>>();
        assert!(
            unexpected.is_empty(),
            "unexpected completion items for {:?}: {unexpected:?}",
            self.marked
        );
        self
    }

    pub fn assert_kind_label_set(&self, kind: CompletionKind, expected: &[&str]) -> &Self {
        let actual = self
            .result
            .items
            .iter()
            .filter(|item| item.kind == kind)
            .map(|item| item.label.as_str())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(
            actual,
            expected.iter().copied().collect(),
            "{:?}",
            self.marked
        );
        self
    }

    pub fn assert_labels_in_order(&self, kind: CompletionKind, expected: &[&str]) -> &Self {
        let mut previous = None;
        for label in expected {
            let position = self
                .result
                .items
                .iter()
                .position(|item| item.kind == kind && item.label == *label)
                .unwrap_or_else(|| panic!("missing {label:?} ({kind:?}) for {:?}", self.marked));
            if let Some(previous) = previous {
                assert!(
                    previous < position,
                    "expected {expected:?} in order for {:?}; got {:?}",
                    self.marked,
                    self.summary()
                );
            }
            previous = Some(position);
        }
        self
    }

    pub fn assert_details_in_order(&self, kind: CompletionKind, expected: &[&str]) -> &Self {
        let mut previous = None;
        for detail in expected {
            let position = self
                .result
                .items
                .iter()
                .position(|item| item.kind == kind && item.detail.as_deref() == Some(*detail))
                .unwrap_or_else(|| {
                    panic!("missing detail {detail:?} ({kind:?}) for {:?}", self.marked)
                });
            if let Some(previous) = previous {
                assert!(
                    previous < position,
                    "expected details {expected:?} in order for {:?}; got {:?}",
                    self.marked,
                    self.summary()
                );
            }
            previous = Some(position);
        }
        self
    }

    pub fn assert_candidates_in_order(&self, expected: &[(CompletionKind, &str)]) -> &Self {
        let mut previous = None;
        for (kind, detail) in expected {
            let position = self
                .result
                .items
                .iter()
                .position(|item| item.kind == *kind && item.detail.as_deref() == Some(*detail))
                .unwrap_or_else(|| {
                    panic!("missing detail {detail:?} ({kind:?}) for {:?}", self.marked)
                });
            if let Some(previous) = previous {
                assert!(
                    previous < position,
                    "expected candidates {expected:?} in order for {:?}; got {:?}",
                    self.marked,
                    self.summary()
                );
            }
            previous = Some(position);
        }
        self
    }

    pub fn assert_value_expression(&self) -> &Self {
        self.assert_has("count", CompletionKind::Function)
            .assert_has("NULL", CompletionKind::Keyword)
            .assert_lacks_kind(CompletionKind::Schema)
            .assert_lacks_kind(CompletionKind::Table)
            .assert_lacks_kind(CompletionKind::View)
            .assert_lacks_kind(CompletionKind::MaterializedView)
            .assert_lacks_kind(CompletionKind::Type)
            .assert_lacks("LATERAL", CompletionKind::Keyword)
    }

    pub fn assert_required_value_expression(&self) -> &Self {
        self.assert_value_expression()
            .assert_lacks("FROM", CompletionKind::Keyword)
            .assert_lacks("WHERE", CompletionKind::Keyword)
            .assert_lacks("GROUP", CompletionKind::Keyword)
            .assert_lacks("HAVING", CompletionKind::Keyword)
            .assert_lacks("ORDER", CompletionKind::Keyword)
            .assert_lacks("LIMIT", CompletionKind::Keyword)
            .assert_lacks("OFFSET", CompletionKind::Keyword)
            .assert_lacks("RETURNING", CompletionKind::Keyword)
    }

    pub fn assert_visible_value_expression(&self) -> &Self {
        self.assert_value_expression()
            .assert_has("name", CompletionKind::Column)
    }

    pub fn assert_required_visible_value_expression(&self) -> &Self {
        self.assert_required_value_expression()
            .assert_has("name", CompletionKind::Column)
    }

    pub fn assert_no_duplicate_items(&self) -> &Self {
        let mut seen = std::collections::HashSet::new();
        for item in &self.result.items {
            let identity = (
                item.kind,
                item.label.to_ascii_lowercase(),
                item.detail.as_deref(),
            );
            assert!(
                seen.insert(identity),
                "duplicate completion item for {:?}: {:?}",
                self.marked,
                item
            );
        }
        self
    }

    pub fn assert_prefix_filtered(&self, prefix: &str, case_sensitive: bool) -> &Self {
        for item in &self.result.items {
            let matches = if case_sensitive {
                item.label.starts_with(prefix)
            } else {
                item.label
                    .to_ascii_lowercase()
                    .starts_with(&prefix.to_ascii_lowercase())
            };
            assert!(
                matches,
                "completion item {:?} does not match prefix {prefix:?} for {:?}",
                item, self.marked
            );
        }
        self
    }

    pub fn assert_replacement_contract(&self) -> &Self {
        let start = usize::from(self.result.replacement.start());
        let end = usize::from(self.result.replacement.end());
        assert!(
            start <= end,
            "invalid replacement range for {:?}",
            self.marked
        );
        assert!(
            end <= self.sql.len(),
            "replacement exceeds source for {:?}",
            self.marked
        );
        assert!(
            self.sql.is_char_boundary(start) && self.sql.is_char_boundary(end),
            "replacement is not on UTF-8 boundaries for {:?}",
            self.marked
        );
        assert!(
            start <= self.cursor && self.cursor <= end,
            "replacement does not contain cursor for {:?}: {start}..{end}, cursor {}",
            self.marked,
            self.cursor
        );
        self
    }

    pub fn assert_first(&self, label: &str, kind: CompletionKind) -> &Self {
        let first = self
            .result
            .items
            .first()
            .unwrap_or_else(|| panic!("no completion items for {:?}", self.marked));
        assert_eq!(
            (&*first.label, first.kind),
            (label, kind),
            "{:?}",
            self.marked
        );
        self
    }

    pub fn assert_incomplete(&self, expected: bool) -> &Self {
        assert_eq!(self.result.is_incomplete, expected, "{:?}", self.marked);
        self
    }

    pub fn assert_replaces(&self, expected: &str) -> &Self {
        let start = usize::from(self.result.replacement.start());
        let end = usize::from(self.result.replacement.end());
        assert_eq!(&self.sql[start..end], expected, "{:?}", self.marked);
        self
    }

    fn find_required(&self, label: &str, kind: CompletionKind) -> &CompletionItem {
        self.find(label, kind).unwrap_or_else(|| {
            panic!(
                "missing {label:?} ({kind:?}) for {:?}; got {:?}",
                self.marked,
                self.summary()
            )
        })
    }

    pub fn assert_kind_labels(&self, kind: CompletionKind, expected: &[&str]) -> &Self {
        let actual = self
            .result
            .items
            .iter()
            .filter(|item| item.kind == kind)
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{:?}", self.marked);
        self
    }

    fn find(&self, label: &str, kind: CompletionKind) -> Option<&CompletionItem> {
        self.result
            .items
            .iter()
            .find(|item| item.label == label && item.kind == kind)
    }

    fn summary(&self) -> Vec<(&str, CompletionKind)> {
        self.result
            .items
            .iter()
            .map(|item| (item.label.as_str(), item.kind))
            .collect()
    }
}

fn marked_sql(marked: &str) -> (String, usize) {
    let mut markers = marked.match_indices('|');
    let (cursor, _) = markers
        .next()
        .unwrap_or_else(|| panic!("completion test must contain one '|' marker: {marked:?}"));
    assert!(
        markers.next().is_none(),
        "completion test must contain exactly one '|' marker: {marked:?}"
    );
    (marked.replacen('|', "", 1), cursor)
}
