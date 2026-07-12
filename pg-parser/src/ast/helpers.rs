use super::*;

pub(crate) fn name_list_node(elements: NodeList) -> Node {
    Node::AArrayExpr(AArrayExpr {
        node_tag: NodeTag::AArrayExpr,
        elements,
        ..AArrayExpr::default()
    })
}

pub(crate) fn make_string_node(value: impl Into<std::string::String>) -> Node {
    Node::String(String::new(value))
}

pub(crate) fn make_def_elem(name: &str, arg: Option<Node>, location: usize) -> Node {
    Node::DefElem(DefElem {
        node_tag: NodeTag::DefElem,
        defname: Some(name.to_owned()),
        arg: arg.map(Box::new),
        location: location as ParseLoc,
        ..DefElem::default()
    })
}

pub(crate) fn range_var_from_parts(parts: Vec<std::string::String>, location: usize) -> RangeVar {
    let mut range = RangeVar {
        node_tag: NodeTag::RangeVar,
        inh: true,
        relpersistence: b'p',
        location: location as ParseLoc,
        ..RangeVar::default()
    };
    match parts.as_slice() {
        [rel] => range.relname = Some(rel.clone()),
        [schema, rel] => {
            range.schemaname = Some(schema.clone());
            range.relname = Some(rel.clone());
        }
        [catalog, schema, rel, ..] => {
            range.catalogname = Some(catalog.clone());
            range.schemaname = Some(schema.clone());
            range.relname = Some(rel.clone());
        }
        [] => {}
    }
    range
}

pub(crate) fn list_to_names(list: &[Node]) -> Vec<std::string::String> {
    list.iter()
        .filter_map(|node| match node {
            Node::String(value) => value.sval.clone(),
            _ => None,
        })
        .collect()
}
