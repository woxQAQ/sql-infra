//! Browser-facing completion adapter used by the Monaco playground.
//!
//! The exported WebAssembly interface intentionally consists of one JSON
//! request/response operation. Monaco's UTF-16 offsets, catalog resolution,
//! candidate rendering, and `pg-completion`'s UTF-8 byte offsets stay behind
//! that interface.

use std::collections::HashSet;

use pg_completion::CompletionContext;
use pg_completion::CompletionPrefix;
use pg_completion::GrammarSlot;
use pg_completion::IdentifierQuoting;
use pg_completion::NamePart;
use pg_completion::ObjectKind;
use pg_completion::ObjectReference;
use pg_completion::RelationKind;
use pg_completion::SyntaxCompletionKind;
use pg_completion::VisibleRelation;
use pg_completion::collect;
use pg_parser::KEYWORDS;
use pg_parser::KeywordCategory;
use pg_parser::TextRange;
use pg_parser::TextSize;
use pg_parser::TokenKind;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompletionRequest {
    source: String,
    cursor_utf16: u32,
    catalog: CatalogDocument,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogDocument {
    #[serde(default = "default_search_path")]
    search_path: Vec<String>,
    #[serde(default)]
    objects: Vec<CatalogObjectInput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogObjectInput {
    kind: String,
    name: Vec<String>,
    #[serde(default)]
    detail: Option<String>,
    #[serde(default)]
    members: Vec<CatalogMemberInput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CatalogMemberInput {
    kind: String,
    name: String,
    #[serde(default)]
    detail: Option<String>,
}

fn default_search_path() -> Vec<String> {
    vec!["public".to_owned()]
}

#[derive(Debug)]
struct Catalog {
    search_path: Vec<String>,
    objects: Vec<CatalogObject>,
}

#[derive(Debug)]
struct CatalogObject {
    kind: ObjectKind,
    name: Vec<String>,
    detail: Option<String>,
    members: Vec<CatalogMember>,
}

#[derive(Clone, Debug)]
struct CatalogMember {
    kind: ObjectKind,
    name: String,
    detail: Option<String>,
}

impl Catalog {
    fn build(document: CatalogDocument) -> Result<Self, String> {
        for (index, part) in document.search_path.iter().enumerate() {
            if part.is_empty() {
                return Err(format!("catalog.searchPath[{index}] must not be empty"));
            }
        }

        let mut objects = Vec::with_capacity(document.objects.len());
        for (object_index, input) in document.objects.into_iter().enumerate() {
            let kind = parse_object_kind(&input.kind).ok_or_else(|| {
                format!(
                    "catalog.objects[{object_index}].kind has unknown value {:?}",
                    input.kind
                )
            })?;
            if input.name.is_empty() || input.name.iter().any(String::is_empty) {
                return Err(format!(
                    "catalog.objects[{object_index}].name must contain non-empty identifiers"
                ));
            }
            let mut members = Vec::with_capacity(input.members.len());
            for (member_index, member) in input.members.into_iter().enumerate() {
                let member_kind = parse_object_kind(&member.kind).ok_or_else(|| {
                    format!(
                        "catalog.objects[{object_index}].members[{member_index}].kind has unknown value {:?}",
                        member.kind
                    )
                })?;
                if member.name.is_empty() {
                    return Err(format!(
                        "catalog.objects[{object_index}].members[{member_index}].name must not be empty"
                    ));
                }
                members.push(CatalogMember {
                    kind: member_kind,
                    name: member.name,
                    detail: member.detail,
                });
            }
            objects.push(CatalogObject {
                kind,
                name: input.name,
                detail: input.detail,
                members,
            });
        }
        Ok(Self {
            search_path: document.search_path,
            objects,
        })
    }

    fn objects<'a>(
        &'a self,
        kinds: &'a [ObjectKind],
        qualifier: &'a [NamePart],
    ) -> impl Iterator<Item = &'a CatalogObject> {
        self.objects.iter().filter(move |object| {
            kinds.contains(&object.kind)
                && (qualifier.is_empty()
                    || (object.name.len() == qualifier.len() + 1
                        && names_match_parts(&object.name[..object.name.len() - 1], qualifier)))
        })
    }

    fn members(
        &self,
        reference: &ObjectReference,
        kinds: &[ObjectKind],
    ) -> Vec<(&CatalogObject, &CatalogMember)> {
        self.resolve_reference(&reference.name, &reference.object_kinds)
            .into_iter()
            .flat_map(|object| {
                object
                    .members
                    .iter()
                    .filter(move |member| kinds.contains(&member.kind))
                    .map(move |member| (object, member))
            })
            .collect()
    }

    fn columns(&self, relation: &VisibleRelation) -> Vec<(&CatalogObject, &CatalogMember)> {
        self.resolve_reference(&relation.name, RELATION_OBJECT_KINDS)
            .into_iter()
            .flat_map(|object| {
                object
                    .members
                    .iter()
                    .filter(|member| member.kind == ObjectKind::Column)
                    .map(move |member| (object, member))
            })
            .collect()
    }

    fn resolve_reference(&self, name: &[NamePart], kinds: &[ObjectKind]) -> Vec<&CatalogObject> {
        self.objects
            .iter()
            .filter(|object| kinds.contains(&object.kind))
            .filter(|object| {
                if object.name.len() == name.len() && names_match_parts(&object.name, name) {
                    return true;
                }
                name.len() == 1
                    && object.name.last() == Some(&name[0].normalized)
                    && self.is_on_search_path(object)
            })
            .collect()
    }

    fn is_on_search_path(&self, object: &CatalogObject) -> bool {
        match object.name.as_slice() {
            [_] => true,
            [schema, _] => self.search_path.contains(schema),
            _ => false,
        }
    }
}

const RELATION_OBJECT_KINDS: &[ObjectKind] = &[
    ObjectKind::Table,
    ObjectKind::View,
    ObjectKind::MaterializedView,
    ObjectKind::ForeignTable,
];

const ALL_OBJECT_KINDS: &[ObjectKind] = &[
    ObjectKind::Table,
    ObjectKind::View,
    ObjectKind::MaterializedView,
    ObjectKind::ForeignTable,
    ObjectKind::Sequence,
    ObjectKind::Index,
    ObjectKind::Column,
    ObjectKind::Attribute,
    ObjectKind::Function,
    ObjectKind::Procedure,
    ObjectKind::Routine,
    ObjectKind::Aggregate,
    ObjectKind::Type,
    ObjectKind::Domain,
    ObjectKind::Schema,
    ObjectKind::Constraint,
    ObjectKind::Collation,
    ObjectKind::Operator,
    ObjectKind::OperatorClass,
    ObjectKind::OperatorFamily,
    ObjectKind::Role,
    ObjectKind::Database,
    ObjectKind::AccessMethod,
    ObjectKind::Conversion,
    ObjectKind::EventTrigger,
    ObjectKind::Extension,
    ObjectKind::ForeignDataWrapper,
    ObjectKind::ForeignServer,
    ObjectKind::Language,
    ObjectKind::Policy,
    ObjectKind::PropertyGraph,
    ObjectKind::Publication,
    ObjectKind::Rule,
    ObjectKind::Statistics,
    ObjectKind::Subscription,
    ObjectKind::Tablespace,
    ObjectKind::TextSearchConfiguration,
    ObjectKind::TextSearchDictionary,
    ObjectKind::TextSearchParser,
    ObjectKind::TextSearchTemplate,
    ObjectKind::Trigger,
];

fn parse_object_kind(input: &str) -> Option<ObjectKind> {
    ALL_OBJECT_KINDS
        .iter()
        .copied()
        .find(|kind| object_kind_name(*kind).eq_ignore_ascii_case(input))
}

fn object_kind_name(kind: ObjectKind) -> String {
    format!("{kind:?}")
}

fn names_match_parts(names: &[String], parts: &[NamePart]) -> bool {
    names.len() == parts.len()
        && names
            .iter()
            .zip(parts)
            .all(|(name, part)| name == &part.normalized)
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompletionItemView {
    label: String,
    insert_text: String,
    replacement_range: OffsetRangeView,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    object_kind: Option<String>,
    detail: String,
    origin: String,
    sort_text: String,
    trigger_suggest: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompletionResponse {
    items: Vec<CompletionItemView>,
    context: ContextView,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WireResponse {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    completion: Option<CompletionResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl WireResponse {
    fn success(completion: CompletionResponse) -> Self {
        Self {
            ok: true,
            completion: Some(completion),
            error: None,
        }
    }

    fn error(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            completion: None,
            error: Some(error.into()),
        }
    }
}

/// Execute the playground interface without WebAssembly. Tests use the same
/// JSON interface as the browser worker.
pub fn complete_json(input: &str) -> String {
    let wire = match serde_json::from_str::<CompletionRequest>(input) {
        Ok(request) => match complete_request(request) {
            Ok(response) => WireResponse::success(response),
            Err(error) => WireResponse::error(error),
        },
        Err(error) => WireResponse::error(format!("invalid completion request: {error}")),
    };
    serde_json::to_string(&wire).expect("wire response is serializable")
}

fn complete_request(request: CompletionRequest) -> Result<CompletionResponse, String> {
    let catalog = Catalog::build(request.catalog)?;
    let mapped = utf16_to_byte_point(&request.source, request.cursor_utf16);
    let requested_byte = TextSize::try_from(mapped.requested_byte)
        .map_err(|error| format!("completion point is too large: {error}"))?;
    let context = collect(&request.source, requested_byte);
    let mut items = Vec::new();

    add_syntax_items(&request.source, &context, &mut items);
    add_privilege_items(&request.source, &context, &mut items);
    add_scope_items(&request.source, &context, &catalog, &mut items);
    add_relation_qualifiers(&request.source, &context, &mut items);
    add_catalog_items(&request.source, &context, &catalog, &mut items);
    add_cte_items(&request.source, &context, &mut items);

    let mut seen = HashSet::new();
    items.retain(|item| {
        seen.insert((
            item.kind.clone(),
            item.label.clone(),
            item.insert_text.clone(),
            item.detail.clone(),
        ))
    });
    items.sort_by(|left, right| {
        left.sort_text
            .cmp(&right.sort_text)
            .then_with(|| left.label.cmp(&right.label))
    });

    Ok(CompletionResponse {
        items,
        context: ContextView::new(&request.source, &context, request.cursor_utf16, mapped),
    })
}

fn add_syntax_items(
    source: &str,
    context: &CompletionContext,
    items: &mut Vec<CompletionItemView>,
) {
    for syntax in context.syntax_completions() {
        let (kind, detail, rank) = match syntax.kind {
            SyntaxCompletionKind::Keyword => (
                "keyword",
                "SQL syntax",
                if syntax.is_follow { 40 } else { 20 },
            ),
            SyntaxCompletionKind::Phrase => (
                "phrase",
                "SQL phrase",
                if syntax.is_follow { 30 } else { 25 },
            ),
        };
        push_item(
            source,
            context,
            syntax.label,
            syntax.insert_text,
            kind,
            None,
            detail.to_owned(),
            "syntax",
            rank,
            false,
            items,
        );
    }
}

fn add_privilege_items(
    source: &str,
    context: &CompletionContext,
    items: &mut Vec<CompletionItemView>,
) {
    if !context.expectations.slots.contains(&GrammarSlot::Privilege) {
        return;
    }
    for privilege in [
        "SELECT",
        "INSERT",
        "UPDATE",
        "DELETE",
        "TRUNCATE",
        "REFERENCES",
        "TRIGGER",
        "CREATE",
        "CONNECT",
        "TEMPORARY",
        "EXECUTE",
        "USAGE",
        "SET",
        "ALTER SYSTEM",
        "MAINTAIN",
    ] {
        if prefix_matches(&context.prefix, privilege) {
            push_item(
                source,
                context,
                privilege.to_owned(),
                privilege.to_owned(),
                "privilege",
                None,
                "PostgreSQL privilege".to_owned(),
                "privilege",
                20,
                false,
                items,
            );
        }
    }
}

fn add_scope_items(
    source: &str,
    context: &CompletionContext,
    catalog: &Catalog,
    items: &mut Vec<CompletionItemView>,
) {
    if !context.intent.object_kinds.contains(&ObjectKind::Column)
        || context
            .intent
            .membership
            .as_ref()
            .is_some_and(|membership| membership.member_kinds.contains(&ObjectKind::Column))
    {
        return;
    }

    for (relation, scope_name) in visible_relations(context) {
        if relation.unsupported.is_some()
            || !relation_is_visible_for_qualifier(relation, &context.intent.qualifier)
        {
            continue;
        }
        let relation_label = visible_relation_label(relation);
        if relation.explicit_columns.is_empty() {
            if relation.kind != RelationKind::Relation {
                continue;
            }
            for (object, member) in catalog.columns(relation) {
                if !prefix_matches(&context.prefix, &member.name) {
                    continue;
                }
                let mut detail = format!(
                    "{} · {} · {}",
                    object_kind_name(member.kind),
                    object.name.join("."),
                    scope_name
                );
                if let Some(member_detail) = &member.detail {
                    detail.push_str(" · ");
                    detail.push_str(member_detail);
                }
                push_item(
                    source,
                    context,
                    member.name.clone(),
                    quote_identifier(&member.name),
                    "column",
                    Some(member.kind),
                    detail,
                    "scope",
                    0,
                    false,
                    items,
                );
            }
        } else {
            for column in &relation.explicit_columns {
                if !prefix_matches(&context.prefix, &column.text) {
                    continue;
                }
                push_item(
                    source,
                    context,
                    column.text.clone(),
                    quote_identifier(&column.text),
                    "column",
                    Some(ObjectKind::Column),
                    format!("Column · {relation_label} · syntax-known output"),
                    "scope",
                    0,
                    false,
                    items,
                );
            }
        }
    }
}

fn add_relation_qualifiers(
    source: &str,
    context: &CompletionContext,
    items: &mut Vec<CompletionItemView>,
) {
    if !context.intent.object_kinds.contains(&ObjectKind::Column)
        || !context.intent.qualifier.is_empty()
    {
        return;
    }
    for (relation, scope_name) in visible_relations(context) {
        if relation.unsupported.is_some() {
            continue;
        }
        let part = relation.alias.as_ref().or_else(|| relation.name.last());
        let Some(part) = part else {
            continue;
        };
        if !prefix_matches(&context.prefix, &part.text) {
            continue;
        }
        push_item(
            source,
            context,
            part.text.clone(),
            quote_identifier(&part.text),
            "reference",
            None,
            format!(
                "Relation qualifier · {} · {scope_name}",
                relation
                    .name
                    .iter()
                    .map(|name| name.text.as_str())
                    .collect::<Vec<_>>()
                    .join(".")
            ),
            "scope",
            10,
            true,
            items,
        );
    }
}

fn add_catalog_items(
    source: &str,
    context: &CompletionContext,
    catalog: &Catalog,
    items: &mut Vec<CompletionItemView>,
) {
    let mut global_kinds = context.intent.object_kinds.clone();
    global_kinds.retain(|kind| *kind != ObjectKind::Column);

    if let Some(membership) = &context.intent.membership {
        let member_kinds = context
            .intent
            .object_kinds
            .iter()
            .copied()
            .filter(|kind| membership.member_kinds.contains(kind))
            .collect::<Vec<_>>();
        for (object, member) in catalog.members(&membership.owner, &member_kinds) {
            if !prefix_matches(&context.prefix, &member.name) {
                continue;
            }
            let mut detail = format!(
                "{} · member of {}",
                object_kind_name(member.kind),
                object.name.join(".")
            );
            if let Some(member_detail) = &member.detail {
                detail.push_str(" · ");
                detail.push_str(member_detail);
            }
            push_item(
                source,
                context,
                member.name.clone(),
                quote_identifier(&member.name),
                editor_kind(member.kind),
                Some(member.kind),
                detail,
                "membership",
                if member.kind == ObjectKind::Column {
                    0
                } else {
                    10
                },
                false,
                items,
            );
        }
        global_kinds.retain(|kind| !membership.member_kinds.contains(kind));
    }

    for object in catalog.objects(&global_kinds, &context.intent.qualifier) {
        let last = object.name.last().expect("catalog names are validated");
        let needs_qualification = context.intent.qualifier.is_empty()
            && object.name.len() > 1
            && !catalog.is_on_search_path(object);
        let insert_text = if needs_qualification {
            quote_qualified_name(&object.name)
        } else {
            quote_identifier(last)
        };
        let display_name = if needs_qualification {
            object.name.join(".")
        } else {
            last.clone()
        };
        if !prefix_matches(&context.prefix, last) && !prefix_matches(&context.prefix, &display_name)
        {
            continue;
        }
        let mut detail = format!(
            "{} · {}",
            object_kind_name(object.kind),
            object.name.join(".")
        );
        if let Some(object_detail) = &object.detail {
            detail.push_str(" · ");
            detail.push_str(object_detail);
        }
        push_item(
            source,
            context,
            display_name,
            insert_text,
            editor_kind(object.kind),
            Some(object.kind),
            detail,
            "catalog",
            10,
            false,
            items,
        );
    }
}

fn add_cte_items(source: &str, context: &CompletionContext, items: &mut Vec<CompletionItemView>) {
    if !context.expectations.slots.contains(&GrammarSlot::Relation) {
        return;
    }
    for cte in &context.scope.ctes {
        if !prefix_matches(&context.prefix, &cte.name.text) {
            continue;
        }
        push_item(
            source,
            context,
            cte.name.text.clone(),
            quote_identifier(&cte.name.text),
            "table",
            Some(ObjectKind::Table),
            "CTE · visible at completion point".to_owned(),
            "cte",
            5,
            false,
            items,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn push_item(
    source: &str,
    context: &CompletionContext,
    label: String,
    insert_text: String,
    kind: &str,
    object_kind: Option<ObjectKind>,
    detail: String,
    origin: &str,
    rank: u8,
    trigger_suggest: bool,
    items: &mut Vec<CompletionItemView>,
) {
    items.push(CompletionItemView {
        sort_text: format!("{rank:02}:{}", label.to_lowercase()),
        label,
        insert_text,
        replacement_range: utf16_range(source, context.replacement_range),
        kind: kind.to_owned(),
        object_kind: object_kind.map(object_kind_name),
        detail,
        origin: origin.to_owned(),
        trigger_suggest,
    });
}

fn visible_relations(context: &CompletionContext) -> Vec<(&VisibleRelation, &'static str)> {
    let mut relations = context
        .scope
        .local
        .relations
        .iter()
        .map(|relation| (relation, "local scope"))
        .collect::<Vec<_>>();
    if let Some(target) = &context.scope.dml_target {
        relations.push((target, "DML target"));
    }
    if let Some(source) = &context.scope.merge_source {
        relations.push((source, "MERGE source"));
    }
    for outer in &context.scope.outer {
        relations.extend(
            outer
                .relations
                .iter()
                .map(|relation| (relation, "outer scope")),
        );
    }
    relations
}

fn relation_is_visible_for_qualifier(relation: &VisibleRelation, qualifier: &[NamePart]) -> bool {
    if qualifier.is_empty() {
        return !relation.qualified_only;
    }
    if let Some(alias) = &relation.alias {
        return qualifier.len() == 1 && alias.normalized == qualifier[0].normalized;
    }
    let matches_full_name = relation.name.len() == qualifier.len()
        && relation
            .name
            .iter()
            .zip(qualifier)
            .all(|(left, right)| left.normalized == right.normalized);
    let matches_relation_name = qualifier.len() == 1
        && relation
            .name
            .last()
            .is_some_and(|name| name.normalized == qualifier[0].normalized);
    matches_full_name || matches_relation_name
}

fn visible_relation_label(relation: &VisibleRelation) -> String {
    let mut label = relation
        .name
        .iter()
        .map(|part| part.text.as_str())
        .collect::<Vec<_>>()
        .join(".");
    if let Some(alias) = &relation.alias {
        label.push_str(" AS ");
        label.push_str(&alias.text);
    }
    label
}

fn prefix_matches(prefix: &CompletionPrefix, candidate: &str) -> bool {
    let candidate = if prefix.quoting == IdentifierQuoting::Unquoted {
        candidate.to_ascii_lowercase()
    } else {
        candidate.to_owned()
    };
    candidate.starts_with(&prefix.normalized)
}

fn quote_qualified_name(parts: &[String]) -> String {
    parts
        .iter()
        .map(|part| quote_identifier(part))
        .collect::<Vec<_>>()
        .join(".")
}

fn quote_identifier(identifier: &str) -> String {
    let simple = identifier
        .bytes()
        .enumerate()
        .all(|(index, byte)| match byte {
            b'a'..=b'z' | b'_' => true,
            b'0'..=b'9' | b'$' => index != 0,
            _ => false,
        });
    let reserved = KEYWORDS.iter().any(|keyword| {
        keyword.word.eq_ignore_ascii_case(identifier)
            && keyword.category == KeywordCategory::Reserved
    });
    if simple && !reserved {
        identifier.to_owned()
    } else {
        format!("\"{}\"", identifier.replace('"', "\"\""))
    }
}

fn token_label(kind: TokenKind) -> Option<String> {
    if let TokenKind::Char(character) = kind {
        return Some(character.to_string());
    }
    if let Some(keyword) = KEYWORDS.iter().find(|keyword| keyword.kind == kind) {
        return Some(keyword.word.to_ascii_uppercase());
    }
    match kind {
        TokenKind::SConst => Some("''".to_owned()),
        TokenKind::TypeCast => Some("::".to_owned()),
        TokenKind::DotDot => Some("..".to_owned()),
        TokenKind::ColonEquals => Some(":=".to_owned()),
        TokenKind::EqualsGreater => Some("=>".to_owned()),
        TokenKind::LessEquals => Some("<=".to_owned()),
        TokenKind::GreaterEquals => Some(">=".to_owned()),
        TokenKind::NotEquals => Some("<>".to_owned()),
        TokenKind::RightArrow => Some("->".to_owned()),
        _ => None,
    }
}

fn editor_kind(kind: ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Column | ObjectKind::Attribute => "column",
        ObjectKind::Function
        | ObjectKind::Procedure
        | ObjectKind::Routine
        | ObjectKind::Aggregate => "function",
        ObjectKind::Table
        | ObjectKind::View
        | ObjectKind::MaterializedView
        | ObjectKind::ForeignTable => "table",
        ObjectKind::Schema => "schema",
        ObjectKind::Type | ObjectKind::Domain => "type",
        ObjectKind::Role => "user",
        ObjectKind::Database => "database",
        _ => "object",
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OffsetRangeView {
    start: u32,
    end: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceRangeView {
    utf8: OffsetRangeView,
    utf16: OffsetRangeView,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ContextView {
    point: PointView,
    statement_range: SourceRangeView,
    replacement_range: SourceRangeView,
    prefix: PrefixView,
    expectations: ExpectationsView,
    intent: IntentView,
    scope: ScopeView,
    diagnostics: Vec<DiagnosticView>,
}

impl ContextView {
    fn new(
        source: &str,
        context: &CompletionContext,
        requested_utf16: u32,
        mapped: MappedPoint,
    ) -> Self {
        let effective_byte = usize::from(context.point);
        let effective_utf16 = byte_to_utf16(source, effective_byte);
        Self {
            point: PointView {
                requested_utf16,
                effective_utf16,
                utf8: u32::try_from(effective_byte).expect("TextSize fits u32"),
                adjusted: mapped.adjusted || effective_byte != mapped.requested_byte,
            },
            statement_range: source_range(source, context.statement_range),
            replacement_range: source_range(source, context.replacement_range),
            prefix: PrefixView {
                raw: context.prefix.raw.clone(),
                normalized: context.prefix.normalized.clone(),
                quoting: format!("{:?}", context.prefix.quoting),
            },
            expectations: ExpectationsView {
                tokens: context
                    .expectations
                    .tokens
                    .iter()
                    .map(|token| token_label(*token).unwrap_or_else(|| format!("{token:?}")))
                    .collect(),
                direct_tokens: context
                    .expectations
                    .direct_tokens
                    .iter()
                    .map(|token| token_label(*token).unwrap_or_else(|| format!("{token:?}")))
                    .collect(),
                lookahead_tokens: context
                    .expectations
                    .lookahead_tokens
                    .iter()
                    .map(|token| token_label(*token).unwrap_or_else(|| format!("{token:?}")))
                    .collect(),
                expression_start_tokens: context
                    .expectations
                    .expression_start_tokens
                    .iter()
                    .map(|token| token_label(*token).unwrap_or_else(|| format!("{token:?}")))
                    .collect(),
                expression_continuation_tokens: context
                    .expectations
                    .expression_continuation_tokens
                    .iter()
                    .map(|token| token_label(*token).unwrap_or_else(|| format!("{token:?}")))
                    .collect(),
                follow_tokens: context
                    .expectations
                    .follow_tokens
                    .iter()
                    .map(|token| token_label(*token).unwrap_or_else(|| format!("{token:?}")))
                    .collect(),
                phrases: context
                    .expectations
                    .phrases
                    .iter()
                    .map(|phrase| {
                        phrase
                            .iter()
                            .map(|token| {
                                token_label(*token).unwrap_or_else(|| format!("{token:?}"))
                            })
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .collect(),
                slots: context
                    .expectations
                    .slots
                    .iter()
                    .map(|slot| format!("{slot:?}"))
                    .collect(),
            },
            intent: IntentView {
                object_kinds: context
                    .intent
                    .object_kinds
                    .iter()
                    .map(|kind| object_kind_name(*kind))
                    .collect(),
                qualifier: context
                    .intent
                    .qualifier
                    .iter()
                    .map(|part| NamePartView::new(source, part))
                    .collect(),
                membership: context.intent.membership.as_ref().map(|membership| {
                    CatalogMembershipView {
                        member_kinds: membership
                            .member_kinds
                            .iter()
                            .map(|kind| object_kind_name(*kind))
                            .collect(),
                        owner: ObjectReferenceView {
                            object_kinds: membership
                                .owner
                                .object_kinds
                                .iter()
                                .map(|kind| object_kind_name(*kind))
                                .collect(),
                            name: membership
                                .owner
                                .name
                                .iter()
                                .map(|part| NamePartView::new(source, part))
                                .collect(),
                        },
                    }
                }),
            },
            scope: ScopeView {
                local: context
                    .scope
                    .local
                    .relations
                    .iter()
                    .map(|relation| RelationView::new(source, relation))
                    .collect(),
                outer: context
                    .scope
                    .outer
                    .iter()
                    .map(|scope| {
                        scope
                            .relations
                            .iter()
                            .map(|relation| RelationView::new(source, relation))
                            .collect()
                    })
                    .collect(),
                ctes: context
                    .scope
                    .ctes
                    .iter()
                    .map(|cte| CteView {
                        name: NamePartView::new(source, &cte.name),
                        explicit_columns: cte
                            .explicit_columns
                            .iter()
                            .map(|part| NamePartView::new(source, part))
                            .collect(),
                        syntax_range: source_range(source, cte.syntax_range),
                        body_range: source_range(source, cte.body_range),
                    })
                    .collect(),
                dml_target: context
                    .scope
                    .dml_target
                    .as_ref()
                    .map(|relation| RelationView::new(source, relation)),
                merge_source: context
                    .scope
                    .merge_source
                    .as_ref()
                    .map(|relation| RelationView::new(source, relation)),
            },
            diagnostics: context
                .diagnostics
                .iter()
                .map(|diagnostic| DiagnosticView {
                    kind: format!("{:?}", diagnostic.kind),
                    range: source_range(source, diagnostic.range),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PointView {
    requested_utf16: u32,
    effective_utf16: u32,
    utf8: u32,
    adjusted: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PrefixView {
    raw: String,
    normalized: String,
    quoting: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExpectationsView {
    tokens: Vec<String>,
    direct_tokens: Vec<String>,
    lookahead_tokens: Vec<String>,
    expression_start_tokens: Vec<String>,
    expression_continuation_tokens: Vec<String>,
    follow_tokens: Vec<String>,
    phrases: Vec<String>,
    slots: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct IntentView {
    object_kinds: Vec<String>,
    qualifier: Vec<NamePartView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    membership: Option<CatalogMembershipView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogMembershipView {
    member_kinds: Vec<String>,
    owner: ObjectReferenceView,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ObjectReferenceView {
    object_kinds: Vec<String>,
    name: Vec<NamePartView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NamePartView {
    text: String,
    normalized: String,
    quoted: bool,
    range: SourceRangeView,
}

impl NamePartView {
    fn new(source: &str, part: &NamePart) -> Self {
        Self {
            text: part.text.clone(),
            normalized: part.normalized.clone(),
            quoted: part.quoted,
            range: source_range(source, part.range),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScopeView {
    local: Vec<RelationView>,
    outer: Vec<Vec<RelationView>>,
    ctes: Vec<CteView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dml_target: Option<RelationView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    merge_source: Option<RelationView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RelationView {
    kind: String,
    name: Vec<NamePartView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    alias: Option<NamePartView>,
    explicit_columns: Vec<NamePartView>,
    qualified_only: bool,
    syntax_range: SourceRangeView,
    #[serde(skip_serializing_if = "Option::is_none")]
    body_range: Option<SourceRangeView>,
    lateral: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    unsupported: Option<UnsupportedView>,
}

impl RelationView {
    fn new(source: &str, relation: &VisibleRelation) -> Self {
        Self {
            kind: format!("{:?}", relation.kind),
            name: relation
                .name
                .iter()
                .map(|part| NamePartView::new(source, part))
                .collect(),
            alias: relation
                .alias
                .as_ref()
                .map(|part| NamePartView::new(source, part)),
            explicit_columns: relation
                .explicit_columns
                .iter()
                .map(|part| NamePartView::new(source, part))
                .collect(),
            qualified_only: relation.qualified_only,
            syntax_range: source_range(source, relation.syntax_range),
            body_range: relation.body_range.map(|range| source_range(source, range)),
            lateral: relation.lateral,
            unsupported: relation
                .unsupported
                .as_ref()
                .map(|unsupported| UnsupportedView {
                    reason: unsupported.reason.clone(),
                    range: source_range(source, unsupported.range),
                }),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UnsupportedView {
    reason: String,
    range: SourceRangeView,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CteView {
    name: NamePartView,
    explicit_columns: Vec<NamePartView>,
    syntax_range: SourceRangeView,
    body_range: SourceRangeView,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticView {
    kind: String,
    range: SourceRangeView,
}

#[derive(Clone, Copy, Debug)]
struct MappedPoint {
    requested_byte: usize,
    adjusted: bool,
}

fn utf16_to_byte_point(source: &str, requested_utf16: u32) -> MappedPoint {
    let requested = requested_utf16 as usize;
    let mut utf16 = 0usize;
    for (byte, character) in source.char_indices() {
        if utf16 == requested {
            return MappedPoint {
                requested_byte: byte,
                adjusted: false,
            };
        }
        let next = utf16 + character.len_utf16();
        if requested < next {
            return MappedPoint {
                requested_byte: byte,
                adjusted: true,
            };
        }
        utf16 = next;
    }
    if requested <= utf16 {
        MappedPoint {
            requested_byte: source.len(),
            adjusted: requested != utf16,
        }
    } else {
        let overshoot = requested - utf16;
        MappedPoint {
            requested_byte: source.len().saturating_add(overshoot),
            adjusted: true,
        }
    }
}

fn byte_to_utf16(source: &str, byte: usize) -> u32 {
    u32::try_from(source[..byte].encode_utf16().count())
        .expect("UTF-16 offset is no larger than the supported source size")
}

fn utf16_range(source: &str, range: TextRange) -> OffsetRangeView {
    OffsetRangeView {
        start: byte_to_utf16(source, usize::from(range.start())),
        end: byte_to_utf16(source, usize::from(range.end())),
    }
}

fn source_range(source: &str, range: TextRange) -> SourceRangeView {
    SourceRangeView {
        utf8: OffsetRangeView {
            start: range.start().get(),
            end: range.end().get(),
        },
        utf16: utf16_range(source, range),
    }
}

#[cfg(target_arch = "wasm32")]
mod ffi {
    use std::sync::Mutex;

    static RESPONSE: Mutex<Vec<u8>> = Mutex::new(Vec::new());

    #[unsafe(no_mangle)]
    pub extern "C" fn playground_alloc(len: u32) -> u32 {
        let bytes = vec![0_u8; len as usize].into_boxed_slice();
        Box::into_raw(bytes) as *mut u8 as u32
    }

    /// Consumes the buffer returned by `playground_alloc` and stores the
    /// response until the next call. The worker copies it before sending a new
    /// request, so a single response slot is sufficient.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn playground_complete(request_ptr: u32, request_len: u32) -> u32 {
        let slice =
            std::ptr::slice_from_raw_parts_mut(request_ptr as *mut u8, request_len as usize);
        let request = unsafe { Box::from_raw(slice) };
        let response = match String::from_utf8(Vec::from(request)) {
            Ok(request) => super::complete_json(&request),
            Err(error) => serde_json::to_string(&super::WireResponse::error(format!(
                "request is not UTF-8: {error}"
            )))
            .expect("wire error is serializable"),
        };
        let mut slot = RESPONSE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *slot = response.into_bytes();
        slot.as_ptr() as u32
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn playground_result_len() -> u32 {
        let slot = RESPONSE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        u32::try_from(slot.len()).expect("completion response fits wasm32 memory")
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use serde_json::json;

    use super::*;

    fn catalog() -> Value {
        json!({
            "searchPath": ["public"],
            "objects": [
                {
                    "kind": "Table",
                    "name": ["public", "users"],
                    "members": [
                        { "kind": "Column", "name": "id", "detail": "bigint" },
                        { "kind": "Column", "name": "name", "detail": "text" }
                    ]
                },
                {
                    "kind": "Function",
                    "name": ["u", "refresh"],
                    "detail": "() returns void"
                },
                {
                    "kind": "Table",
                    "name": ["analytics", "events"],
                    "members": []
                }
            ]
        })
    }

    fn response(source_with_point: &str) -> Value {
        let point = source_with_point.find('|').expect("test has a point");
        let source = source_with_point.replacen('|', "", 1);
        let cursor_utf16 = source[..point].encode_utf16().count();
        let request = json!({
            "source": source,
            "cursorUtf16": cursor_utf16,
            "catalog": catalog()
        });
        serde_json::from_str(&complete_json(&request.to_string())).unwrap()
    }

    fn item_tuples(response: &Value) -> Vec<(&str, &str)> {
        response["completion"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| {
                (
                    item["kind"].as_str().unwrap(),
                    item["label"].as_str().unwrap(),
                )
            })
            .collect()
    }

    #[test]
    fn alias_and_schema_qualifier_are_resolved_independently() {
        let response = response("SELECT u.| FROM public.users AS u");
        let items = item_tuples(&response);
        assert!(items.contains(&("column", "name")), "{items:?}");
        assert!(items.contains(&("function", "refresh")), "{items:?}");
    }

    #[test]
    fn unaliased_schema_qualified_relation_is_visible_by_relation_name() {
        for source in [
            "SELECT * FROM public.orders JOIN public.users ON users.|",
            "SELECT * FROM public.orders JOIN public.users ON public.users.|",
        ] {
            let response = response(source);
            let items = item_tuples(&response);
            assert!(items.contains(&("column", "id")), "{source}: {items:?}");
            assert!(items.contains(&("column", "name")), "{source}: {items:?}");
        }

        let response = response("SELECT * FROM public.orders JOIN public.users AS u ON users.|");
        let items = item_tuples(&response);
        assert!(
            !items.iter().any(|(kind, _)| *kind == "column"),
            "an alias must hide the original relation name: {items:?}"
        );
    }

    #[test]
    fn schema_qualifier_resolves_relation_objects() {
        let response = response("SELECT * FROM public.|");
        let items = item_tuples(&response);
        assert!(
            items.contains(&("table", "users")),
            "{}",
            serde_json::to_string_pretty(&response).unwrap()
        );
        assert_eq!(
            response["completion"]["context"]["intent"]["qualifier"][0]["normalized"],
            "public"
        );
    }

    #[test]
    fn relation_alias_prefix_does_not_offer_clause_keywords() {
        for source in [
            "SELECT * FROM public.orders o|",
            "SELECT * FROM public.orders AS o|",
        ] {
            let response = response(source);
            let items = item_tuples(&response);
            assert!(items.is_empty(), "{source}: {items:?}");
        }
    }

    #[test]
    fn join_prefix_after_relation_alias_offers_join() {
        for source in [
            "SELECT * FROM public.orders o j|",
            "SELECT * FROM public.orders AS o j|",
        ] {
            let response = response(source);
            let items = item_tuples(&response);
            assert_eq!(items, [("keyword", "JOIN")], "{source}: {items:?}");
        }
    }

    #[test]
    fn punctuation_expectations_are_not_published_as_completion_items() {
        let response = response("SELECT |");
        let items = item_tuples(&response);

        for punctuation in ["(", "*", "+", "-"] {
            assert!(
                !items.iter().any(|(_, label)| *label == punctuation),
                "{punctuation}: {items:?}"
            );
        }
        assert!(items.contains(&("keyword", "ARRAY")), "{items:?}");

        let raw_tokens = response["completion"]["context"]["expectations"]["tokens"]
            .as_array()
            .unwrap();
        for punctuation in ["(", "*", "+", "-"] {
            assert!(
                raw_tokens.iter().any(|token| token == punctuation),
                "raw context lost {punctuation}: {raw_tokens:?}"
            );
        }
    }

    #[test]
    fn unprefixed_expression_tail_does_not_publish_editor_items() {
        let tight_response = response("SELECT * FROM public.users JOIN public.orders ON users.id|");
        let items = item_tuples(&tight_response);
        assert!(
            !items
                .iter()
                .any(|(kind, _)| matches!(*kind, "keyword" | "phrase" | "operator")),
            "{items:?}"
        );

        let response = response("SELECT * FROM public.users JOIN public.orders ON users.id |");
        let items = response["completion"]["items"].as_array().unwrap();
        assert!(items.is_empty(), "{items:?}");

        let tokens = response["completion"]["context"]["expectations"]["tokens"]
            .as_array()
            .unwrap();
        assert!(tokens.iter().any(|token| token == "="));
        let follows = response["completion"]["context"]["expectations"]["followTokens"]
            .as_array()
            .unwrap();
        assert!(follows.iter().any(|token| token == "JOIN"));
        assert!(!follows.iter().any(|token| token == "="));
    }

    #[test]
    fn typed_expression_continuation_prefix_restores_keyword_completion() {
        let response = response("SELECT * FROM public.users JOIN public.orders ON users.id A|");
        let items = item_tuples(&response);

        assert!(items.contains(&("keyword", "AND")), "{items:?}");
        assert!(!items.iter().any(|(kind, _)| *kind == "operator"));
    }

    #[test]
    fn member_candidates_come_from_the_named_catalog_object() {
        let response = response("CREATE INDEX users_name ON public.users (|)");
        let items = item_tuples(&response);
        assert!(items.contains(&("column", "id")), "{items:?}");
    }

    #[test]
    fn qualified_only_dml_relation_maps_back_to_target_columns() {
        let response = response(
            "INSERT INTO public.users (id, name) VALUES (1, 'Ada') \
             ON CONFLICT (id) DO UPDATE SET name = excluded.|",
        );
        let items = item_tuples(&response);
        assert!(items.contains(&("column", "name")), "{items:?}");
        let local = response["completion"]["context"]["scope"]["local"]
            .as_array()
            .unwrap();
        assert_eq!(local[0]["alias"]["text"], "excluded");
        assert_eq!(local[0]["qualifiedOnly"], true);
    }

    #[test]
    fn explicit_cte_columns_need_no_catalog_object() {
        let response = response(
            "WITH active(user_id, display_name) AS (SELECT 1, 'Ada') \
             SELECT a.| FROM active AS a",
        );
        let items = item_tuples(&response);
        assert!(items.contains(&("column", "user_id")), "{items:?}");
        assert!(items.contains(&("column", "display_name")), "{items:?}");
    }

    #[test]
    fn objects_outside_search_path_insert_a_qualified_name() {
        let response = response("SELECT * FROM eve|");
        let item = response["completion"]["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["label"] == "analytics.events")
            .unwrap();
        assert_eq!(item["insertText"], "analytics.events");
    }

    #[test]
    fn utf16_cursor_and_ranges_survive_non_bmp_text() {
        let response = response("SELECT '😀', u.na| FROM public.users AS u");
        let context = &response["completion"]["context"];
        assert_eq!(
            context["point"]["effectiveUtf16"],
            context["point"]["requestedUtf16"]
        );
        assert_eq!(context["prefix"]["raw"], "na");
        let range = &context["replacementRange"];
        assert_eq!(
            range["utf16"]["end"].as_u64().unwrap() - range["utf16"]["start"].as_u64().unwrap(),
            2
        );
        assert_eq!(
            range["utf8"]["end"].as_u64().unwrap() - range["utf8"]["start"].as_u64().unwrap(),
            2
        );
    }

    #[test]
    fn invalid_catalog_kind_is_a_wire_error() {
        let request = json!({
            "source": "SELECT ",
            "cursorUtf16": 7,
            "catalog": { "objects": [{ "kind": "DefinitelyNotPostgres", "name": ["x"] }] }
        });
        let response: Value = serde_json::from_str(&complete_json(&request.to_string())).unwrap();
        assert_eq!(response["ok"], false);
        assert!(
            response["error"]
                .as_str()
                .unwrap()
                .contains("unknown value")
        );
    }
}
