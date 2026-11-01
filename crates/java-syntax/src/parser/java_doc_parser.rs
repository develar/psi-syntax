//! `JavaDocParser.kt`: the inside of a doc comment (HTML and markdown), and the reference and type holders in it.
#![allow(unused_variables, unused_imports, dead_code, clippy::too_many_arguments)] // the skeleton's stubs: remove with the last `TODO(port)`

use std::borrow::Cow;

use psi_builder::Marker;

use crate::builder::SyntaxTreeBuilder;
use crate::builder::is_eol_token;
use crate::language_level::LanguageLevel;
use crate::node_kind::NodeKind;
use crate::parser::java_parser::JavaParser;
use crate::tokens::{ElementType, TokenKind, TokenSet};

/// `JavaDocParser(builder, languageLevel)`: holds the builder it parses with and its brace state.
pub struct JavaDocParser<'a, 'b> {
    pub builder: &'a mut SyntaxTreeBuilder<'b>,
    pub language_level: LanguageLevel,
    brace_scope: i32,
    /// true for all '{' opening an inline tag
    closing_status_list: Vec<bool>,
}

impl<'a, 'b> JavaDocParser<'a, 'b> {
    /// `JavaDocParser(builder, languageLevel)`.
    pub fn new(builder: &'a mut SyntaxTreeBuilder<'b>, language_level: LanguageLevel) -> Self {
        JavaDocParser { builder, language_level, brace_scope: 0, closing_status_list: Vec::new() }
    }

    /// `parseJavadocReference(parser)` (JavaDocParser.kt:43).
    pub fn parse_javadoc_reference(&mut self, parser: &JavaParser) {
        // TODO(port): JavaDocParser.kt:43
        todo!("JavaDocParser.parseJavadocReference (JavaDocParser.kt:43)")
    }

    /// `parseJavadocType(parser)` (JavaDocParser.kt:48).
    pub fn parse_javadoc_type(&mut self, parser: &JavaParser) {
        // TODO(port): JavaDocParser.kt:48
        todo!("JavaDocParser.parseJavadocType (JavaDocParser.kt:48)")
    }

    /// `swallowTokens()` (JavaDocParser.kt:53).
    fn swallow_tokens(&mut self) {
        // TODO(port): JavaDocParser.kt:53
        todo!("JavaDocParser.swallowTokens (JavaDocParser.kt:53)")
    }

    /// `parseDocCommentText()` (JavaDocParser.kt:57).
    pub fn parse_doc_comment_text(&mut self) {
        // TODO(port): JavaDocParser.kt:57
        todo!("JavaDocParser.parseDocCommentText (JavaDocParser.kt:57)")
    }

    /// `parseTag()` (JavaDocParser.kt:71).
    fn parse_tag(&mut self) {
        // TODO(port): JavaDocParser.kt:71
        todo!("JavaDocParser.parseTag (JavaDocParser.kt:71)")
    }

    /// `parseDataItem(tagName, isInline)` (JavaDocParser.kt:83).
    fn parse_data_item(&mut self, tag_name: Option<&[u8]>, is_inline: bool) {
        // TODO(port): JavaDocParser.kt:83
        todo!("JavaDocParser.parseDataItem (JavaDocParser.kt:83)")
    }

    /// `parseCommentData()` (JavaDocParser.kt:182).
    fn parse_comment_data(&mut self) {
        // TODO(port): JavaDocParser.kt:182
        todo!("JavaDocParser.parseCommentData (JavaDocParser.kt:182)")
    }

    /// `parseInlineCodeBlock()` (JavaDocParser.kt:208).
    fn parse_inline_code_block(&mut self) {
        // TODO(port): JavaDocParser.kt:208
        todo!("JavaDocParser.parseInlineCodeBlock (JavaDocParser.kt:208)")
    }

    /// `parseCodeBlock()` (JavaDocParser.kt:230).
    fn parse_code_block(&mut self) {
        // TODO(port): JavaDocParser.kt:230
        todo!("JavaDocParser.parseCodeBlock (JavaDocParser.kt:230)")
    }

    /// `parseMarkdownReferenceOrLink()` (JavaDocParser.kt:257).
    fn parse_markdown_reference_or_link(&mut self) {
        // TODO(port): JavaDocParser.kt:257
        todo!("JavaDocParser.parseMarkdownReferenceOrLink (JavaDocParser.kt:257)")
    }

    /// `parseMarkdownReference()` (JavaDocParser.kt:362).
    fn parse_markdown_reference(&mut self) -> bool {
        // TODO(port): JavaDocParser.kt:362
        todo!("JavaDocParser.parseMarkdownReference (JavaDocParser.kt:362)")
    }

    /// `parseMarkdownLink()` (JavaDocParser.kt:493).
    fn parse_markdown_link(&mut self) {
        // TODO(port): JavaDocParser.kt:493
        todo!("JavaDocParser.parseMarkdownLink (JavaDocParser.kt:493)")
    }

