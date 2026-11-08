//! PSI accessors and Bazel plugin rules shared by Starlark consumers.

use std::ops::Range;

use crate::{NodeId, NodeKind, TokenKind, Tree, string};

/// A BUILD target declared by a top-level rule call with a non-empty literal `name` argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleTarget {
    pub name: String,
    pub name_at: usize,
    pub rule: Option<String>,
    pub range: Range<usize>,
}

/// The first child of `node` accepted by `accepts`.
pub fn first_child(tree: &Tree, node: NodeId, mut accepts: impl FnMut(NodeId) -> bool) -> Option<NodeId> {
    let mut child = tree.first_child(node);
    while let Some(current) = child {
        if accepts(current) {
            return Some(current);
        }
        child = tree.next_sibling(current);
    }
    None
}

/// The first child of `node` that is a PSI node rather than a token.
pub fn first_node_child(tree: &Tree, node: NodeId) -> Option<NodeId> {
    first_child(tree, node, |child| tree.token_kind(child).is_none())
}

/// The direct identifier token of a PSI node.
pub fn direct_identifier(tree: &Tree, node: NodeId) -> Option<NodeId> {
    first_child(tree, node, |child| tree.token_kind(child) == Some(TokenKind::IDENTIFIER))
}

/// The direct token of `kind` in a PSI node.
pub fn direct_token(tree: &Tree, node: NodeId, kind: TokenKind) -> Option<NodeId> {
    first_child(tree, node, |child| tree.token_kind(child) == Some(kind))
}

/// The next sibling of `token` that is a PSI node rather than a token.
pub fn value_after(tree: &Tree, token: NodeId) -> Option<NodeId> {
    let mut child = tree.next_sibling(token);
    while let Some(current) = child {
        if tree.token_kind(current).is_none() {
            return Some(current);
        }
        child = tree.next_sibling(current);
    }
    None
}

/// The assignment whose left side contains `target`; `None` when the name is read or is on the right side.
pub fn assignment_of_target(tree: &Tree, target: NodeId) -> Option<NodeId> {
    let mut current = target;
    while let Some(parent) = tree.parent(current) {
        match tree.node_kind(parent) {
            Some(
                NodeKind::TUPLE_EXPRESSION | NodeKind::LIST_LITERAL_EXPRESSION | NodeKind::PARENTHESIZED_EXPRESSION,
            ) => {
                current = parent;
            }
            Some(NodeKind::ASSIGNMENT_STATEMENT | NodeKind::AUG_ASSIGNMENT_STATEMENT) => {
                let operator = assignment_operator(tree, parent)?;
                return (tree.range(target).start < tree.range(operator).start).then_some(parent);
            }
            _ => return None,
        }
    }
    None
}

/// The final assignment operator in an assignment statement.
pub fn assignment_operator(tree: &Tree, assignment: NodeId) -> Option<NodeId> {
    let mut result = None;
    first_child(tree, assignment, |child| {
        if matches!(
            tree.token_kind(child),
            Some(
                TokenKind::EQ
                    | TokenKind::PLUSEQ
                    | TokenKind::MINUSEQ
                    | TokenKind::MULTEQ
                    | TokenKind::DIVEQ
                    | TokenKind::FLOORDIVEQ
                    | TokenKind::PERCEQ
            )
        ) {
            result = Some(child);
        }
        false
    });
    result
}

/// The expression iterated by the `for` statement or comprehension containing `target`.
pub fn loop_iterable(tree: &Tree, target: NodeId) -> Option<NodeId> {
    let mut parent = tree.parent(target);
    while let Some(current) = parent {
        if matches!(
            tree.node_kind(current),
            Some(
                NodeKind::FOR_STATEMENT
                    | NodeKind::LIST_COMP_EXPRESSION
                    | NodeKind::DICT_COMP_EXPRESSION
                    | NodeKind::GENERATOR_EXPRESSION
            )
        ) {
            let in_token = direct_token(tree, current, TokenKind::IN_KEYWORD)?;
            if tree.range(target).start >= tree.range(in_token).start {
                return None;
            }
            return value_after(tree, in_token);
        }
        parent = tree.parent(current);
    }
    None
}

