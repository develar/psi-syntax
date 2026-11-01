//! `JavaDocParser.kt`: the inside of a doc comment (HTML and markdown), and the reference and type holders in it.

use psi_builder::Marker;

use crate::builder::{Binder, SyntaxTreeBuilder};
use crate::language_level::LanguageLevel;
use crate::node_kind::NodeKind;
use crate::parser::java_parser::JavaParser;
use crate::parser::java_parser_util::empty_element;
use crate::parser::reference_parser::ReferenceParser;
use crate::tokens::{TokenKind, TokenSet};

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
        parser.reference_parser().parse_java_code_reference(self.builder, true, true, false, true);
        self.swallow_tokens();
    }

    /// `parseJavadocType(parser)` (JavaDocParser.kt:48).
    pub fn parse_javadoc_type(&mut self, parser: &JavaParser) {
        parser.reference_parser().parse_type(
            self.builder,
            ReferenceParser::EAT_LAST_DOT | ReferenceParser::ELLIPSIS | ReferenceParser::WILDCARD,
        );
        self.swallow_tokens();
    }

    /// `swallowTokens()` (JavaDocParser.kt:53).
    fn swallow_tokens(&mut self) {
        while !self.builder.eof() {
            self.builder.advance_lexer();
        }
    }

    /// `parseDocCommentText()` (JavaDocParser.kt:57).
    pub fn parse_doc_comment_text(&mut self) {
        self.builder.enforce_comment_tokens(SKIP_TOKENS);

        while !self.builder.eof() {
            let token_type = self.get_token_type();
            if token_type == Some(TokenKind::DOC_TAG_NAME) {
                self.parse_tag();
            } else {
                self.parse_data_item(None, false);
            }
        }
    }

    /// `parseTag()` (JavaDocParser.kt:71).
    fn parse_tag(&mut self) {
        let tag_name = tag_name_of(self.builder.token_text());
        let tag = self.builder.mark();
        self.builder.advance_lexer();
        loop {
            let token_type = self.get_token_type();
            if token_type.is_none()
                || token_type == Some(TokenKind::DOC_TAG_NAME)
                || token_type == Some(TokenKind::DOC_COMMENT_END)
            {
                break;
            }
            self.parse_data_item(tag_name, false);
        }
        tag.done(self.builder, NodeKind::DOC_TAG);
    }

    /// `parseDataItem(tagName, isInline)` (JavaDocParser.kt:83). A tag name is compared only with the tags this parser
    /// knows, so it is passed as that tag's constant, or as "" for any other tag (`tag_name_of`).
    fn parse_data_item(&mut self, tag_name: Option<&[u8]>, is_inline: bool) {
        let mut token_type = self.get_token_type();
        if token_type == Some(TokenKind::DOC_INLINE_TAG_START) {
            let tag = self.builder.mark();
            self.builder.advance_lexer();

            token_type = self.get_token_type();
            if token_type != Some(TokenKind::DOC_TAG_NAME) && token_type != Some(TokenKind::DOC_COMMENT_BAD_CHARACTER) {
                tag.rollback_to(self.builder);
                self.builder.remap_current_token(TokenKind::DOC_COMMENT_DATA);
                self.builder.advance_lexer();
                self.closing_status_list.push(false);
                return;
            }

            self.closing_status_list.push(true);

            let mut inline_tag_name: Option<&'static [u8]> = Some(b"");
            let mut is_snippet = false;

            loop {
                token_type = self.get_token_type();
                if token_type == Some(TokenKind::DOC_TAG_NAME) {
                    inline_tag_name = tag_name_of(self.builder.token_text());
                    is_snippet = inline_tag_name == Some(SNIPPET_TAG);
                } else if token_type.is_none() || token_type == Some(TokenKind::DOC_COMMENT_END) {
                    break;
                }

                if token_type == Some(TokenKind::DOC_INLINE_TAG_END) {
                    // the list holds this tag's `true` at least (Kotlin's `removeAt(lastIndex)`)
                    let should_close = self.closing_status_list.pop().unwrap_or(false);
                    if should_close {
                        self.set_brace_scope(self.get_brace_scope() - 1);
                        self.builder.advance_lexer();
                        break;
                    } else {
                        self.builder.remap_current_token(TokenKind::DOC_COMMENT_DATA);
                        self.builder.advance_lexer();
                        continue;
                    }
                }

                self.parse_data_item(inline_tag_name, true);
            }

            if is_snippet {
                tag.done(self.builder, NodeKind::DOC_SNIPPET_TAG);
            } else {
                tag.done(self.builder, NodeKind::DOC_INLINE_TAG);
            }
        } else if TAG_VALUES_SET.contains(token_type) {
            if tag_name == Some(SEE_TAG) && !is_inline
                || tag_name == Some(LINK_TAG) && is_inline
                || self.language_level.is_at_least(LanguageLevel::JDK_1_4)
                    && tag_name == Some(LINK_PLAIN_TAG)
                    && is_inline
            {
                self.parse_see_tag_value(false);
            } else if !is_inline && tag_name.is_some_and(|name| REFERENCE_TAGS.contains(&name))
                || is_inline && tag_name == Some(INHERIT_DOC_TAG)
            {
                let tag_value = self.builder.mark();
                self.builder.remap_current_token(TokenKind::DOC_REFERENCE_HOLDER);
                self.builder.advance_lexer();
                tag_value.done(self.builder, NodeKind::DOC_TAG_VALUE_ELEMENT);
            } else if !is_inline && tag_name == Some(PARAM_TAG) {
                self.parse_parameter_ref();
            } else if self.language_level.is_at_least(LanguageLevel::JDK_1_5)
                && tag_name == Some(VALUE_TAG)
                && is_inline
            {
                self.parse_see_tag_value(true);
            } else if tag_name == Some(SNIPPET_TAG) && is_inline {
                self.parse_snippet_tag_value();
            } else {
                self.parse_simple_tag_value();
            }
        } else if token_type == Some(TokenKind::DOC_INLINE_CODE_FENCE) {
            self.parse_inline_code_block();
        } else if token_type == Some(TokenKind::DOC_CODE_FENCE) {
            self.parse_code_block();
        } else if token_type == Some(TokenKind::DOC_LBRACKET) {
            self.parse_markdown_reference_or_link();
        } else if COMMENT_DATA_TOKENS.contains(token_type)
            || (is_white_space(token_type) && !is_eol_token(token_type, self.builder.token_text()))
        {
            self.parse_comment_data();
        } else {
            self.remap_and_advance();
        }
    }

    /// `parseCommentData()` (JavaDocParser.kt:182).
    fn parse_comment_data(&mut self) {
        let comment_data = self.builder.mark();
        let offset = self.builder.current_offset();

        loop {
            let ahead_type = self.builder.raw_lookup(1);
            if ahead_type.is_none() {
                break;
            }
            let has_comment_token_next = COMMENT_DATA_TOKENS.contains(ahead_type);
            let has_white_space_token_next =
                is_white_space(ahead_type) && !is_eol_token(ahead_type, Some(self.builder.raw_token_text(1)));

            if has_comment_token_next || has_white_space_token_next {
                self.builder.raw_advance_lexer(1);
            } else {
                break;
            }
        }

        if self.builder.current_offset() != offset {
            self.builder.advance_lexer();
            comment_data.collapse(self.builder, TokenKind::DOC_COMMENT_DATA);
        } else {
            comment_data.drop(self.builder);
            self.builder.advance_lexer();
        }
    }

    /// `parseInlineCodeBlock()` (JavaDocParser.kt:208).
    fn parse_inline_code_block(&mut self) {
        let block_marker = self.builder.mark();
        self.builder.raw_advance_lexer(1);
        let content_marker = self.builder.mark();
        let stop_element_type = self.find_inline_token(Some(TokenKind::DOC_INLINE_CODE_FENCE));

        if stop_element_type != Some(TokenKind::DOC_INLINE_CODE_FENCE) {
            // Bail out, no end
            content_marker.drop(self.builder);
            block_marker.rollback_to(self.builder);
            self.builder.advance_lexer();
            return;
        }

        self.fake_collapse(content_marker, TokenKind::DOC_COMMENT_DATA);
        if !self.builder.eof() {
            self.builder.advance_lexer();
        }

        block_marker.done(self.builder, NodeKind::DOC_MARKDOWN_CODE_BLOCK);
    }

    /// `parseCodeBlock()` (JavaDocParser.kt:230).
    fn parse_code_block(&mut self) {
        if self.get_brace_scope() > 0 {
            self.builder.remap_current_token(TokenKind::DOC_COMMENT_DATA);
            self.builder.advance_lexer();
            return;
        }
        // Store the fence type, a fenced code block can only be closed by the same fence type
        let fence_start = self.builder.token_text().and_then(|text| text.first().copied());
        let tag = self.builder.mark();

        // Look for the nearest closing code fence, converting everything inside as comment data
        while !self.builder.eof() {
            self.builder.advance_lexer();
            if self.get_token_type() == Some(TokenKind::DOC_CODE_FENCE)
                && self.builder.token_text().and_then(|text| text.first().copied()) == fence_start
            {
                break;
            }
            self.builder.remap_current_token(TokenKind::DOC_COMMENT_DATA);
        }

        if !self.builder.eof() {
            self.builder.advance_lexer();
        }

        tag.done(self.builder, NodeKind::DOC_MARKDOWN_CODE_BLOCK);
    }

    /// `parseMarkdownReferenceOrLink()` (JavaDocParser.kt:257): ensure a link is good before parsing it.
    fn parse_markdown_reference_or_link(&mut self) {
        let mut has_label = true;
        let mut tag = self.builder.mark();

        // Step 1 ensure that we have a label
        let mut left_bracket_count = 1;
        let mut right_bracket_count = 0;
        let start_label_offset = self.builder.current_offset();

        // Labels are allowed balanced brackets, count each side
        while !self.builder.eof() {
            let token = self.find_inline_token_with(Some(TokenKind::DOC_RBRACKET), Some(TokenKind::DOC_LBRACKET), true);
            if token == Some(TokenKind::DOC_LBRACKET) {
                left_bracket_count += 1;
                continue;
            }
            if token == Some(TokenKind::DOC_RBRACKET) {
                right_bracket_count += 1;
                if left_bracket_count == right_bracket_count {
                    break;
                }
            }
        }

        let end_label_offset = self.builder.current_offset();
        let is_short_ref_empty = end_label_offset - start_label_offset <= 1;
        if left_bracket_count != right_bracket_count || is_short_ref_empty {
            // Stop if unbalanced brackets/empty reference
            tag.rollback_to(self.builder);
            self.builder.advance_lexer();
            return;
        }

        let first_reference_token =
            self.find_inline_token_with(Some(TokenKind::DOC_LBRACKET), Some(TokenKind::DOC_SPACE), false);
        if first_reference_token != Some(TokenKind::DOC_LBRACKET) {
            has_label = false;
            // The label may actually be a reference, verify brackets balance or if we have a normal Markdown link
            if left_bracket_count > 1 || first_reference_token == Some(TokenKind::DOC_LPAREN) {
                tag.rollback_to(self.builder);
                self.parse_markdown_link();
                return;
            }
        }

        // Step 2 get the reference for full reference link
        if has_label
            && self.find_inline_token_with(Some(TokenKind::DOC_RBRACKET), Some(TokenKind::DOC_LBRACKET), true)
                != Some(TokenKind::DOC_RBRACKET)
        {
            has_label = false;
            // The label is actually a reference, verify brackets balance
            if left_bracket_count > 1 {
                tag.rollback_to(self.builder);
                self.builder.advance_lexer();
                return;
            }
        }

        // Step 3, validity ensured, parse the link
        tag.rollback_to(self.builder);
        tag = self.builder.mark();
        if has_label {
            self.builder.advance_lexer();
            let label = self.builder.mark();
            // Collapses comment data within a label
            let mut comment_data_marker: Option<Marker> = None;
            // Label range already known, mark it as comment data
            while !self.builder.eof() {
                if self.builder.token_type() == Some(TokenKind::DOC_INLINE_CODE_FENCE) {
                    if let Some(it) = comment_data_marker.take() {
                        self.fake_collapse(it, TokenKind::DOC_COMMENT_DATA);
                    }
                    self.parse_inline_code_block();
                    continue;
                }

                if self.builder.current_offset() < end_label_offset {
                    if comment_data_marker.is_none() {
                        comment_data_marker = Some(self.builder.mark());
                    }
                    self.builder.advance_lexer();
                    continue;
                }
                break;
            }

            if let Some(it) = comment_data_marker {
                self.fake_collapse(it, TokenKind::DOC_COMMENT_DATA);
            }
            label.done(self.builder, NodeKind::DOC_MARKDOWN_REFERENCE_LABEL);
            self.builder.advance_lexer();
        }

        // Parse the reference itself
        self.builder.advance_lexer();
        if self.parse_markdown_reference() {
            self.builder.advance_lexer();
            tag.done(self.builder, NodeKind::DOC_MARKDOWN_REFERENCE_LINK);
        } else {
            tag.drop(self.builder);
        }
    }

    /// `parseMarkdownReference()` (JavaDocParser.kt:362): whether the reference managed to get parsed.
    fn parse_markdown_reference(&mut self) -> bool {
        let mark = self.builder.mark();
        let module_marker = self.parse_module_ref(mark);
        let mut reference_parsed = false;
        let mut reference_ended = false;
        let mut fragment_reference = false;

        if self.get_token_type() == Some(TokenKind::DOC_RBRACKET) {
            if module_marker.is_none() {
                return false;
            }
            reference_ended = true;
        }
        if let Some(module_marker) = module_marker {
            module_marker.done(self.builder, NodeKind::DOC_TAG_VALUE_ELEMENT);
        }
        let ref_start = self.builder.mark();
        let mut is_method_field_or_ref = false;
        if !reference_ended
            && self.get_token_type() != Some(TokenKind::DOC_SHARP)
            && self.get_token_type() != Some(TokenKind::DOC_DOUBLE_SHARP)
        {
            // Javadoc methods references may not have the # token if it is alone, rely on the existence of () to assign the proper type.
            // In practice, () is not mandatory, and fields have the same issue but cannot be separated from class names on parsing
            if self.builder.look_ahead(1) == Some(TokenKind::DOC_LPAREN) {
                is_method_field_or_ref = true;
            } else if !REFERENCE_LINK_EOC_EXPECTED_TOKENS.contains(self.builder.look_ahead(1)) {
                // Additional unexpected data: "[String unlawfulData]"
                ref_start.rollback_to(self.builder);
                return false;
            } else {
                self.parse_maybe_generic_type(true);
            }
        }

        if !reference_ended && !is_method_field_or_ref {
            is_method_field_or_ref = self.get_token_type() == Some(TokenKind::DOC_SHARP);
            if is_method_field_or_ref {
                // Existing integration require this token for auto completion
                self.builder.remap_current_token(TokenKind::DOC_TAG_VALUE_SHARP_TOKEN);

                self.builder.advance_lexer();
            }
        }

        if !reference_ended && is_method_field_or_ref {
            // method/variable name
            self.builder.remap_current_token(TokenKind::DOC_TAG_VALUE_TOKEN);

            // A method only has parenthesis, comment data which may be the type, the optional argument name and commas
            self.builder.advance_lexer();
            if self.builder.token_type() == Some(TokenKind::DOC_LPAREN) {
                self.builder.advance_lexer();
                let sub_value = self.builder.mark();

                // Only the first data element count as a type in links like [#foo(int arg1, int arg2)]
                let mut data_since_comma = false;
                let mut space_count = 0;

                while !self.builder.eof() {
                    let element_type = self.get_token_type();
                    if element_type == Some(TokenKind::DOC_COMMENT_DATA) {
                        if is_white_space(self.builder.raw_lookup(1)) {
                            space_count += 1;
                        }
                        if space_count >= 2 {
                            // Unexpected data: [#foo(int desc unexpectedData)]
                            sub_value.drop(self.builder);
                            self.fake_collapse(ref_start, TokenKind::DOC_COMMENT_DATA);
                            return false;
                        }

                        if !data_since_comma {
                            data_since_comma = true;
                            self.parse_maybe_generic_type(false);
                        } else {
                            self.builder.advance_lexer();
                        }
                    } else if element_type != Some(TokenKind::DOC_COMMA) {
                        break;
                    } else {
                        data_since_comma = false;
                        self.builder.advance_lexer();
                    }
                }

                if self.get_token_type() == Some(TokenKind::DOC_RPAREN) {
                    sub_value.done(self.builder, NodeKind::DOC_TAG_VALUE_ELEMENT);
                    self.builder.advance_lexer();
                } else {
                    sub_value.drop(self.builder);
                }
            }

            reference_parsed = true;
        } else if !reference_ended && self.get_token_type() == Some(TokenKind::DOC_DOUBLE_SHARP) {
            // auto-completion token
            self.builder.remap_current_token(TokenKind::DOC_TAG_VALUE_DOUBLE_SHARP_TOKEN);
            self.builder.advance_lexer();

            // fragment name
            let name_marker = self.builder.mark();

            if self.get_token_type() != Some(TokenKind::DOC_COMMENT_DATA) {
                name_marker.rollback_to(self.builder);
            } else {
                self.builder.advance_lexer();
                name_marker.done(self.builder, NodeKind::DOC_FRAGMENT_NAME);
                reference_parsed = true;
                fragment_reference = true;
            }
        }

        if reference_parsed {
            if fragment_reference {
                ref_start.done(self.builder, NodeKind::DOC_FRAGMENT_REF);
            } else {
                ref_start.done(self.builder, NodeKind::DOC_METHOD_OR_FIELD_REF);
            }
        } else {
            ref_start.drop(self.builder);
        }
        // This method is guaranteed the existence of an end bracket
        if self.get_token_type() != Some(TokenKind::DOC_RBRACKET) {
            self.find_inline_token(Some(TokenKind::DOC_RBRACKET));
        }

        true
    }

    /// `parseMarkdownLink()` (JavaDocParser.kt:493): a simple parse function that wraps a Markdown link in an element.
    fn parse_markdown_link(&mut self) {
        let fallback = self.builder.mark();
        if self.builder.token_type() != Some(TokenKind::DOC_LBRACKET) {
            fallback.rollback_to(self.builder);
            self.builder.advance_lexer();
            return;
        }
        self.builder.advance_lexer();
        let label = self.builder.mark();
        self.find_inline_token_with(Some(TokenKind::DOC_RBRACKET), Some(TokenKind::DOC_LBRACKET), true);
        self.fake_collapse(label, TokenKind::DOC_COMMENT_DATA);

        if self.builder.token_type() != Some(TokenKind::DOC_RBRACKET) {
            fallback.rollback_to(self.builder);
            self.builder.advance_lexer();
            return;
        }

        self.builder.advance_lexer();
        if self.builder.token_type() != Some(TokenKind::DOC_LPAREN) {
            fallback.rollback_to(self.builder);
            self.builder.advance_lexer();
            return;
        }

        self.builder.advance_lexer();
        let link = self.builder.mark();
        self.find_inline_token(Some(TokenKind::DOC_RPAREN));

        if self.builder.token_type() != Some(TokenKind::DOC_RPAREN) {
            fallback.rollback_to(self.builder);
            self.builder.advance_lexer();
            return;
        }

        self.fake_collapse(link, TokenKind::DOC_COMMENT_DATA);
        self.builder.advance_lexer();
        fallback.done(self.builder, NodeKind::DOC_MARKDOWN_LINK);
    }

    /// `findInlineToken(needle)` (JavaDocParser.kt:529).
    fn find_inline_token(&mut self, needle: Option<TokenKind>) -> Option<TokenKind> {
        self.find_inline_token_with(needle, None, false)
    }

    /// `findInlineToken(needle, travelToken, isTravelTokenDisallowed)` (JavaDocParser.kt:543): looks for `needle`,
    /// taking into account markdown line break rules. `travel_token` is the token either allowed or disallowed on the
    /// way to `needle` (`None`: no check); with `is_travel_token_disallowed` the `travel_token` aborts the search,
    /// without it anything but `travel_token` or `needle` does. Returns the last token encountered during the search.
    fn find_inline_token_with(
        &mut self,
        needle: Option<TokenKind>,
        travel_token: Option<TokenKind>,
        is_travel_token_disallowed: bool,
    ) -> Option<TokenKind> {
        let mut token: Option<TokenKind> = None;
        let mut previous_token: Option<TokenKind>;
        while !self.builder.eof() {
            self.builder.advance_lexer();
            previous_token = token;
            token = self.get_token_type_with(false);
            if token == needle {
                return token;
            }
            let travel_token_found = travel_token == token;
            if travel_token.is_some()
                && ((is_travel_token_disallowed && travel_token_found)
                    || (!is_travel_token_disallowed && !travel_token_found))
            {
                break;
            }

            // Markdown specific, check for EOL
            if token == Some(TokenKind::WHITE_SPACE) && previous_token == Some(TokenKind::WHITE_SPACE) {
                break;
            }
        }

        if self.builder.eof() { None } else { token }
    }

    /// `parseSnippetTagValue()` (JavaDocParser.kt:571).
    fn parse_snippet_tag_value(&mut self) {
        // we are right after @snippet
        let snippet_value = self.builder.mark();
        snippet_value.set_custom_edge_token_binders(
            self.builder,
            Some(Binder::DefaultRight),
            Some(Binder::DefaultLeft),
        );

        // recovery, when "foo" goes right after @snippet
        loop {
            let token = self.get_token_type();
            if token != Some(TokenKind::DOC_TAG_VALUE_QUOTE) {
                break;
            }
            self.builder.advance_lexer();
        }

        let token_type = self.get_token_type();
        if token_type == Some(TokenKind::DOC_TAG_VALUE_COLON) {
            empty_element(self.builder, NodeKind::DOC_SNIPPET_ATTRIBUTE_LIST);
            self.parse_snippet_tag_body();
        } else if token_type == Some(TokenKind::DOC_TAG_VALUE_TOKEN) {
            self.parse_snippet_attribute_list();
            if self.builder.token_type() == Some(TokenKind::DOC_TAG_VALUE_COLON) {
                self.parse_snippet_tag_body();
            }
        } else {
            empty_element(self.builder, NodeKind::DOC_SNIPPET_ATTRIBUTE_LIST);
            let mut current = self.get_token_type();
            while current.is_some() && current != Some(TokenKind::DOC_INLINE_TAG_END) {
                self.builder.advance_lexer();
                current = self.get_token_type();
            }
        }
        snippet_value.done(self.builder, NodeKind::DOC_SNIPPET_TAG_VALUE);
    }

    /// `parseSnippetTagBody()` (JavaDocParser.kt:607). The Kotlin `require`s the colon both callers are at; here a
    /// debug assertion.
    fn parse_snippet_tag_body(&mut self) {
        let body = self.builder.mark();
        body.set_custom_edge_token_binders(self.builder, Some(Binder::DefaultRight), Some(Binder::DefaultLeft));
        let colon = self.get_token_type();
        debug_assert_eq!(colon, Some(TokenKind::DOC_TAG_VALUE_COLON));
        self.builder.advance_lexer();
        loop {
            let token_type = self.get_token_type();
            if token_type.is_none() || token_type == Some(TokenKind::DOC_INLINE_TAG_END) {
                break;
            }
            self.builder.advance_lexer();
        }
        body.done(self.builder, NodeKind::DOC_SNIPPET_BODY);
    }

    /// `parseSnippetAttributeList()` (JavaDocParser.kt:622).
    fn parse_snippet_attribute_list(&mut self) {
        let attribute_list = self.builder.mark();
        'outer: loop {
            let mut element_type = self.get_token_type();
            while element_type != Some(TokenKind::DOC_TAG_VALUE_TOKEN) {
                // recovery
                if element_type != Some(TokenKind::DOC_TAG_VALUE_QUOTE) {
                    break 'outer;
                }
                self.builder.advance_lexer();
                element_type = self.get_token_type();
            }
            self.parse_snippet_attribute();
        }
        attribute_list.done(self.builder, NodeKind::DOC_SNIPPET_ATTRIBUTE_LIST);
    }

    /// `parseSnippetAttribute()` (JavaDocParser.kt:639). The Kotlin `require`s the DOC_TAG_VALUE_TOKEN its caller is
    /// at; here a debug assertion. `remapCurrentToken(DOC_SNIPPET_ATTRIBUTE_VALUE); advanceLexer()` (a composite type
    /// remapped onto one token, which `TokenKind` cannot hold) is the one-token collapse that makes the same leaf.
    fn parse_snippet_attribute(&mut self) {
        let attribute = self.builder.mark();
        let name = self.builder.token_type();
        debug_assert_eq!(name, Some(TokenKind::DOC_TAG_VALUE_TOKEN));
        self.builder.remap_current_token(TokenKind::DOC_TAG_ATTRIBUTE_NAME);
        self.builder.advance_lexer();
        self.get_token_type(); // skipping spaces
        if self.builder.token_text() == Some(b"=") {
            self.builder.advance_lexer();
            let after_eq_token = self.get_token_type();
            if after_eq_token == Some(TokenKind::DOC_TAG_VALUE_QUOTE) {
                let quoted_value = self.builder.mark();
                self.builder.advance_lexer();
                if self.get_token_type() == Some(TokenKind::DOC_TAG_VALUE_TOKEN) {
                    self.builder.advance_lexer();
                }
                if self.get_token_type() == Some(TokenKind::DOC_TAG_VALUE_QUOTE) {
                    self.builder.advance_lexer();
                }
                quoted_value.collapse(self.builder, NodeKind::DOC_SNIPPET_ATTRIBUTE_VALUE);
            } else if after_eq_token == Some(TokenKind::DOC_TAG_VALUE_TOKEN) {
                let value = self.builder.mark();
                self.builder.advance_lexer();
                value.collapse(self.builder, NodeKind::DOC_SNIPPET_ATTRIBUTE_VALUE);
            }
        }
        attribute.done(self.builder, NodeKind::DOC_SNIPPET_ATTRIBUTE);
    }

    /// `parseSeeTagValue(allowBareFieldReference)` (JavaDocParser.kt:671): the reference inside a tag (like `@link`
    /// and `@see`); with `allow_bare_field_reference` bare references are **always** method or field references.
    fn parse_see_tag_value(&mut self, allow_bare_field_reference: bool) {
        let mark = self.builder.mark();
        let module_marker = self.parse_module_ref(mark);

        let token_type = self.get_token_type();
        if token_type == Some(TokenKind::DOC_TAG_VALUE_SHARP_TOKEN) {
            let ref_start = self.builder.mark();
            self.parse_method_ref(ref_start);
        } else if token_type == Some(TokenKind::DOC_TAG_VALUE_DOUBLE_SHARP_TOKEN) {
            let ref_start = self.builder.mark();
            self.parse_fragment_ref(ref_start);
        } else if token_type == Some(TokenKind::DOC_TAG_VALUE_TOKEN) {
            let ref_start = self.builder.mark();
            self.parse_maybe_generic_type(true);

            if self.get_token_type() == Some(TokenKind::DOC_TAG_VALUE_SHARP_TOKEN) {
                self.parse_method_ref(ref_start);
            } else if self.get_token_type() == Some(TokenKind::DOC_TAG_VALUE_DOUBLE_SHARP_TOKEN) {
                self.parse_fragment_ref(ref_start);
            }
            // Javadoc methods references may not have the # token if it is alone, rely on the existence of () to assign the proper type.
            // In practice, () is not mandatory, and fields have the same issue but cannot be separated from class names on parsing
            else if allow_bare_field_reference || self.get_token_type() == Some(TokenKind::DOC_TAG_VALUE_LPAREN) {
                ref_start.rollback_to(self.builder);
                self.builder.remap_current_token(TokenKind::DOC_TAG_VALUE_TOKEN);
                let ref_start = self.builder.mark();
                self.parse_method_ref(ref_start);
            } else {
                ref_start.drop(self.builder);
            }
        } else if module_marker.is_none() {
            let tag_value = self.builder.mark();
            self.builder.advance_lexer();
            tag_value.done(self.builder, NodeKind::DOC_TAG_VALUE_ELEMENT);
        }

        if let Some(module_marker) = module_marker {
            module_marker.done(self.builder, NodeKind::DOC_TAG_VALUE_ELEMENT);
        }
    }

    /// `parseMaybeGenericType(isReferenceHolder)` (JavaDocParser.kt:712): a class with a potential generic type
    /// attached to it (e.g. `List<String>`, `Map<String, String>`).
    fn parse_maybe_generic_type(&mut self, is_reference_holder: bool) {
        let ref_start = self.builder.mark();
        let new_element =
            if is_reference_holder { TokenKind::DOC_REFERENCE_HOLDER } else { TokenKind::DOC_TYPE_HOLDER };

        if self.get_token_type() != Some(TokenKind::DOC_TAG_VALUE_TOKEN)
            && self.get_token_type() != Some(TokenKind::DOC_COMMENT_DATA)
        {
            ref_start.rollback_to(self.builder);
            return;
        }

        self.builder.remap_current_token(new_element);
        self.builder.advance_lexer();

        if LT_TOKENS.contains(self.get_token_type()) {
            let mut nesting = 1;
            loop {
                let token_type = self.builder.token_type();
                if is_eol_token(token_type, self.builder.token_text()) {
                    break;
                }
                self.builder.advance_lexer();
                if DATA_IN_GENERICS_TOKENS.contains(self.builder.token_type()) {
                    continue;
                }
                if LT_TOKENS.contains(self.builder.token_type()) {
                    nesting += 1;
                    continue;
                }
                if GT_TOKENS.contains(self.builder.token_type()) {
                    nesting -= 1;
                    if nesting == 0 {
                        self.builder.advance_lexer();
                        break;
                    }
                    continue;
                }

                // Disallowed token, break regardless of the nesting
                break;
            }
        }

        ref_start.collapse(self.builder, new_element);
    }

    /// `parseModuleRef(refStart)` (JavaDocParser.kt:751).
    fn parse_module_ref(&mut self, ref_start: Marker) -> Option<Marker> {
        self.builder.advance_lexer();
        if self.get_token_type() == Some(TokenKind::DOC_TAG_VALUE_SLASH) {
            ref_start.rollback_to(self.builder);
            let container_ref = self.builder.mark();
            let module_ref = self.builder.mark();
            self.builder.advance_lexer();
            module_ref.done(self.builder, NodeKind::MODULE_REFERENCE);
            self.builder.advance_lexer();
            Some(container_ref)
        } else {
            ref_start.rollback_to(self.builder);
            None
        }
    }

    /// `parseFragmentRef(mark)` (JavaDocParser.kt:769).
    fn parse_fragment_ref(&mut self, mark: Marker) {
        self.builder.advance_lexer();
        let fragment_name = self.builder.mark();
        if self.get_token_type() != Some(TokenKind::DOC_TAG_VALUE_TOKEN) {
            fragment_name.drop(self.builder);
            mark.done(self.builder, NodeKind::DOC_FRAGMENT_REF);
        } else {
            self.builder.advance_lexer();
            fragment_name.done(self.builder, NodeKind::DOC_FRAGMENT_NAME);
            mark.done(self.builder, NodeKind::DOC_FRAGMENT_REF);
        }
    }

    /// `parseMethodRef(refStart)` (JavaDocParser.kt:782).
    fn parse_method_ref(&mut self, ref_start: Marker) {
        if self.get_token_type() == Some(TokenKind::DOC_TAG_VALUE_SHARP_TOKEN) {
            self.builder.advance_lexer();
        }
        if self.get_token_type() != Some(TokenKind::DOC_TAG_VALUE_TOKEN) {
            ref_start.done(self.builder, NodeKind::DOC_METHOD_OR_FIELD_REF);
            return;
        }
        self.builder.advance_lexer();

        if self.get_token_type() == Some(TokenKind::DOC_TAG_VALUE_LPAREN) {
            self.builder.advance_lexer();

            let sub_value = self.builder.mark();

            loop {
                let token_type = self.get_token_type();
                if !TAG_VALUES_SET.contains(token_type) {
                    break;
                }
                match token_type {
                    Some(TokenKind::DOC_TAG_VALUE_TOKEN) => {
                        self.parse_maybe_generic_type(false);

                        loop {
                            let token_type = self.get_token_type();
                            if !(TAG_VALUES_SET.contains(token_type)
                                && token_type != Some(TokenKind::DOC_TAG_VALUE_COMMA)
                                && token_type != Some(TokenKind::DOC_TAG_VALUE_RPAREN))
                            {
                                break;
                            }
                            self.builder.advance_lexer();
                        }
                    }
                    Some(TokenKind::DOC_TAG_VALUE_RPAREN) => {
                        sub_value.done(self.builder, NodeKind::DOC_TAG_VALUE_ELEMENT);
                        self.builder.advance_lexer();
                        ref_start.done(self.builder, NodeKind::DOC_METHOD_OR_FIELD_REF);
                        return;
                    }
                    _ => {
                        self.builder.advance_lexer();
                    }
                }
            }

            sub_value.done(self.builder, NodeKind::DOC_TAG_VALUE_ELEMENT);
        }

        ref_start.done(self.builder, NodeKind::DOC_METHOD_OR_FIELD_REF);
    }

    /// `parseParameterRef()` (JavaDocParser.kt:828).
    fn parse_parameter_ref(&mut self) {
        let tag_value = self.builder.mark();
        while TAG_VALUES_SET.contains(self.get_token_type()) {
            self.builder.advance_lexer();
        }
        tag_value.done(self.builder, NodeKind::DOC_PARAMETER_REF);
    }

    /// `parseSimpleTagValue()` (JavaDocParser.kt:834).
    fn parse_simple_tag_value(&mut self) {
        let tag_data = self.builder.mark();
        loop {
            let token_type = self.get_token_type();
            if token_type == Some(TokenKind::DOC_COMMENT_BAD_CHARACTER) {
                self.builder.remap_current_token(TokenKind::DOC_TAG_VALUE_TOKEN);
            } else if !TAG_VALUES_SET.contains(token_type) {
                break;
            }
            self.builder.advance_lexer();
        }
        tag_data.done(self.builder, NodeKind::DOC_TAG_VALUE_ELEMENT);
    }

    /// `fakeCollapse(marker, tokenType)` (JavaDocParser.kt:853): like-ish `Marker.collapse`, but skips over ends of
    /// line and leading asterisks, so it cannot merge everything in a single node. `marker` is dropped (rolled back).
    /// The Kotlin local `shouldIgnore(type)` (whether a token is ignored during the fake collapse: a type that is the
    /// leading asterisks or an end of line by the builder's current token text) is `should_ignore` here.
    fn fake_collapse(&mut self, marker: Marker, token_type: TokenKind) {
        fn should_ignore(builder: &mut SyntaxTreeBuilder<'_>, element_type: Option<TokenKind>) -> bool {
            element_type.is_some()
                && (element_type == Some(TokenKind::DOC_COMMENT_LEADING_ASTERISKS)
                    || is_eol_token(element_type, builder.token_text()))
        }

        let end_offset = self.builder.current_offset();
        marker.rollback_to(self.builder);
        let current_token_type = self.get_token_type_with(false);
        let mut fuse_marker: Option<Marker> =
            if should_ignore(self.builder, current_token_type) { None } else { Some(self.builder.mark()) };

        while !self.builder.eof() && self.builder.current_offset() < end_offset {
            if should_ignore(self.builder, current_token_type) {
                if let Some(fuse) = fuse_marker {
                    fuse.collapse(self.builder, token_type);
                    fuse.set_custom_edge_token_binders(self.builder, None, Some(Binder::FakeCollapseRight));
                }
                fuse_marker = None;

                self.builder.advance_lexer();
                continue;
            }
            let fuse = match fuse_marker {
                Some(fuse) => fuse,
                None => {
                    let fuse = self.builder.mark();
                    fuse_marker = Some(fuse);
                    fuse
                }
            };
            if is_white_space(self.builder.raw_lookup(1))
                && self.builder.raw_lookup(2) == Some(TokenKind::DOC_COMMENT_LEADING_ASTERISKS)
            {
                self.builder.advance_lexer();
                fuse.collapse(self.builder, token_type);
                fuse.set_custom_edge_token_binders(self.builder, None, Some(Binder::FakeCollapseRight));
                fuse_marker = None;
            } else {
                self.builder.advance_lexer();
            }
        }
        if let Some(fuse) = fuse_marker {
            fuse.collapse(self.builder, token_type);
            fuse.set_custom_edge_token_binders(self.builder, None, Some(Binder::FakeCollapseRight));
        }
    }

    /// `getTokenType(skipWhitespace)` with `skip_whitespace` at its default, true (JavaDocParser.kt:888).
    fn get_token_type(&mut self) -> Option<TokenKind> {
        self.get_token_type_with(true)
    }

    /// `getTokenType(skipWhitespace)` (JavaDocParser.kt:888).
    fn get_token_type_with(&mut self, skip_whitespace: bool) -> Option<TokenKind> {
        loop {
            let token_type = self.builder.token_type();
            if token_type != Some(TokenKind::DOC_SPACE) {
                return token_type;
            }
            self.builder.remap_current_token(TokenKind::WHITE_SPACE);
            if skip_whitespace {
                self.builder.advance_lexer();
            }
        }
    }

    /// `getBraceScope()` (JavaDocParser.kt:897).
    fn get_brace_scope(&self) -> i32 {
        self.brace_scope
    }

    /// `setBraceScope(braceScope)` (JavaDocParser.kt:901).
    fn set_brace_scope(&mut self, brace_scope: i32) {
        self.brace_scope = brace_scope;
    }

    /// `remapAndAdvance()` (JavaDocParser.kt:905).
    fn remap_and_advance(&mut self) {
        if INLINE_TAG_BORDERS_SET.contains(self.builder.token_type()) && self.get_brace_scope() == 0 {
            self.builder.remap_current_token(TokenKind::DOC_COMMENT_DATA);
        }
        self.builder.advance_lexer();
    }
}

/// A tag's name as `parseDataItem` compares it: the constant of a tag this parser knows, "" for any other.
fn tag_name_of(text: Option<&[u8]>) -> Option<&'static [u8]> {
    let text = text?;
    let known = [SEE_TAG, LINK_TAG, LINK_PLAIN_TAG, PARAM_TAG, VALUE_TAG, SNIPPET_TAG, INHERIT_DOC_TAG];
    Some(known.into_iter().chain(REFERENCE_TAGS).find(|&tag| tag == text).unwrap_or(b""))
}

/// `isEolToken(type, text)`: a whitespace token whose text has a `\n`.
fn is_eol_token(element_type: Option<TokenKind>, text: Option<&[u8]>) -> bool {
    is_white_space(element_type) && text.is_some_and(|text| text.contains(&b'\n'))
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