    /// `findInlineToken(needle)` (JavaDocParser.kt:529).
    fn find_inline_token(&mut self, needle: Option<TokenKind>) -> Option<TokenKind> {
        // TODO(port): JavaDocParser.kt:529
        todo!("JavaDocParser.findInlineToken (JavaDocParser.kt:529)")
    }

    /// `findInlineToken(needle, travelToken, isTravelTokenDisallowed)` (JavaDocParser.kt:543).
    fn find_inline_token_with(
        &mut self,
        needle: Option<TokenKind>,
        travel_token: Option<TokenKind>,
        is_travel_token_disallowed: bool,
    ) -> Option<TokenKind> {
        // TODO(port): JavaDocParser.kt:543
        todo!("JavaDocParser.findInlineToken (JavaDocParser.kt:543)")
    }

    /// `parseSnippetTagValue()` (JavaDocParser.kt:571).
    fn parse_snippet_tag_value(&mut self) {
        // TODO(port): JavaDocParser.kt:571
        todo!("JavaDocParser.parseSnippetTagValue (JavaDocParser.kt:571)")
    }

    /// `parseSnippetTagBody()` (JavaDocParser.kt:607).
    fn parse_snippet_tag_body(&mut self) {
        // TODO(port): JavaDocParser.kt:607
        todo!("JavaDocParser.parseSnippetTagBody (JavaDocParser.kt:607)")
    }

    /// `parseSnippetAttributeList()` (JavaDocParser.kt:622).
    fn parse_snippet_attribute_list(&mut self) {
        // TODO(port): JavaDocParser.kt:622
        todo!("JavaDocParser.parseSnippetAttributeList (JavaDocParser.kt:622)")
    }

    /// `parseSnippetAttribute()` (JavaDocParser.kt:639).
    fn parse_snippet_attribute(&mut self) {
        // TODO(port): JavaDocParser.kt:639
        todo!("JavaDocParser.parseSnippetAttribute (JavaDocParser.kt:639)")
    }

    /// `parseSeeTagValue(allowBareFieldReference)` (JavaDocParser.kt:671).
    fn parse_see_tag_value(&mut self, allow_bare_field_reference: bool) {
        // TODO(port): JavaDocParser.kt:671
        todo!("JavaDocParser.parseSeeTagValue (JavaDocParser.kt:671)")
    }

    /// `parseMaybeGenericType(isReferenceHolder)` (JavaDocParser.kt:712).
    fn parse_maybe_generic_type(&mut self, is_reference_holder: bool) {
        // TODO(port): JavaDocParser.kt:712
        todo!("JavaDocParser.parseMaybeGenericType (JavaDocParser.kt:712)")
    }

    /// `parseModuleRef(refStart)` (JavaDocParser.kt:751).
    fn parse_module_ref(&mut self, ref_start: Marker) -> Option<Marker> {
        // TODO(port): JavaDocParser.kt:751
        todo!("JavaDocParser.parseModuleRef (JavaDocParser.kt:751)")
    }

    /// `parseFragmentRef(mark)` (JavaDocParser.kt:769).
    fn parse_fragment_ref(&mut self, mark: Marker) {
        // TODO(port): JavaDocParser.kt:769
        todo!("JavaDocParser.parseFragmentRef (JavaDocParser.kt:769)")
    }

    /// `parseMethodRef(refStart)` (JavaDocParser.kt:782).
    fn parse_method_ref(&mut self, ref_start: Marker) {
        // TODO(port): JavaDocParser.kt:782
        todo!("JavaDocParser.parseMethodRef (JavaDocParser.kt:782)")
    }

    /// `parseParameterRef()` (JavaDocParser.kt:828).
    fn parse_parameter_ref(&mut self) {
        // TODO(port): JavaDocParser.kt:828
        todo!("JavaDocParser.parseParameterRef (JavaDocParser.kt:828)")
    }

    /// `parseSimpleTagValue()` (JavaDocParser.kt:834).
    fn parse_simple_tag_value(&mut self) {
        // TODO(port): JavaDocParser.kt:834
        todo!("JavaDocParser.parseSimpleTagValue (JavaDocParser.kt:834)")
    }

    /// `fakeCollapse(marker, tokenType)` (JavaDocParser.kt:853).
    fn fake_collapse(&mut self, marker: Marker, token_type: TokenKind) {
        // TODO(port): JavaDocParser.kt:853
        todo!("JavaDocParser.fakeCollapse (JavaDocParser.kt:853)")
    }

    /// `getTokenType(skipWhitespace)` with `skip_whitespace` at its default, true (JavaDocParser.kt:888).
    fn get_token_type(&mut self) -> Option<TokenKind> {
        self.get_token_type_with(true)
    }

    /// `getTokenType(skipWhitespace)` (JavaDocParser.kt:888).
    fn get_token_type_with(&mut self, skip_whitespace: bool) -> Option<TokenKind> {
        // TODO(port): JavaDocParser.kt:888
        todo!("JavaDocParser.getTokenType (JavaDocParser.kt:888)")
    }