/// The decoded string literal and the byte range of its contents within the file.
pub fn string_value(tree: &Tree, node: NodeId) -> Option<(String, Range<usize>)> {
    let literal =
        tree.descendants(node).find(|&child| tree.node_kind(child) == Some(NodeKind::STRING_LITERAL_EXPRESSION))?;
    let raw = std::str::from_utf8(tree.text(literal)).ok()?;
    let contents = string::contents_range(raw);
    let start = tree.range(literal).start;
    Some((string::value(raw), start + contents.start..start + contents.end))
}

/// The decoded label and its content range in a `load` statement.
pub fn load_label(tree: &Tree, load: NodeId) -> Option<(String, Range<usize>)> {
    if tree.node_kind(load) != Some(NodeKind::LOAD_STATEMENT) {
        return None;
    }
    let filename = first_child(tree, load, |child| tree.node_kind(child) == Some(NodeKind::FILENAME_LOAD_VALUE))?;
    string_value(tree, filename)
}

/// The nodes that bind names imported by a `load` statement.
pub fn loaded_values(tree: &Tree, load: NodeId) -> Vec<NodeId> {
    if tree.node_kind(load) != Some(NodeKind::LOAD_STATEMENT) {
        return Vec::new();
    }
    tree.children(load)
        .filter(|&child| {
            matches!(tree.node_kind(child), Some(NodeKind::STRING_LOAD_VALUE | NodeKind::NAMED_LOAD_VALUE))
        })
        .collect()
}

/// Whether `node` is part of a `load` statement directly under the file root.
pub fn load_is_top_level(tree: &Tree, node: NodeId) -> bool {
    let root = tree.root();
    let mut current = Some(node);
    while let Some(parent) = current {
        if tree.node_kind(parent) == Some(NodeKind::LOAD_STATEMENT) {
            return tree.parent(parent) == Some(root);
        }
        current = tree.parent(parent);
    }
    false
}

/// Whether `call` is an expression statement at file scope.
pub fn is_bazel_file_top_level_call(tree: &Tree, call: NodeId) -> bool {
    if tree.node_kind(call) != Some(NodeKind::CALL_EXPRESSION) {
        return false;
    }
    let Some(statement) = tree.parent(call) else { return false };
    tree.node_kind(statement) == Some(NodeKind::EXPRESSION_STATEMENT) && tree.parent(statement) == Some(tree.root())
}

