//! `NameSyntax` (`Syntax/NameSyntax.cs`) and the overrides of its subclasses (`SimpleNameSyntax.cs`,
//! `QualifiedNameSyntax.cs`, `AliasedQualifiedNameSyntax.cs`, `IdentifierNameSyntax.cs`, `GenericNameSyntax.cs`): on a
//! node of a type deriving from it (`super::is_name_syntax`: `IdentifierName`, `GenericName`, `QualifiedName`,
//! `AliasQualifiedName`), None (0) on any other. Roslyn nests `A.B.C` left-deep, a level a part: these loop, so a name
//! of 100 000 parts costs its parts, no stack.

use super::{alias_qualified_name, generic_name, identifier_name, is_name_syntax, qualified_name, type_argument_list};
use crate::syntax_kind::NodeKind;
use crate::tree::{NodeId, Tree};

/// `NameSyntax.Arity` (NameSyntax.cs:13): a generic name's type argument count, 0 for any other name.
pub fn arity(tree: &Tree, name: NodeId) -> usize {
    match tree.node_kind(name) {
        Some(NodeKind::GenericName) => generic_name::type_argument_list(tree, name)
            .map_or(0, |list| type_argument_list::arguments(tree, list).count()),
        _ => 0,
    }
}

/// `NameSyntax.GetUnqualifiedName()` (NameSyntax.cs:29): a simple name itself (SimpleNameSyntax.cs:17), a qualified
/// name's `Right` (QualifiedNameSyntax.cs:17), an alias qualified name's `Name` (AliasedQualifiedNameSyntax.cs:12).
pub fn get_unqualified_name(tree: &Tree, name: NodeId) -> Option<NodeId> {
    match tree.node_kind(name)? {
        NodeKind::IdentifierName | NodeKind::GenericName => Some(name),
        NodeKind::QualifiedName => qualified_name::right(tree, name),
        NodeKind::AliasQualifiedName => alias_qualified_name::name(tree, name),
        _ => None,
    }
}

/// `NameSyntax.GetAliasQualifierOpt()` (NameSyntax.cs:41) as the identifier token whose `ValueText` it returns: down
/// the `Left` of qualified names to an alias qualified name, its `Alias`'s identifier (`global` of `global::A.B`, `E`
/// of `E::A`); None for a name without one.
pub fn get_alias_qualifier_opt(tree: &Tree, name: NodeId) -> Option<NodeId> {
    let mut name = name;
    loop {
        match tree.node_kind(name)? {
            NodeKind::QualifiedName => name = qualified_name::left(tree, name)?,
            NodeKind::AliasQualifiedName => {
                return alias_qualified_name::alias(tree, name)
                    .and_then(|alias| identifier_name::identifier(tree, alias));
            }
            _ => return None,
        }
    }
}

/// `NameSyntax.ErrorDisplayName()` (NameSyntax.cs:34), the name as Roslyn's diagnostics print it: an identifier's
/// `ValueText` (IdentifierNameSyntax.cs:9), a generic name's with `<`, a `,` per type argument after the first and `>`
/// (`List<,>`, GenericNameSyntax.cs:23), a qualified name's sides joined by `.` (QualifiedNameSyntax.cs:22), an alias
/// qualified name's by `::` (AliasedQualifiedNameSyntax.cs:17); no trivia, no type arguments. None for another node and
/// for an identifier whose text is not UTF-8.
pub fn error_display_name(tree: &Tree, name: NodeId) -> Option<String> {
    if !tree.node_kind(name).is_some_and(is_name_syntax) {
        return None;
    }
    let mut out = String::new();
    // the parts left to write, right to left: a name, or the separator before the part written next
    let mut pending = vec![Part::Name(name)];
    while let Some(part) = pending.pop() {
        let name = match part {
            Part::Text(text) => {
                out.push_str(text);
                continue;
            }
            Part::Name(name) => name,
        };
        match tree.node_kind(name) {
            Some(NodeKind::IdentifierName) => {
                out.push_str(identifier_value(tree, identifier_name::identifier(tree, name))?)
            }
            Some(NodeKind::GenericName) => {
                out.push_str(identifier_value(tree, generic_name::identifier(tree, name))?);
                out.push('<');
                for _ in 1..arity(tree, name) {
                    out.push(',');
                }
                out.push('>');
            }
            Some(NodeKind::QualifiedName) => {
                pending.extend(qualified_name::right(tree, name).map(Part::Name));
                pending.push(Part::Text("."));
                pending.extend(qualified_name::left(tree, name).map(Part::Name));
            }
            Some(NodeKind::AliasQualifiedName) => {
                pending.extend(alias_qualified_name::name(tree, name).map(Part::Name));
                pending.push(Part::Text("::"));
                pending.extend(alias_qualified_name::alias(tree, name).map(Part::Name));
            }
            _ => {}
        }
    }
    Some(out)
}

enum Part {
    Name(NodeId),
    Text(&'static str),
}

/// An identifier token's `ValueText`, "" for a name without one.
fn identifier_value(tree: &Tree, identifier: Option<NodeId>) -> Option<&str> {
    match identifier {
        Some(token) => tree.value_text(token),
        None => Some(""),
    }
}
