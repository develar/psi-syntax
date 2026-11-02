//! `JavaParserUtil.kt`: the helpers every Java parser shares (done with the comment binders, errors, expectations,
//! empty elements, the brace-matching and stopping builders).

use std::borrow::Cow;

use psi_builder::Marker;

use crate::builder::{
    PRECEDING_COMMENT_SET, SyntaxTreeBuilder, TRAILING_COMMENT_SET, get_preceding_comment_binder,
    trailing_comment_binder,
};
use crate::bundle::message;
use crate::language_level::LanguageLevel;
use crate::node_kind::NodeKind;
use crate::tokens::{ElementType, TokenKind, TokenSet};

/// `done(marker, type, languageLevel)`: `marker.done(type)`, then the preceding comment binder on the left of a
/// member, module or implicit class and the trailing one on the right of a statement, an import, the package statement
/// or a member.
#[inline(always)]
pub fn done(
    builder: &mut SyntaxTreeBuilder<'_>,
    marker: Marker,
    element_type: NodeKind,
    language_level: LanguageLevel,
) {
    marker.done(builder, element_type);
    let left = if PRECEDING_COMMENT_SET.contains(element_type) {
        Some(get_preceding_comment_binder(language_level))
    } else {
        None
    };

    let right = if TRAILING_COMMENT_SET.contains(element_type) { Some(trailing_comment_binder()) } else { None };

    marker.set_custom_edge_token_binders(builder, left, right);
}

/// `exprType(marker)`: the node type the marker was done with.
pub fn expr_type(builder: &SyntaxTreeBuilder<'_>, marker: Option<Marker>) -> Option<ElementType> {
    marker.and_then(|m| builder.marker_node_type(m))
}

/// `error(builder, message)`: used instead of `SyntaxTreeBuilder.error()` as it keeps all subsequent error messages.
#[cold]
pub fn error(builder: &mut SyntaxTreeBuilder<'_>, message: impl Into<Cow<'static, str>>) {
    builder.mark().error(builder, message);
}

/// `error(builder, message, before)`: an error before the marker `before`, or at the current token without one.
#[cold]
pub fn error_with(builder: &mut SyntaxTreeBuilder<'_>, message: impl Into<Cow<'static, str>>, before: Option<Marker>) {
    match before {
        None => error(builder, message),
        Some(before) => before.precede(builder).error_before(builder, message, before),
    }
}

/// `expectOrError(builder, expected: SyntaxElementTypeSet, key)`.
pub fn expect_or_error_set(builder: &mut SyntaxTreeBuilder<'_>, expected: TokenSet, key: &'static str) -> bool {
    if !builder.expect_set(expected) {
        error(builder, message(key));
        return false;
    }
    true
}

/// `expectOrError(builder, expected: SyntaxElementType?, key)`.
pub fn expect_or_error(builder: &mut SyntaxTreeBuilder<'_>, expected: Option<TokenKind>, key: &'static str) -> bool {
    if !builder.expect(expected) {
        error(builder, message(key));
        return false;
    }
    true
}

/// `emptyElement(builder, type)`.
pub fn empty_element(builder: &mut SyntaxTreeBuilder<'_>, element_type: NodeKind) {
    builder.mark().done(builder, element_type);
}

/// `emptyElement(before, type)`.
pub fn empty_element_before(builder: &mut SyntaxTreeBuilder<'_>, before: Marker, element_type: NodeKind) {
    before.precede(builder).done_before(builder, element_type, before);
}

/// `semicolon(builder)`.
pub fn semicolon(builder: &mut SyntaxTreeBuilder<'_>) {
    expect_or_error(builder, Some(TokenKind::SEMICOLON), "expected.semicolon");
}

/// `braceMatchingBuilder(builder)`: the builder stopping at the `}` that closes the block it is in (or at the end).
///
/// Not in the Kotlin: the Kotlin finds the `}` with a loop over the tokens (`braceCount` from 1, `{` +1, `}` -1, stop at
/// 0 or at `eof()`), which nested class bodies run once per level, quadratic; `brace_block_end_offset` gives the
/// offset that loop stops at in constant time. The marker is kept, so the cursor and the builder's state after the
/// rollback are the Kotlin's.
pub fn brace_matching_builder<'s>(builder: &'s mut SyntaxTreeBuilder<'_>) -> SyntaxTreeBuilder<'s> {
    let pos = builder.mark();

    let stop_at = builder.brace_block_end_offset();

    pos.rollback_to(builder);

    stopping_builder(builder, stop_at)
}

/// `stoppingBuilder(builder, stopAt)`: the builder whose `tokenType` is null and `eof()` true from the offset `stop_at`.
pub fn stopping_builder<'s>(builder: &'s mut SyntaxTreeBuilder<'_>, stop_at: i32) -> SyntaxTreeBuilder<'s> {
    builder.stopping_builder(stop_at)
}

// `stoppingBuilder(builder, condition)` (JavaParserUtil.kt:133) is not ported: nothing in intellij-community at
// decc943 calls it (the parsers call only `stoppingBuilder(builder, stopAt)`, from `braceMatchingBuilder` and
// `DeclarationParser.parse`), and its adapter would put a call of the condition on every `tokenType` and `eof()` of
// every builder (`SyntaxTreeBuilder` holds only the offset). Port it into `SyntaxTreeBuilder` with its first caller.

/// Not in the Kotlin: what a construct nested past a limit (`SyntaxTreeBuilder::enter_nesting`) parses as instead, an
/// error element "Code is too deeply nested" over the rest of it: from the current token to the first one that closes
/// a bracket opened before it, at least the current token (`skip_too_deeply_nested`). The construct around it then
/// closes as it would, and the error costs constant time, also in an attempt that is rolled back. Returns the error
/// element's marker, which the caller returns as the construct it parsed.
pub fn too_deeply_nested(builder: &mut SyntaxTreeBuilder<'_>) -> Marker {
    let err = builder.mark();
    builder.skip_too_deeply_nested();
    err.error(builder, crate::builder::TOO_DEEPLY_NESTED);
    err
}
