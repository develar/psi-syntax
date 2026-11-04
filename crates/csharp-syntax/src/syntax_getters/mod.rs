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
//! - `slot_of`, `slot_info`, `slot`, `slot_list`, `slot_children`: the slots themselves; `is_in_type_slot`: whether C#
//!   reads a node as a type where it stands.
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

use crate::syntax::Slot;
use crate::syntax::types::{
    AliasQualifiedNameSyntax, BinaryExpressionSyntax, BreakStatementSyntax, ContinueStatementSyntax,
    FileScopedNamespaceDeclarationSyntax, MemberAccessExpressionSyntax, MemberBindingExpressionSyntax, NameColonSyntax,
    NameEqualsSyntax, NameMemberCrefSyntax, NamespaceDeclarationSyntax, QualifiedNameSyntax, UsingDirectiveSyntax,
    XmlNameAttributeSyntax,
};
use crate::syntax_kind::{NodeKind, TokenKind};
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

/// The slot of its parent's node type that `child` fills (an index into `syntax::node_type(kind).slots`), None for the
/// root, trivia and a child no slot takes. Reads the parent's children up to `child`.
pub fn slot_of(tree: &Tree, child: NodeId) -> Option<usize> {
    let parent = tree.parent(child)?;
    slot_children(tree, parent).find(|&(_, c)| c == child).map(|(slot, _)| slot).filter(|&slot| slot != usize::MAX)
}

/// The slot `child` fills, as Syntax.xml declares it: its field's name, shape and declared type
/// (`Slot::type_name`: `TypeSyntax` of a parameter's type, `SimpleNameSyntax` of a member access's name).
pub fn slot_info(tree: &Tree, child: NodeId) -> Option<&'static Slot> {
    let slot = slot_of(tree, child)?;
    crate::syntax::node_type(tree.node_kind(tree.parent(child)?)?).slots.get(slot)
}

/// Whether C# reads the node `child` as a type where it stands: it fills a slot whose declared type derives from
/// `TypeSyntax` (a parameter's, property's, variable's or method's type, a cast's type, a base type, a type argument,
/// `typeof`'s, `new`'s, a pattern's, a catch's, a foreach's type, an attribute's name, an explicit interface, a type
/// parameter of a `where` clause, a cref's type), or it is the right side of `x is T` or `x as T`, which Syntax.xml
/// declares an `ExpressionSyntax` while the parser puts a type there and Roslyn's binder reads one (`BindIsOperator`,
/// `BindAsOperator`). Not a type, though declared a `TypeSyntax` subclass: a member access's or member binding's name
/// (`x.Foo`), an argument's or alias's name (`f(a: 1)`, `using A = …`'s `A`), an alias qualifier (`global`), a
/// `break`'s or `continue`'s label, a documentation comment's `name` attribute, a namespace's name, a qualified name's
/// left side (a qualifier: a namespace or a type, found by lookup), a cref's member name, and the target of a using
/// directive but `using static` (a namespace, or with an alias a namespace or a type). A qualified name's right side
/// and an alias qualified name's name answer as the name around them (`System.Int32 x;`'s `Int32`). False for a token,
/// trivia and the root.
pub fn is_in_type_slot(tree: &Tree, child: NodeId) -> bool {
    let mut child = child;
    loop {
        if tree.is_leaf(child) {
            return false;
        }
        let (Some(parent), Some(slot)) = (tree.parent(child), slot_of(tree, child)) else { return false };
        let Some(kind) = tree.node_kind(parent) else { return false };
        match kind {
            // the name around them decides
            NodeKind::QualifiedName if slot == QualifiedNameSyntax::RIGHT => child = parent,
            NodeKind::AliasQualifiedName if slot == AliasQualifiedNameSyntax::NAME => child = parent,
            NodeKind::IsExpression | NodeKind::AsExpression => return slot == BinaryExpressionSyntax::RIGHT,
            NodeKind::UsingDirective if slot == UsingDirectiveSyntax::NAMESPACE_OR_TYPE => {
                return using_directive::static_keyword(tree, parent).is_some();
            }
            _ => {
                let not_a_type = match kind {
                    NodeKind::QualifiedName => slot == QualifiedNameSyntax::LEFT,
                    NodeKind::AliasQualifiedName => slot == AliasQualifiedNameSyntax::ALIAS,
                    NodeKind::SimpleMemberAccessExpression | NodeKind::PointerMemberAccessExpression => {
                        slot == MemberAccessExpressionSyntax::NAME
                    }
                    NodeKind::MemberBindingExpression => slot == MemberBindingExpressionSyntax::NAME,
                    NodeKind::NameColon => slot == NameColonSyntax::NAME,
                    NodeKind::NameEquals => slot == NameEqualsSyntax::NAME,
                    NodeKind::BreakStatement => slot == BreakStatementSyntax::NAME,
                    NodeKind::ContinueStatement => slot == ContinueStatementSyntax::NAME,
                    NodeKind::XmlNameAttribute => slot == XmlNameAttributeSyntax::IDENTIFIER,
                    NodeKind::NamespaceDeclaration => slot == NamespaceDeclarationSyntax::NAME,
                    NodeKind::FileScopedNamespaceDeclaration => slot == FileScopedNamespaceDeclarationSyntax::NAME,
                    NodeKind::NameMemberCref => slot == NameMemberCrefSyntax::NAME,
                    _ => false,
                };
                return !not_a_type && fields::TYPE_SLOTS[slots::node_type_index(kind) as usize] & (1 << slot) != 0;
            }
        }
    }
}