/// The target declared by a top-level call, if it has a non-empty literal `name` argument.
pub fn rule_target(tree: &Tree, call: NodeId) -> Option<RuleTarget> {
    // `StarlarkCallExpression.isBazelFileTopLevelCall` and `isRuleTarget` in the Bazel plugin.
    if !is_bazel_file_top_level_call(tree, call) {
        return None;
    }
    let arguments = first_child(tree, call, |child| tree.node_kind(child) == Some(NodeKind::ARGUMENT_LIST))?;
    let name_argument = tree.children(arguments).find(|&argument| {
        tree.node_kind(argument) == Some(NodeKind::NAMED_ARGUMENT_EXPRESSION)
            && direct_identifier(tree, argument).is_some_and(|name| tree.text(name) == b"name")
    })?;
    let equal = direct_token(tree, name_argument, TokenKind::EQ)?;
    let value = value_after(tree, equal)?;
    if tree.node_kind(value) != Some(NodeKind::STRING_LITERAL_EXPRESSION) {
        return None;
    }
    let (name, name_range) = string_value(tree, value)?;
    if name.is_empty() {
        return None;
    }
    let callee =
        first_node_child(tree, call).filter(|&node| tree.node_kind(node) == Some(NodeKind::REFERENCE_EXPRESSION));
    Some(RuleTarget {
        name,
        name_at: name_range.start,
        rule: callee.map(|node| String::from_utf8_lossy(tree.text(node)).into_owned()),
        range: tree.range(call),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        NodeId, assignment_of_target, direct_identifier, is_bazel_file_top_level_call, load_is_top_level, load_label,
        loaded_values, loop_iterable, rule_target, string_value,
    };
    use crate::{FileKind, NodeKind, Parser, Tree};

    fn parse(source: &str) -> Tree {
        Parser::new().parse(source, FileKind::Build)
    }

    fn nodes_of_kind(tree: &Tree, kind: NodeKind) -> Vec<NodeId> {
        tree.descendants(tree.root()).filter(|&node| tree.node_kind(node) == Some(kind)).collect()
    }

    #[test]
    fn rule_targets_match_the_bazel_plugin_top_level_literal_name_rule() {
        let source = "jvm_library(name = \"monolith\")\nnative.cc_library(name = \"x\")\ngenrule(name = \"gen_\" + SUFFIX)\nexports_files([\"a.txt\"])\ndef macro():\n    nested(name = \"inside\")\n";
        let tree = parse(source);
        let calls = nodes_of_kind(&tree, NodeKind::CALL_EXPRESSION);
        let targets: Vec<_> = calls.iter().copied().filter_map(|call| rule_target(&tree, call)).collect();
        assert_eq!(calls.iter().filter(|&&call| is_bazel_file_top_level_call(&tree, call)).count(), 4);
        assert_eq!(targets.iter().map(|target| target.name.as_str()).collect::<Vec<_>>(), ["monolith", "x"]);
        assert_eq!(targets[0].rule.as_deref(), Some("jvm_library"));
        assert_eq!(targets[1].rule.as_deref(), Some("native.cc_library"));
        assert_eq!(&source[targets[0].name_at..targets[0].name_at + targets[0].name.len()], "monolith");
        assert!(targets.iter().all(|target| source[target.range.clone()].contains("name =")));
    }

    #[test]
    fn load_accessors_find_labels_values_and_top_level_statements() {
        let tree = parse(
            "load(\"//defs:rules.bzl\", \"simple\", macro = \"remote\")\ndef f():\n    load(\":local.bzl\", \"inside\")\n",
        );
        let loads = nodes_of_kind(&tree, NodeKind::LOAD_STATEMENT);
        assert_eq!(loads.len(), 2);
        assert_eq!(load_label(&tree, loads[0]).map(|(label, _)| label), Some("//defs:rules.bzl".to_owned()));
        assert_eq!(load_label(&tree, loads[1]).map(|(label, _)| label), Some(":local.bzl".to_owned()));
        assert!(load_is_top_level(&tree, loads[0]));
        assert!(!load_is_top_level(&tree, loads[1]));
        let values = loaded_values(&tree, loads[0]);
        assert_eq!(values.len(), 2);
        assert_eq!(string_value(&tree, values[0]).map(|(name, _)| name), Some("simple".to_owned()));
        assert_eq!(direct_identifier(&tree, values[1]).map(|name| tree.text(name)), Some(&b"macro"[..]));
        assert_eq!(string_value(&tree, values[1]).map(|(name, _)| name), Some("remote".to_owned()));
    }

    #[test]
    fn assignment_and_loop_accessors_distinguish_writes_from_reads() {
        let tree = parse("a, (b, c) = pair\nfor item in items:\n    pass\n");
        let targets: Vec<_> = nodes_of_kind(&tree, NodeKind::TARGET_EXPRESSION)
            .into_iter()
            .filter(|&node| direct_identifier(&tree, node).is_some())
            .collect();
        assert_eq!(targets.iter().filter(|&&node| assignment_of_target(&tree, node).is_some()).count(), 3);
        let item = targets.iter().find(|&&node| tree.text(node) == b"item").copied().unwrap();
        assert_eq!(tree.text(loop_iterable(&tree, item).unwrap()), b"items");
    }
}
