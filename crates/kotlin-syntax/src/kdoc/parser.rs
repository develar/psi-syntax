//! The ports of `KDocParser` (sections and tags of a doc comment) and `KDocLinkParser` (the qualified name of a
//! markdown link, lexed with the Kotlin lexer).

use crate::builder::PsiBuilderImpl;
use crate::kdoc::lexer::find_known_tag;
use crate::node_kind::NodeKind::*;
use crate::tokens::TokenKind::*;
use crate::tokens::{ElementType, KEYWORDS, TokenKind};

/// `KDocParser.parse(root, builder)`.
pub(crate) fn parse(root: ElementType, builder: &mut PsiBuilderImpl) {
    let root_marker = builder.mark();
    if builder.get_token_type() == KDOC_START {
        builder.advance_lexer();
    }
    let mut current_section_marker = Some(builder.mark());

    // todo: parse KDoc tags, markdown, etc...
    while !builder.eof() {
        if builder.get_token_type() == KDOC_TAG_NAME {
            current_section_marker = parse_tag(builder, current_section_marker);
        } else if builder.get_token_type() == KDOC_END {
            if let Some(marker) = current_section_marker.take() {
                marker.done(builder, KDOC_SECTION);
            }
            builder.advance_lexer();
        } else {
            builder.advance_lexer();
        }
    }

    if let Some(marker) = current_section_marker {
        marker.done(builder, KDOC_SECTION);
    }
    root_marker.done(builder, root);
}

/// `KDocParser.parseTag(builder, currentSectionMarker)`.
fn parse_tag(
    builder: &mut PsiBuilderImpl,
    mut current_section_marker: Option<crate::builder::Marker>,
) -> Option<crate::builder::Marker> {
    let known_tag = builder.get_token_bytes().and_then(find_known_tag);
    if let Some((_, is_section_start)) = known_tag
        && is_section_start
    {
        // Java dereferences the marker unconditionally; it is never null here (a tag never follows the comment end).
        if let Some(marker) = current_section_marker {
            marker.done(builder, KDOC_SECTION);
        }
        current_section_marker = Some(builder.mark());
    }
    let tag_start = builder.mark();
    builder.advance_lexer();

    while !builder.eof() && !is_at_end_of_tag(builder) {
        builder.advance_lexer();
    }
    tag_start.done(builder, KDOC_TAG);
    current_section_marker
}

/// `KDocParser.isAtEndOfTag(builder)`.
fn is_at_end_of_tag(builder: &mut PsiBuilderImpl) -> bool {
    if builder.get_token_type() == KDOC_END {
        return true;
    }
    if builder.get_token_type() == KDOC_LEADING_ASTERISK {
        let mut look_ahead_count = 1;
        if builder.look_ahead(1) == KDOC_TEXT {
            look_ahead_count += 1;
        }
        if builder.look_ahead(look_ahead_count) == KDOC_TAG_NAME {
            return true;
        }
    }
    false
}

/// `KDocLinkParser.parse(root, builder)`.
pub(crate) fn parse_markdown_link(root: ElementType, builder: &mut PsiBuilderImpl) {
    let root_marker = builder.mark();
    let has_l_bracket = builder.get_token_type() == LBRACKET;
    if has_l_bracket {
        builder.advance_lexer();
    }
    parse_qualified_name(builder);
    if has_l_bracket {
        if !builder.eof() && builder.get_token_type() != RBRACKET {
            builder.error("Closing bracket expected");
            while !builder.eof() && builder.get_token_type() != RBRACKET {
                builder.advance_lexer();
            }
        }
        if builder.get_token_type() == RBRACKET {
            builder.advance_lexer();
        }
    } else if !builder.eof() {
        builder.error("Expression expected");
        while !builder.eof() {
            builder.advance_lexer();
        }
    }
    root_marker.done(builder, root);
}

/// `KDocLinkParser.parseQualifiedName(builder)`.
fn parse_qualified_name(builder: &mut PsiBuilderImpl) {
    let mut marker = builder.mark();
    loop {
        // don't highlight a word in a link as an error if it happens to be a Kotlin keyword
        if !is_name(builder.get_token_type()) {
            marker.drop(builder);
            builder.error("Identifier expected");
            break;
        }
        builder.advance_lexer();
        marker.done(builder, KDOC_NAME);
        if builder.get_token_type() != DOT {
            break;
        }
        marker = marker.precede(builder);
        builder.advance_lexer();
    }
}

/// `KDocLinkParser.isName(tokenType)`.
fn is_name(token_type: Option<TokenKind>) -> bool {
    token_type == IDENTIFIER || KEYWORDS.contains(token_type)
}