    /// `getBraceScope()` (JavaDocParser.kt:897).
    fn get_brace_scope(&mut self) -> i32 {
        // TODO(port): JavaDocParser.kt:897
        todo!("JavaDocParser.getBraceScope (JavaDocParser.kt:897)")
    }

    /// `setBraceScope(braceScope)` (JavaDocParser.kt:901).
    fn set_brace_scope(&mut self, brace_scope: i32) {
        // TODO(port): JavaDocParser.kt:901
        todo!("JavaDocParser.setBraceScope (JavaDocParser.kt:901)")
    }

    /// `remapAndAdvance()` (JavaDocParser.kt:905).
    fn remap_and_advance(&mut self) {
        // TODO(port): JavaDocParser.kt:905
        todo!("JavaDocParser.remapAndAdvance (JavaDocParser.kt:905)")
    }
}

/// `isWhiteSpace(type)`: DOC_SPACE or WHITE_SPACE.
fn is_white_space(element_type: Option<TokenKind>) -> bool {
    matches!(element_type, Some(TokenKind::DOC_SPACE | TokenKind::WHITE_SPACE))
}

/// `TAG_VALUES_SET`.
const TAG_VALUES_SET: TokenSet = TokenSet::create(&[
    TokenKind::DOC_TAG_VALUE_TOKEN,
    TokenKind::DOC_TAG_VALUE_COMMA,
    TokenKind::DOC_TAG_VALUE_DOT,
    TokenKind::DOC_TAG_VALUE_LPAREN,
    TokenKind::DOC_TAG_VALUE_RPAREN,
    TokenKind::DOC_TAG_VALUE_SLASH,
    TokenKind::DOC_TAG_VALUE_SHARP_TOKEN,
    TokenKind::DOC_TAG_VALUE_DOUBLE_SHARP_TOKEN,
    TokenKind::DOC_TAG_VALUE_LT,
    TokenKind::DOC_TAG_VALUE_GT,
    TokenKind::DOC_TAG_VALUE_COLON,
    TokenKind::DOC_TAG_VALUE_QUOTE,
]);

/// `INLINE_TAG_BORDERS_SET`.
const INLINE_TAG_BORDERS_SET: TokenSet =
    TokenSet::create(&[TokenKind::DOC_INLINE_TAG_START, TokenKind::DOC_INLINE_TAG_END]);

/// `SKIP_TOKENS`.
const SKIP_TOKENS: TokenSet = TokenSet::create(&[TokenKind::DOC_COMMENT_LEADING_ASTERISKS]);

/// `COMMENT_DATA_TOKENS`.
const COMMENT_DATA_TOKENS: TokenSet = TokenSet::create(&[
    TokenKind::DOC_COMMENT_DATA,
    TokenKind::DOC_TAG_VALUE_SLASH,
    TokenKind::DOC_COMMA,
    TokenKind::DOC_SHARP,
    TokenKind::DOC_DOUBLE_SHARP,
    TokenKind::DOC_LT,
    TokenKind::DOC_GT,
]);

/// `REFERENCE_LINK_EOC_EXPECTED_TOKENS`: the expected token at the caret for Markdown link `[Class<caret>#method()]`.
const REFERENCE_LINK_EOC_EXPECTED_TOKENS: TokenSet =
    TokenSet::create(&[TokenKind::DOC_RBRACKET, TokenKind::DOC_LT, TokenKind::DOC_SHARP, TokenKind::DOC_DOUBLE_SHARP]);

/// `DATA_IN_GENERICS_TOKENS`: the allowed base tokens when parsing generics.
const DATA_IN_GENERICS_TOKENS: TokenSet = TokenSet::create(&[
    TokenKind::DOC_TAG_VALUE_TOKEN,
    TokenKind::DOC_COMMENT_DATA,
    TokenKind::DOC_TAG_VALUE_COMMA,
    TokenKind::DOC_COMMA,
    TokenKind::DOC_SPACE,
    TokenKind::WHITE_SPACE,
]);
/// `LT_TOKENS`.
const LT_TOKENS: TokenSet = TokenSet::create(&[TokenKind::DOC_LT, TokenKind::DOC_TAG_VALUE_LT]);
/// `GT_TOKENS`.
const GT_TOKENS: TokenSet = TokenSet::create(&[TokenKind::DOC_GT, TokenKind::DOC_TAG_VALUE_GT]);

const SEE_TAG: &[u8] = b"@see";
const LINK_TAG: &[u8] = b"@link";
const LINK_PLAIN_TAG: &[u8] = b"@linkplain";
const PARAM_TAG: &[u8] = b"@param";
const VALUE_TAG: &[u8] = b"@value";
const SNIPPET_TAG: &[u8] = b"@snippet";
const INHERIT_DOC_TAG: &[u8] = b"@inheritDoc";

/// `REFERENCE_TAGS`.
const REFERENCE_TAGS: [&[u8]; 4] = [b"@throws", b"@exception", b"@provides", b"@uses"];
