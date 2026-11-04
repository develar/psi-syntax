//! `getters_to_string`: the answer of every getter of `syntax_getters` and of `read::span` on every node of a tree, a
//! line each, as `tools/csharp-oracle/oracle.sh getters` prints Roslyn's own properties (`Getters.cs`,
//! `Fields.g.cs`), so that `ksdump --lang csharp --getters --check` compares the two and `tests/syntax_getters.rs`
//! runs every getter on every node of its inputs.
//!
//! A line is `<id> <getter> <answer>`, nodes in preorder (the oracle numbers the elements of its dump in the order it
//! prints them, nodes, tokens, trivia and structured trivia alike, which is our ids: the gate keeps the dumps equal);
//! tokens and trivia have no lines. Per node: `Span` (`read::span`, `s..e` in bytes); the fields of its node type in
//! Syntax.xml order and those of each abstract class it derives from, prefixed with the class
//! (`fields::print_fields`); then the computed properties of `syntax_getters` that apply to it, each prefixed with its
//! class. An answer is a node or token id, `-` for none (a null node, a `default` token), ids joined by `,` (`.` when
//! empty), a count, `true` or `false`, or a text in quotes escaped as the dump escapes it (`dump::escape`).

use std::fmt::Write;

use super::fields::print_fields;
use super::*;
use crate::syntax_kind::NodeKind;
use crate::tree::{NodeId, Tree};

/// Every line of a tree; empty for the tokens of a lexed text (`Parser::lex`), which have no nodes.
pub fn getters_to_string(tree: &Tree) -> String {
    let mut out = String::new();
    if tree.is_lexed() || tree.has_no_result() {
        return out;
    }
    let spans = crate::read::Spans::new(tree);
    let mut lines = Lines { out: &mut out, id: tree.root() };
    for id in tree.descendants(tree.root()) {
        let Some(kind) = tree.node_kind(id) else { continue };
        lines.id = id;
        let span = spans.span(tree, id);
        lines.raw("Span", format_args!("{}..{}", span.start, span.end));
        print_fields(tree, id, kind, &mut lines);
        computed(tree, id, kind, &mut lines);
    }
    out
}

/// The lines of the computed properties, in the order `tools/csharp-oracle/Getters.cs` prints them.
fn computed(tree: &Tree, id: NodeId, kind: NodeKind, out: &mut Lines) {
    if is_name_syntax(kind) {
        out.count("NameSyntax.Arity", name::arity(tree, id));
        out.one("NameSyntax.GetUnqualifiedName", name::get_unqualified_name(tree, id));
        out.text(
            "NameSyntax.GetAliasQualifierOpt",
            name::get_alias_qualifier_opt(tree, id).and_then(|t| tree.value_text(t)),
        );
        out.text("NameSyntax.ErrorDisplayName", name::error_display_name(tree, id).as_deref());
    }
    if kind == NodeKind::GenericName {
        out.bool("GenericNameSyntax.IsUnboundGenericName", generic_name::is_unbound_generic_name(tree, id));
    }
    if is_type_declaration_syntax(kind) {
        out.count("TypeDeclarationSyntax.Arity", type_declaration::arity(tree, id));
        out.one(
            "TypeDeclarationSyntax.PrimaryConstructorBaseTypeIfClass",
            type_declaration::primary_constructor_base_type_if_class(tree, id),
        );
    }
    if kind == NodeKind::MethodDeclaration {
        out.count("MethodDeclarationSyntax.Arity", method_declaration::arity(tree, id));
    }
    if kind == NodeKind::DelegateDeclaration {
        out.count("DelegateDeclarationSyntax.Arity", delegate_declaration::arity(tree, id));
    }
    if kind == NodeKind::Parameter {
        out.bool("ParameterSyntax.IsArgList", parameter::is_arg_list(tree, id));
    }
    if kind == NodeKind::ParameterList {
        out.count("ParameterListSyntax.ParameterCount", parameter_list::parameter_count(tree, id));
    }
    if kind == NodeKind::ArrayRankSpecifier {
        out.count("ArrayRankSpecifierSyntax.Rank", array_rank_specifier::rank(tree, id));
    }
    if is_anonymous_function_expression_syntax(kind) {
        out.one("AnonymousFunctionExpressionSyntax.Body", anonymous_function_expression::body(tree, id));
        out.one(
            "AnonymousFunctionExpressionSyntax.AsyncKeyword",
            anonymous_function_expression::async_keyword(tree, id),
        );
    }
    if kind == NodeKind::LocalDeclarationStatement {
        out.bool("LocalDeclarationStatementSyntax.IsConst", local_declaration_statement::is_const(tree, id));
    }
    if kind == NodeKind::UsingDirective {
        out.one("UsingDirectiveSyntax.Name", using_directive::name(tree, id));
    }
    if kind == NodeKind::Subpattern {
        out.one("SubpatternSyntax.NameColon", subpattern::name_colon(tree, id));
    }
    if is_type_syntax(kind) {
        out.bool("TypeSyntax.IsVar", type_::is_var(tree, id));
        out.bool("TypeSyntax.IsUnmanaged", type_::is_unmanaged(tree, id));
        out.bool("TypeSyntax.IsNotNull", type_::is_not_null(tree, id));
        out.bool("TypeSyntax.IsNint", type_::is_nint(tree, id));
        out.bool("TypeSyntax.IsNuint", type_::is_nuint(tree, id));
    }
    if is_directive_trivia_syntax(kind) {
        out.one("DirectiveTriviaSyntax.DirectiveNameToken", directive_trivia::directive_name_token(tree, id));
    }
}

/// The lines of one node.
pub(super) struct Lines<'o> {
    out: &'o mut String,
    id: NodeId,
}

impl Lines<'_> {
    fn raw(&mut self, getter: &str, answer: std::fmt::Arguments) {
        let _ = writeln!(self.out, "{} {getter} {answer}", self.id.0);
    }

    /// A token or node: its id, `-` for none.
    pub(super) fn one(&mut self, getter: &str, node: Option<NodeId>) {
        match node {
            Some(n) => self.raw(getter, format_args!("{}", n.0)),
            None => self.raw(getter, format_args!("-")),
        }
    }

    /// A list: the ids joined by `,`, `.` when empty.
    pub(super) fn list(&mut self, getter: &str, nodes: impl Iterator<Item = NodeId>) {
        let _ = write!(self.out, "{} {getter} ", self.id.0);
        let mut empty = true;
        for n in nodes {
            if !empty {
                self.out.push(',');
            }
            let _ = write!(self.out, "{}", n.0);
            empty = false;
        }
        if empty {
            self.out.push('.');
        }
        self.out.push('\n');
    }

    fn count(&mut self, getter: &str, value: usize) {
        self.raw(getter, format_args!("{value}"));
    }

    fn bool(&mut self, getter: &str, value: bool) {
        self.raw(getter, format_args!("{value}"));
    }

    /// A text in quotes, escaped as the dump escapes token texts; `-` for none.
    fn text(&mut self, getter: &str, text: Option<&str>) {
        match text {
            Some(text) => {
                let mut quoted = String::with_capacity(text.len() + 2);
                quoted.push('\'');
                crate::dump::escape(text, &mut quoted);
                quoted.push('\'');
                self.raw(getter, format_args!("{quoted}"));
            }
            None => self.raw(getter, format_args!("-")),
        }
    }
}
