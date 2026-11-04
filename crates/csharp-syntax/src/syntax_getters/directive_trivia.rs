//! `DirectiveTriviaSyntax`: the fields it declares (`super::fields`: `hash_token`, `end_of_directive_token`) on every
//! directive, and `DirectiveTriviaSyntax.cs`'s `DirectiveNameToken`.

pub use super::fields::directive_trivia::*;

use super::*;
use crate::syntax_kind::NodeKind;
use crate::tree::{NodeId, Tree};

/// `DirectiveTriviaSyntax.DirectiveNameToken` (DirectiveTriviaSyntax.cs:17): the directive's keyword (`if` of `#if`,
/// `pragma` of `#pragma warning`), a bad directive's identifier, a shebang's `!`, an ignored directive's `:`; None on
/// another node (where Roslyn throws).
pub fn directive_name_token(tree: &Tree, directive: NodeId) -> Option<NodeId> {
    match tree.node_kind(directive)? {
        NodeKind::IfDirectiveTrivia => if_directive_trivia::if_keyword(tree, directive),
        NodeKind::ElifDirectiveTrivia => elif_directive_trivia::elif_keyword(tree, directive),
        NodeKind::ElseDirectiveTrivia => else_directive_trivia::else_keyword(tree, directive),
        NodeKind::EndIfDirectiveTrivia => end_if_directive_trivia::end_if_keyword(tree, directive),
        NodeKind::RegionDirectiveTrivia => region_directive_trivia::region_keyword(tree, directive),
        NodeKind::EndRegionDirectiveTrivia => end_region_directive_trivia::end_region_keyword(tree, directive),
        NodeKind::ErrorDirectiveTrivia => error_directive_trivia::error_keyword(tree, directive),
        NodeKind::WarningDirectiveTrivia => warning_directive_trivia::warning_keyword(tree, directive),
        NodeKind::BadDirectiveTrivia => bad_directive_trivia::identifier(tree, directive),
        NodeKind::DefineDirectiveTrivia => define_directive_trivia::define_keyword(tree, directive),
        NodeKind::UndefDirectiveTrivia => undef_directive_trivia::undef_keyword(tree, directive),
        NodeKind::LineDirectiveTrivia => line_directive_trivia::line_keyword(tree, directive),
        NodeKind::LineSpanDirectiveTrivia => line_span_directive_trivia::line_keyword(tree, directive),
        NodeKind::PragmaWarningDirectiveTrivia => pragma_warning_directive_trivia::pragma_keyword(tree, directive),
        NodeKind::PragmaChecksumDirectiveTrivia => pragma_checksum_directive_trivia::pragma_keyword(tree, directive),
        NodeKind::ReferenceDirectiveTrivia => reference_directive_trivia::reference_keyword(tree, directive),
        NodeKind::LoadDirectiveTrivia => load_directive_trivia::load_keyword(tree, directive),
        NodeKind::ShebangDirectiveTrivia => shebang_directive_trivia::exclamation_token(tree, directive),
        NodeKind::IgnoredDirectiveTrivia => ignored_directive_trivia::colon_token(tree, directive),
        NodeKind::NullableDirectiveTrivia => nullable_directive_trivia::nullable_keyword(tree, directive),
        _ => None,
    }
}
