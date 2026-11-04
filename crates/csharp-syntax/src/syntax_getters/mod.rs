//! `csharp_syntax::syntax_getters`: the properties of Roslyn's syntax nodes (`Microsoft.CodeAnalysis.CSharp.Syntax`,
//! the classes `Syntax/Syntax.xml` generates and their hand-written partials) as free functions over the tree and a
//! node's `NodeId`, so that a consumer asks Roslyn's question instead of walking `NodeKind`s: what a type declaration's
//! identifier is, a method's return type, a using directive's alias, a qualified name's right side.
//!
//! - A module per node type and per abstract class with fields, a function per field in snake_case (`fields`, generated
//!   from Syntax.xml by `tools/csharp-gen/gen.py`): `class_declaration::identifier` is `ClassDeclarationSyntax.Identifier`,
//!   `type_declaration::members` is `TypeDeclarationSyntax.Members` on a class, struct, record, interface or extension
//!   block, `member_declaration::modifiers` on any member, `base_method_declaration::parameter_list`,
//!   `base_property_declaration::type_`, `base_namespace_declaration::name` (block and file-scoped). A token or node
//!   field is `Option<NodeId>`: None where Roslyn's slot is null and on a node of another type (where C# would not
//!   compile, or a cast would fail), `Some` for a missing token (an empty leaf, `tree.is_missing`); a list is an
//!   iterator of its children (`Elements`), a separated list's without its separators.
//! - `is_<abstract class>` (`is_type_syntax`, `is_member_declaration_syntax`): whether a kind's node type derives from
//!   the class.
//! - `<field>_of` (`modifiers_of`, `identifier_of`, `type_parameter_list_of`, `body_of`, ...): the field on a node of
//!   any type that has one, as the interfaces of Java's PSI are dispatched on the element type (`has_modifier`).
//! - The computed properties of Roslyn's partial classes, each with its source line: `name` (`NameSyntax.Arity`,
//!   `GetUnqualifiedName`, `GetAliasQualifierOpt`, `ErrorDisplayName`), `generic_name::is_unbound_generic_name`,
//!   `type_declaration::arity` and `primary_constructor_base_type_if_class`, `method_declaration::arity`,
//!   `delegate_declaration::arity`, `parameter::is_arg_list`, `parameter_list::parameter_count`,
//!   `array_rank_specifier::rank`, `anonymous_function_expression::body` and `async_keyword`,
//!   `local_declaration_statement::is_const`, `using_directive::name`, `subpattern::name_colon`, `type_::is_var` and
//!   its siblings, `directive_trivia::directive_name_token`.
//!
//! Each answers Roslyn's red tree (`SyntaxNode` properties, no semantic model), reads the node's children only up to
//! its field (a type of 30 000 members is not scanned for its identifier), allocates nothing but the texts of
//! `name::error_display_name`, recurses nowhere and panics on no tree. `getters` prints every answer on every node, as
//! `tools/csharp-oracle/oracle.sh getters` prints Roslyn's own; `ksdump --lang csharp --getters --check` compares the
//! two (AGENTS.md, "C#"). Boolean fields of Syntax.xml (a directive's `IsActive`, `BranchTaken`, `ConditionValue`)
//! are not in the tree and have no getter.

mod fields;
pub mod getters;
mod slots;

pub use fields::*;
pub use slots::{Elements, SlotChildren};

pub mod anonymous_function_expression;
pub mod array_rank_specifier;
pub mod delegate_declaration;
pub mod directive_trivia;
pub mod generic_name;
pub mod local_declaration_statement;
pub mod method_declaration;
pub mod name;
pub mod parameter;
pub mod parameter_list;
pub mod subpattern;
pub mod type_;
pub mod type_declaration;
pub mod using_directive;

use crate::syntax_kind::TokenKind;
use crate::tree::{NodeId, Tree};

/// The children of node `id` that are no trivia, each with the slot of its node type it fills (an index into
/// `syntax::node_type(kind).slots`, `syntax::types::X::FIELD`; `usize::MAX` past them), in order; none for a leaf.
pub fn slot_children(tree: &Tree, id: NodeId) -> impl Iterator<Item = (usize, NodeId)> + '_ {
    tree.node_kind(id).map(|kind| SlotChildren::of_type(tree, id, slots::node_type_index(kind))).into_iter().flatten()
}

/// The child in slot `slot` of node `id` of any type (`syntax::types::ClassDeclarationSyntax::IDENTIFIER`), the first
/// of a list; None when the slot is null.
pub fn slot(tree: &Tree, id: NodeId, slot: usize) -> Option<NodeId> {
    slots::slot_children(tree, id, slot).next()
}

/// Every child in slot `slot` of node `id`, in order: a token list's tokens, a list's nodes, a separated list's
/// elements and separators.
pub fn slot_list(tree: &Tree, id: NodeId, slot: usize) -> Elements<'_> {
    slots::slot_children(tree, id, slot)
}

/// Whether the node's `Modifiers` (`modifiers_of`: a member, a parameter, an accessor, a local declaration or function,
/// a lambda) hold a token of `kind`: `SyntaxTokenList.Any(kind)` (`Modifiers.Any(SyntaxKind.StaticKeyword)`), the
/// `this` of an extension method's first parameter (`has_modifier(tree, parameter, TokenKind::ThisKeyword)`).
pub fn has_modifier(tree: &Tree, id: NodeId, kind: TokenKind) -> bool {
    modifiers_of(tree, id).any(|m| tree.token_kind(m) == Some(kind))
}

/// The first `Modifiers` token of `kind` (`SyntaxTokenList.FirstOrDefault(kind)`), None when there is none.
pub fn first_modifier(tree: &Tree, id: NodeId, kind: TokenKind) -> Option<NodeId> {
    modifiers_of(tree, id).find(|&m| tree.token_kind(m) == Some(kind))
}
