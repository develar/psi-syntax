//! `DocumentationCommentParser.cs`: the XML of documentation comments and crefs, run by the lexer on each
//! documentation comment it meets (`Lexer.LexXmlDocComment`) over the same lexer in the XML modes (lane D). Every
//! method in C# order; see `parser/mod.rs` for the conventions.
//!
//! The static helpers that read a node's text (`MatchingXmlNames`, `XmlElementSupportsNameAttribute`,
//! `IsNonAsciiQuotationMark`) are methods here: texts are in the arena and the source. A node's `ToString()` or
//! `ToFullString()` (diagnostic arguments, attribute names) is written into reused buffers, not a string per call.
//!
//! Recursion: Roslyn recurses without a guard through nested elements (`ParseXmlElement`), nested type argument lists
//! of a cref (`ParseTypeArguments`) and chained extension member crefs (`ParsePossibleExtensionMemberCref`); here the
//! three share one depth counter up to `MAX_NESTED_SCAN_DEPTH`, the lexer's limit. Past it an element is not started
//! (`ParseXmlNode` returns null, so the rest of the comment is the remainder text), a cref name takes no type argument
//! list and `extension` is a plain name member: the parse stays lossless and ends, on a 1 MB stack.

use std::collections::HashMap;

use super::*;
use crate::green::GreenArena;
use crate::lexer::{Lexer, MAX_NESTED_SCAN_DEPTH};
use crate::syntax_parser::{ParserKind, SyntaxParserBuffers};
use crate::tree::DocumentationMode;

/// The buffers of a `DocumentationCommentParser`, kept by the lexer from one documentation comment to the next.
#[derive(Default)]
pub struct DocumentationCommentParserBuffers {
    syntax: SyntaxParserBuffers,
    pool: SyntaxListPool,
    text: Vec<u8>,
    other_text: Vec<u8>,
    pieces: Vec<Piece>,
    seen_text: Vec<u8>,
    attributes_seen: Vec<SeenAttribute>,
    seen_index: HashMap<u64, u32>,
}

/// An attribute name of `_attributesSeen`: its range in `seen_text` and the previous name of the same hash.
#[derive(Copy, Clone, Debug)]
pub(crate) struct SeenAttribute {
    start: u32,
    len: u32,
    previous: u32,
}

/// `DocumentationCommentParser`: a `SyntaxParser` (`Deref`) with mode resets over the lexer that lexes the comment.
pub struct DocumentationCommentParser<'l, 't> {
    pub(crate) base: SyntaxParser<'l, 't>,
    /// `_pool`.
    pub(crate) pool: SyntaxListPool,
    /// `_isDelimited`.
    pub(crate) is_delimited: bool,
    /// `_attributesSeen` (DocumentationCommentParser.cs:321): the attribute names of the element being parsed, as
    /// ranges of `seen_text`, a hash set by `seen_index` (the last name of each hash, chained by `previous`).
    pub(crate) attributes_seen: Vec<SeenAttribute>,
    seen_text: Vec<u8>,
    seen_index: HashMap<u64, u32>,
    /// Where a node's text is written (`ToString()`).
    text: Vec<u8>,
    other_text: Vec<u8>,
    pieces: Vec<Piece>,
    /// The nesting of elements, cref type argument lists and extension member crefs (see the module's documentation).
    depth: u32,
}

impl<'l, 't> std::ops::Deref for DocumentationCommentParser<'l, 't> {
    type Target = SyntaxParser<'l, 't>;

    fn deref(&self) -> &SyntaxParser<'l, 't> {
        &self.base
    }
}

impl<'l, 't> std::ops::DerefMut for DocumentationCommentParser<'l, 't> {
    fn deref_mut(&mut self) -> &mut SyntaxParser<'l, 't> {
        &mut self.base
    }
}

impl<'l, 't> DocumentationCommentParser<'l, 't> {
    /// `DocumentationCommentParser(lexer)` (DocumentationCommentParser.cs:40): `LexerMode.None`, mode resets allowed.
    pub fn new(lexer: &'l mut Lexer<'t>, buffers: DocumentationCommentParserBuffers) -> Self {
        DocumentationCommentParser {
            base: SyntaxParser::new(
                lexer,
                LexerMode::NONE,
                ParserKind::DocumentationComment,
                true,
                false,
                buffers.syntax,
            ),
            pool: buffers.pool,
            is_delimited: false,
            attributes_seen: buffers.attributes_seen,
            seen_text: buffers.seen_text,
            seen_index: buffers.seen_index,
            text: buffers.text,
            other_text: buffers.other_text,
            pieces: buffers.pieces,
            depth: 0,
        }
    }

    /// `Dispose()`: the buffers back for reuse.
    pub fn into_buffers(self) -> DocumentationCommentParserBuffers {
        DocumentationCommentParserBuffers {
            syntax: self.base.into_buffers(),
            pool: self.pool,
            text: self.text,
            other_text: self.other_text,
            pieces: self.pieces,
            seen_text: self.seen_text,
            attributes_seen: self.attributes_seen,
            seen_index: self.seen_index,
        }
    }

    /// The source the tokens' texts index.
    fn src(&self) -> &'t [u8] {
        self.base.lexer.text()
    }

    /// A node's text as a diagnostic argument: `ToString()`, or `ToFullString()` when `full`. Below
    /// `DocumentationMode.Diagnose` no diagnostic of this parser is attached, so the argument is not made.
    fn text_arg(&mut self, node: Green, full: bool) -> DiagArg {
        if self.options().doc_mode < DocumentationMode::Diagnose {
            return DiagArg::Str("");
        }
        let mut text = std::mem::take(&mut self.text);
        write_text(self.base.arena_ref(), self.src(), node, full, &mut text, &mut self.pieces);
        let (start, len) = self.base.arena().intern(&String::from_utf8_lossy(&text));
        self.text = text;
        DiagArg::Owned { start, len }
    }
}

/// A piece of a text being written (`write_text`'s stack).
#[derive(Copy, Clone, Debug)]
pub(crate) enum Piece {
    Element(Green),
    Text(Green),
    Trivia(crate::green::Trivia),
}

/// Writes the text of `element` to `out` (cleared first): `ToFullString()` when `full`, else `ToString()` (without the
/// first token's leading and the last token's trailing trivia). Iterative over `stack`, structured trivia included.
pub(crate) fn write_text(
    arena: &GreenArena,
    source: &[u8],
    element: Green,
    full: bool,
    out: &mut Vec<u8>,
    stack: &mut Vec<Piece>,
) {
    out.clear();
    stack.clear();
    stack.push(Piece::Element(element));
    while let Some(piece) = stack.pop() {
        match piece {
            Piece::Element(g) if g.is_token() => {
                for &t in arena.trivia_items(arena.trailing_trivia(g)).iter().rev() {
                    stack.push(Piece::Trivia(t));
                }
                stack.push(Piece::Text(g));
                for &t in arena.trivia_items(arena.leading_trivia(g)).iter().rev() {
                    stack.push(Piece::Trivia(t));
                }
            }
            Piece::Element(g) => {
                for &s in arena.slots(g).iter().rev() {
                    if !s.is_null() {
                        stack.push(Piece::Element(s));
                    }
                }
            }
            Piece::Text(g) => out.extend_from_slice(arena.token_text(g, source)),
            Piece::Trivia(t) => {
                let structure = arena.trivia_structure(t);
                if !structure.is_null() {
                    stack.push(Piece::Element(structure));
                } else {
                    let start = arena.trivia_start(t) as usize;
                    let end = start + arena.trivia_width(t) as usize;
                    out.extend_from_slice(source.get(start..end).unwrap_or(b""));
                }
            }
        }
    }
    if !full {
        let leading = (arena.leading_trivia_width(element) as usize).min(out.len());
        let trailing = (arena.trailing_trivia_width(element) as usize).min(out.len() - leading);
        out.truncate(out.len() - trailing);
        out.drain(..leading);
    }
}

/// `InCref` of a mode (DocumentationCommentParser.cs:1613).
fn in_cref_mode(mode: LexerMode) -> bool {
    matches!(
        mode.and(LexerMode::XML_CREF_DOUBLE_QUOTE.or(LexerMode::XML_CREF_QUOTE)),
        LexerMode::XML_CREF_QUOTE | LexerMode::XML_CREF_DOUBLE_QUOTE
    )
}

/// `protected override SyntaxDiagnosticInfo GetExpectedTokenError(SyntaxKind expected, SyntaxKind actual, int offset,
/// int length)` (DocumentationCommentParser.cs:790), called by `SyntaxParser::get_expected_token_error`: no errors in
/// crefs, only warnings (`WRN_ErrorOverride` around the base's error); elsewhere an XML parse error.
pub(crate) fn get_expected_token_error(
    base: &mut SyntaxParser<'_, '_>,
    expected: SyntaxKind,
    actual: SyntaxKind,
    offset: i32,
    length: i32,
) -> DiagnosticId {
    // NOTE: There are no errors in crefs - only warnings.  We accomplish this by wrapping every diagnostic in ErrorCode.WRN_ErrorOverride.
    if in_cref_mode(base.mode()) {
        let raw_info = base.base_get_expected_token_error(expected, actual, offset, length);
        let (raw_offset, raw_width, raw_code) = {
            let d = base.arena_ref().diagnostic(raw_info);
            (d.offset, d.width, d.code)
        };
        return base.make_error(
            raw_offset,
            raw_width,
            ErrorCode::WRN_ErrorOverride,
            &[DiagArg::Info(raw_info), DiagArg::Int(raw_code.number() as i64)],
        );
    }
    match expected {
        SyntaxKind::IdentifierToken => {
            base.arena().make_xml_diagnostic(offset, length, XmlParseErrorCode::XML_ExpectedIdentifier, &[])
        }
        _ => base.arena().make_xml_diagnostic(
            offset,
            length,
            XmlParseErrorCode::XML_InvalidToken,
            &[DiagArg::Str(facts::get_text(actual))],
        ),
    }
}

/// `protected override SyntaxDiagnosticInfo GetExpectedMissingNodeOrTokenError(GreenNode missingNodeOrToken,
/// SyntaxKind expected, SyntaxKind actual)` (DocumentationCommentParser.cs:810).
pub(crate) fn get_expected_missing_node_or_token_error(
    base: &mut SyntaxParser<'_, '_>,
    missing_node_or_token: Green,
    expected: SyntaxKind,
    actual: SyntaxKind,
) -> DiagnosticId {
    debug_assert!(base.arena_ref().is_missing(missing_node_or_token));
    // NOTE: There are no errors in crefs - only warnings.  We accomplish this by wrapping every diagnostic in ErrorCode.WRN_ErrorOverride.
    if in_cref_mode(base.mode()) {
        return base.base_get_expected_missing_node_or_token_error(missing_node_or_token, expected, actual);
    }
    match expected {
        SyntaxKind::IdentifierToken => {
            base.arena().make_xml_diagnostic(0, 0, XmlParseErrorCode::XML_ExpectedIdentifier, &[])
        }
        _ => base.arena().make_xml_diagnostic(
            0,
            0,
            XmlParseErrorCode::XML_InvalidToken,
            &[DiagArg::Str(facts::get_text(actual))],
        ),
    }
}

// `WithAdditionalDiagnostics` (DocumentationCommentParser.cs:846), the third override, is also
// `SyntaxParser::with_additional_diagnostics` with `ParserKind::DocumentationComment`, for the base's own methods.

// DocumentationCommentParser.cs:37: `private readonly SyntaxListPool _pool = new SyntaxListPool();`

// DocumentationCommentParser.cs:38: `private bool _isDelimited;`

// DocumentationCommentParser.cs:40: `internal DocumentationCommentParser(Lexer lexer) : base(lexer, LexerMode.None, oldTree: null, changes: null, allowModeReset: true)`

impl<'l, 't> DocumentationCommentParser<'l, 't> {
    /// `internal void ReInitialize(LexerMode modeflags)` (DocumentationCommentParser.cs:45).
    pub(crate) fn re_initialize(&mut self, modeflags: LexerMode) {
        self.base.re_initialize();
        self.base.set_mode(LexerMode::XML_DOC_COMMENT.or(LexerMode::XML_DOC_COMMENT_LOCATION_START).or(modeflags));
        self.is_delimited = modeflags.and(LexerMode::XML_DOC_COMMENT_STYLE_DELIMITED) != LexerMode::NONE;
    }

    /// `private LexerMode SetMode(LexerMode mode)` (DocumentationCommentParser.cs:52).
    pub(crate) fn set_mode(&mut self, mode: LexerMode) -> LexerMode {
        let tmp = self.base.mode();
        self.base.set_mode(
            mode.or(tmp.and(LexerMode::MASK_XML_DOC_COMMENT_LOCATION.or(LexerMode::MASK_XML_DOC_COMMENT_STYLE))),
        );
        tmp
    }

    /// `private void ResetMode(LexerMode mode)` (DocumentationCommentParser.cs:59).
    pub(crate) fn reset_mode(&mut self, mode: LexerMode) {
        self.base.set_mode(mode);
    }

    /// `public DocumentationCommentTriviaSyntax ParseDocumentationComment(out bool isTerminated)` (DocumentationCommentParser.cs:64).
    pub(crate) fn parse_documentation_comment(&mut self, is_terminated: &mut bool) -> Green {
        let mut nodes = self.pool.allocate();
        self.parse_xml_nodes(&mut nodes);

        // It's possible that we finish parsing the xml, and we are still left in the middle
        // of an Xml comment. For example,
        //
        //     /// <goo></goo></uhoh>
        //                    ^
        // In this case, we stop at the caret. We need to ensure that we consume the remainder
        // of the doc comment here, since otherwise we will return the lexer to the state
        // where it recognizes C# tokens, which means that C# parser will get the </uhoh>,
        // which is not at all what we want.

        if self.current_kind() != SyntaxKind::EndOfDocumentationCommentToken {
            self.parse_remainder(&mut nodes);
        }

        let eoc = self.eat_token_kind(SyntaxKind::EndOfDocumentationCommentToken);

        *is_terminated = !self.is_delimited || {
            let a = self.arena_ref();
            a.trivia_items(a.leading_trivia(eoc)).last().is_some_and(|&t| {
                let start = a.trivia_start(t) as usize;
                a.trivia_structure(t).is_null()
                    && self.src().get(start..start + a.trivia_width(t) as usize) == Some(b"*/".as_slice())
            })
        };
        let kind = if self.is_delimited {
            SyntaxKind::MultiLineDocumentationCommentTrivia
        } else {
            SyntaxKind::SingleLineDocumentationCommentTrivia
        };

        let a = self.base.arena();
        let content = a.list(&nodes);
        let result = a.documentation_comment_trivia(kind, content, eoc);
        self.pool.free(nodes);
        result
    }

    /// `public void ParseRemainder(SyntaxListBuilder<XmlNodeSyntax> nodes)` (DocumentationCommentParser.cs:99).
    pub(crate) fn parse_remainder(&mut self, nodes: &mut SyntaxListBuilder) {
        let end_tag = self.current_kind() == SyntaxKind::LessThanSlashToken;

        let save_mode = self.set_mode(LexerMode::XML_C_DATA_SECTION_TEXT);

        let mut text_tokens = self.pool.allocate();
        // (an end of file ends it too, which the lexer never gives before the end of the comment: no endless loop)
        while !matches!(self.current_kind(), SyntaxKind::EndOfDocumentationCommentToken | SyntaxKind::EndOfFileToken) {
            let token = self.eat_token();

            // TODO: It is possible that a non-literal gets in here. ]]>, specifically. Is that ok?
            text_tokens.push(token);
        }

        let a = self.base.arena();
        let list = a.list(&text_tokens);
        let mut all_remainder_text = a.xml_text(list);

        let code =
            if end_tag { XmlParseErrorCode::XML_EndTagNotExpected } else { XmlParseErrorCode::XML_ExpectedEndOfXml };
        let error = self.base.arena().make_xml_diagnostic(0, 1, code, &[]);
        all_remainder_text = self.with_additional_diagnostics(all_remainder_text, &[error]);

        nodes.push(all_remainder_text);
        self.pool.free(text_tokens);

        self.reset_mode(save_mode);
    }

    /// `private void ParseXmlNodes(SyntaxListBuilder<XmlNodeSyntax> nodes)` (DocumentationCommentParser.cs:131).
    pub(crate) fn parse_xml_nodes(&mut self, nodes: &mut SyntaxListBuilder) {
        loop {
            let node = self.parse_xml_node();
            if node.is_null() {
                return;
            }

            nodes.push(node);
        }
    }

    /// `private XmlNodeSyntax ParseXmlNode()` (DocumentationCommentParser.cs:145); an element past the nesting limit
    /// is not started (see the module's documentation).
    pub(crate) fn parse_xml_node(&mut self) -> Green {
        match self.current_kind() {
            SyntaxKind::XmlTextLiteralToken
            | SyntaxKind::XmlTextLiteralNewLineToken
            | SyntaxKind::XmlEntityLiteralToken => self.parse_xml_text(),
            SyntaxKind::LessThanToken if self.depth < MAX_NESTED_SCAN_DEPTH => self.parse_xml_element(),
            SyntaxKind::XmlCommentStartToken => self.parse_xml_comment(),
            SyntaxKind::XmlCDataStartToken => self.parse_xml_c_data_section(),
            SyntaxKind::XmlProcessingInstructionStartToken => self.parse_xml_processing_instruction(),
            SyntaxKind::EndOfDocumentationCommentToken => Green::NULL,
            // This means we have some unrecognized token. We probably need to give an error.
            _ => Green::NULL,
        }
    }

    /// `private bool IsXmlNodeStartOrStop()` (DocumentationCommentParser.cs:169).
    pub(crate) fn is_xml_node_start_or_stop(&mut self) -> bool {
        matches!(
            self.current_kind(),
            SyntaxKind::LessThanToken
                | SyntaxKind::LessThanSlashToken
                | SyntaxKind::XmlCommentStartToken
                | SyntaxKind::XmlCDataStartToken
                | SyntaxKind::XmlProcessingInstructionStartToken
                | SyntaxKind::GreaterThanToken
                | SyntaxKind::SlashGreaterThanToken
                | SyntaxKind::EndOfDocumentationCommentToken
        )
    }

    /// `private XmlNodeSyntax ParseXmlText()` (DocumentationCommentParser.cs:187).
    pub(crate) fn parse_xml_text(&mut self) -> Green {
        let mut text_tokens = self.pool.allocate();
        while matches!(
            self.current_kind(),
            SyntaxKind::XmlTextLiteralToken | SyntaxKind::XmlTextLiteralNewLineToken | SyntaxKind::XmlEntityLiteralToken
        ) {
            let token = self.eat_token();
            text_tokens.push(token);
        }

        let list = self.base.arena().list(&text_tokens);
        self.pool.free(text_tokens);
        self.base.arena().xml_text(list)
    }

    /// `private XmlNodeSyntax ParseXmlElement()` (DocumentationCommentParser.cs:202).
    pub(crate) fn parse_xml_element(&mut self) -> Green {
        let less_than = self.eat_token_kind(SyntaxKind::LessThanToken); // guaranteed
        let save_mode = self.set_mode(LexerMode::XML_ELEMENT_TAG);
        let mut name = self.parse_xml_name();
        if self.arena_ref().trailing_trivia_width(less_than) > 0 || self.arena_ref().leading_trivia_width(name) > 0 {
            // The Xml spec disallows whitespace here: STag ::= '<' Name (S Attribute)* S? '>'
            name = self.with_xml_parse_error(name, XmlParseErrorCode::XML_InvalidWhitespace);
        }

        let mut attrs = self.pool.allocate();
        self.parse_xml_attributes(&mut name, &mut attrs);

        let result = if self.current_kind() == SyntaxKind::GreaterThanToken {
            let greater_than_token = self.eat_token();
            let a = self.base.arena();
            let attributes = a.list(&attrs);
            let start_tag = a.xml_element_start_tag(less_than, name, attributes, greater_than_token);
            self.set_mode(LexerMode::XML_DOC_COMMENT);
            let mut nodes = self.pool.allocate();
            self.depth += 1;
            self.parse_xml_nodes(&mut nodes);
            self.depth -= 1;

            let mut end_name;
            let greater_than;

            // end tag
            let mut less_than_slash = self.eat_token_report(SyntaxKind::LessThanSlashToken, false);

            // If we didn't see "</", then we can't really be confident that this is actually an end tag,
            // so just insert a missing one.
            if self.arena_ref().is_missing(less_than_slash) {
                self.reset_mode(save_mode);
                let name_text = self.text_arg(name, false);
                less_than_slash = self.with_xml_parse_error_with_args_2(
                    less_than_slash,
                    XmlParseErrorCode::XML_EndTagExpected,
                    &[name_text],
                );
                let a = self.base.arena();
                let local_name = a.missing_token(SyntaxKind::IdentifierToken);
                end_name = a.xml_name(Green::NULL, local_name);
                greater_than = a.missing_token(SyntaxKind::GreaterThanToken);
            } else {
                self.set_mode(LexerMode::XML_ELEMENT_TAG);
                end_name = self.parse_xml_name();
                if self.arena_ref().trailing_trivia_width(less_than_slash) > 0
                    || self.arena_ref().leading_trivia_width(end_name) > 0
                {
                    // The Xml spec disallows whitespace here: STag ::= '<' Name (S Attribute)* S? '>'
                    end_name = self.with_xml_parse_error(end_name, XmlParseErrorCode::XML_InvalidWhitespace);
                }

                if !self.arena_ref().is_missing(end_name) && !self.matching_xml_names(name, end_name) {
                    let end_name_text = self.text_arg(end_name, false);
                    let name_text = self.text_arg(name, false);
                    end_name = self.with_xml_parse_error_with_args(
                        end_name,
                        XmlParseErrorCode::XML_ElementTypeMatch,
                        &[end_name_text, name_text],
                    );
                }

                // if we don't see the greater than token then skip the badness until we do or abort
                if self.current_kind() != SyntaxKind::GreaterThanToken {
                    self.skip_bad_tokens(
                        &mut end_name,
                        None,
                        |p| p.current_kind() != SyntaxKind::GreaterThanToken,
                        |p| p.is_xml_node_start_or_stop(),
                        XmlParseErrorCode::XML_InvalidToken,
                    );
                }

                greater_than = self.eat_token_kind(SyntaxKind::GreaterThanToken);
            }

            let end_tag = self.base.arena().xml_element_end_tag(less_than_slash, end_name, greater_than);
            self.reset_mode(save_mode);
            let a = self.base.arena();
            let content = a.list(&nodes);
            let element = a.xml_element(start_tag, content, end_tag);
            self.pool.free(nodes);
            element
        } else {
            let mut slash_greater = self.eat_token_report(SyntaxKind::SlashGreaterThanToken, false);
            if self.arena_ref().is_missing(slash_greater) && !self.arena_ref().is_missing(name) {
                let name_text = self.text_arg(name, false);
                slash_greater = self.with_xml_parse_error_with_args_2(
                    slash_greater,
                    XmlParseErrorCode::XML_ExpectedEndOfTag,
                    &[name_text],
                );
            }

            self.reset_mode(save_mode);
            let a = self.base.arena();
            let attributes = a.list(&attrs);
            a.xml_empty_element(less_than, name, attributes, slash_greater)
        };
        self.pool.free(attrs);
        result
    }

    /// `private static bool MatchingXmlNames(XmlNameSyntax name, XmlNameSyntax endName)` (DocumentationCommentParser.cs:297):
    /// a method, the texts are in the arena. Equivalent names have equal texts, so the texts decide.
    pub(crate) fn matching_xml_names(&mut self, name: Green, end_name: Green) -> bool {
        // PERF: because of deduplication we often get the same name for name and endName,
        //       so we will check for such case first before materializing text for entire nodes
        //       and comparing that.
        if name == end_name {
            return true;
        }

        let mut text = std::mem::take(&mut self.text);
        let mut other = std::mem::take(&mut self.other_text);
        write_text(self.base.arena_ref(), self.src(), name, false, &mut text, &mut self.pieces);
        write_text(self.base.arena_ref(), self.src(), end_name, false, &mut other, &mut self.pieces);
        let result = text == other;
        self.text = text;
        self.other_text = other;
        result
    }

    /// `_attributesSeen.Add(name)`: whether `name` was not seen yet (it is now).
    fn add_attribute_seen(&mut self, name: &[u8]) -> bool {
        // FNV-1a
        let hash = name.iter().fold(0xcbf29ce484222325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x100000001b3));
        let mut at = self.seen_index.get(&hash).copied().unwrap_or(u32::MAX);
        while let Some(seen) = self.attributes_seen.get(at as usize) {
            if self.seen_text.get(seen.start as usize..(seen.start + seen.len) as usize) == Some(name) {
                return false;
            }
            at = seen.previous;
        }
        let previous = self.seen_index.insert(hash, self.attributes_seen.len() as u32).unwrap_or(u32::MAX);
        self.attributes_seen.push(SeenAttribute { start: self.seen_text.len() as u32, len: name.len() as u32, previous });
        self.seen_text.extend_from_slice(name);
        true
    }

    // DocumentationCommentParser.cs:321: `private readonly HashSet<string> _attributesSeen = new HashSet<string>();`
    /// `private void ParseXmlAttributes(ref XmlNameSyntax elementName, SyntaxListBuilder<XmlAttributeSyntax> attrs)` (DocumentationCommentParser.cs:323).
    pub(crate) fn parse_xml_attributes(&mut self, element_name: &mut Green, attrs: &mut SyntaxListBuilder) {
        self.attributes_seen.clear();
        self.seen_text.clear();
        self.seen_index.clear();
        loop {
            if self.current_kind() == SyntaxKind::IdentifierToken {
                let mut attr = self.parse_xml_attribute(*element_name);
                // `attr.Name.ToString()`: the name is the first slot of every attribute
                let attr_name = self.arena_ref().slot(attr, 0);
                let mut text = std::mem::take(&mut self.text);
                write_text(self.base.arena_ref(), self.src(), attr_name, false, &mut text, &mut self.pieces);
                let added = self.add_attribute_seen(&text);
                self.text = text;
                if !added {
                    let attr_name_text = self.text_arg(attr_name, false);
                    attr = self.with_xml_parse_error_with_args(
                        attr,
                        XmlParseErrorCode::XML_DuplicateAttribute,
                        &[attr_name_text],
                    );
                }

                attrs.push(attr);
            } else {
                let skip = self.skip_bad_tokens(
                    element_name,
                    Some(attrs),
                    // not expected condition
                    |p| p.current_kind() != SyntaxKind::IdentifierName,
                    // abort condition (looks like something we might understand later)
                    |p| {
                        matches!(
                            p.current_kind(),
                            SyntaxKind::GreaterThanToken
                                | SyntaxKind::SlashGreaterThanToken
                                | SyntaxKind::LessThanToken
                                | SyntaxKind::LessThanSlashToken
                                | SyntaxKind::EndOfDocumentationCommentToken
                                | SyntaxKind::EndOfFileToken
                        )
                    },
                    XmlParseErrorCode::XML_InvalidToken,
                );

                if skip == SkipResult::Abort {
                    break;
                }
            }
        }
    }
}

/// `SkipResult` (DocumentationCommentParser.cs:365).
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub(crate) enum SkipResult {
    #[default]
    Continue,
    Abort,
}

impl<'l, 't> DocumentationCommentParser<'l, 't> {
    /// `private SkipResult SkipBadTokens<T>( ref T startNode, SyntaxListBuilder list, Func<DocumentationCommentParser, bool> isNotExpectedFunction, Func<DocumentationCommentParser, bool> abortFunction, XmlParseErrorCode error ) where T : CSharpSyntaxNode` (DocumentationCommentParser.cs:371).
    pub(crate) fn skip_bad_tokens(
        &mut self,
        start_node: &mut Green,
        list: Option<&mut SyntaxListBuilder>,
        is_not_expected_function: fn(&mut DocumentationCommentParser<'_, '_>) -> bool,
        abort_function: fn(&mut DocumentationCommentParser<'_, '_>) -> bool,
        error: XmlParseErrorCode,
    ) -> SkipResult {
        let mut bad_tokens: Option<SyntaxListBuilder> = None;
        let mut has_error = false;

        let mut result = SkipResult::Continue;

        while is_not_expected_function(self) {
            if abort_function(self) {
                result = SkipResult::Abort;
                break;
            }

            let mut token = self.eat_token();
            if !has_error {
                token = self.with_xml_parse_error_with_args_2(token, error, &[DiagArg::TokenText(token)]);
                has_error = true;
            }

            bad_tokens.get_or_insert_with(|| self.pool.allocate()).push(token);
        }

        let skip = match bad_tokens.as_deref() {
            Some(tokens) if !tokens.is_empty() => {
                // use skipped text since cannot nest structured trivia under structured trivia
                let skipped = self.base.arena().list(tokens);
                match list {
                    Some(list) if !list.is_empty() => {
                        let last = list.len() - 1;
                        list[last] = self.add_trailing_skipped_syntax(list[last], skipped);
                    }
                    _ => *start_node = self.add_trailing_skipped_syntax(*start_node, skipped),
                }

                result
            }
            // somehow we did not consume anything, so tell caller to abort parse rule
            _ => SkipResult::Abort,
        };
        if let Some(tokens) = bad_tokens {
            self.pool.free(tokens);
        }
        skip
    }

    /// `private XmlAttributeSyntax ParseXmlAttribute(XmlNameSyntax elementName)` (DocumentationCommentParser.cs:438).
    pub(crate) fn parse_xml_attribute(&mut self, element_name: Green) -> Green {
        let mut attr_name = self.parse_xml_name();
        if self.arena_ref().leading_trivia_width(attr_name) == 0 {
            // The Xml spec requires whitespace here: STag ::= '<' Name (S Attribute)* S? '>'
            attr_name = self.with_xml_parse_error(attr_name, XmlParseErrorCode::XML_WhitespaceMissing);
        }

        let mut equals = self.eat_token_report(SyntaxKind::EqualsToken, false);
        if self.arena_ref().is_missing(equals) {
            equals = self.with_xml_parse_error(equals, XmlParseErrorCode::XML_MissingEqualsAttribute);

            match self.current_kind() {
                SyntaxKind::SingleQuoteToken | SyntaxKind::DoubleQuoteToken => {
                    // There could be a value coming up, let's keep parsing.
                }
                _ => {
                    // This is probably not a complete attribute.
                    let a = self.base.arena();
                    let start_quote = a.missing_token(SyntaxKind::DoubleQuoteToken);
                    let end_quote = a.missing_token(SyntaxKind::DoubleQuoteToken);
                    return a.xml_text_attribute(attr_name, equals, start_quote, Green::NULL, end_quote);
                }
            }
        }

        let mut start_quote = Green::NULL;
        let mut end_quote = Green::NULL;
        let (has_no_prefix, is_cref, is_name) = {
            let a = self.arena_ref();
            let local_name = a.slot(attr_name, 1);
            let attr_name_text = a.token_value_text(local_name, self.src());
            // `DocumentationCommentXmlNames.AttributeEquals`: ordinal
            (a.slot(attr_name, 0).is_null(), attr_name_text == b"cref", attr_name_text == b"name")
        };
        if has_no_prefix && is_cref && !self.is_verbatim_cref() {
            let mut cref = Green::NULL;
            self.parse_cref_attribute(&mut start_quote, &mut cref, &mut end_quote);
            self.base.arena().xml_cref_attribute(attr_name, equals, start_quote, cref, end_quote)
        } else if has_no_prefix && is_name && self.xml_element_supports_name_attribute(element_name) {
            let mut identifier = Green::NULL;
            self.parse_name_attribute(&mut start_quote, &mut identifier, &mut end_quote);
            self.base.arena().xml_name_attribute(attr_name, equals, start_quote, identifier, end_quote)
        } else {
            let mut text_tokens = self.pool.allocate();
            self.parse_xml_attribute_text(&mut start_quote, &mut text_tokens, &mut end_quote);
            let a = self.base.arena();
            let list = a.list(&text_tokens);
            let attribute = a.xml_text_attribute(attr_name, equals, start_quote, list, end_quote);
            self.pool.free(text_tokens);
            attribute
        }
    }

    /// `private static bool XmlElementSupportsNameAttribute(XmlNameSyntax elementName)` (DocumentationCommentParser.cs:502):
    /// a method, the name's text is in the arena.
    pub(crate) fn xml_element_supports_name_attribute(&self, element_name: Green) -> bool {
        let a = self.arena_ref();
        if !a.slot(element_name, 0).is_null() {
            return false;
        }

        let local_name = a.token_value_text(a.slot(element_name, 1), self.src());
        // `DocumentationCommentXmlNames.ElementEquals`: ordinal ignoring case (no character outside ASCII folds to
        // one of these names' letters)
        [b"param".as_slice(), b"paramref", b"typeparam", b"typeparamref"]
            .iter()
            .any(|name| local_name.eq_ignore_ascii_case(name))
    }

    /// `private bool IsVerbatimCref()` (DocumentationCommentParser.cs:517).
    pub(crate) fn is_verbatim_cref(&mut self) -> bool {
        // As in XMLDocWriter::ReplaceReferences, if the first character of the value is not colon and the second character
        // is, then don't process the cref - just emit it as-is.
        let mut is_verbatim = false;

        let reset_point = self.base.get_reset_point();

        let quote_kind = if self.current_kind() == SyntaxKind::SingleQuoteToken {
            SyntaxKind::SingleQuoteToken
        } else {
            SyntaxKind::DoubleQuoteToken
        };
        let open_quote = self.eat_token_kind(quote_kind);

        // NOTE: Don't need to save mode, since we're already using a reset point.
        self.set_mode(LexerMode::XML_CHARACTER);

        let current = self.current_token();
        let open_quote_text = facts::get_text(self.arena_ref().kind(open_quote)).as_bytes();
        if self.is_text_or_entity(current) && {
            let value = self.arena_ref().token_value_text(current, self.src());
            value != open_quote_text && value != b":"
        } {
            self.eat_token();

            let current = self.current_token();
            if self.is_text_or_entity(current) && self.arena_ref().token_value_text(current, self.src()) == b":" {
                is_verbatim = true;
            }
        }

        self.base.reset(&reset_point);
        self.base.release(&reset_point);

        is_verbatim
    }

    /// `current.Kind == SyntaxKind.XmlTextLiteralToken || current.Kind == SyntaxKind.XmlEntityLiteralToken`.
    fn is_text_or_entity(&self, token: Green) -> bool {
        matches!(self.arena_ref().kind(token), SyntaxKind::XmlTextLiteralToken | SyntaxKind::XmlEntityLiteralToken)
    }

    /// `private void ParseCrefAttribute(out SyntaxToken startQuote, out CrefSyntax cref, out SyntaxToken endQuote)` (DocumentationCommentParser.cs:553).
    pub(crate) fn parse_cref_attribute(&mut self, start_quote: &mut Green, cref: &mut Green, end_quote: &mut Green) {
        *start_quote = self.parse_xml_attribute_start_quote();
        let quote_kind = self.arena_ref().kind(*start_quote);

        {
            let save_mode = self.set_mode(if quote_kind == SyntaxKind::SingleQuoteToken {
                LexerMode::XML_CREF_QUOTE
            } else {
                LexerMode::XML_CREF_DOUBLE_QUOTE
            });

            *cref = self.parse_cref_attribute_value();

            self.reset_mode(save_mode);
        }

        *end_quote = self.parse_xml_attribute_end_quote(quote_kind);
    }

    /// `private void ParseNameAttribute(out SyntaxToken startQuote, out IdentifierNameSyntax identifier, out SyntaxToken endQuote)` (DocumentationCommentParser.cs:571).
    pub(crate) fn parse_name_attribute(
        &mut self,
        start_quote: &mut Green,
        identifier: &mut Green,
        end_quote: &mut Green,
    ) {
        *start_quote = self.parse_xml_attribute_start_quote();
        let quote_kind = self.arena_ref().kind(*start_quote);

        {
            let save_mode = self.set_mode(if quote_kind == SyntaxKind::SingleQuoteToken {
                LexerMode::XML_NAME_QUOTE
            } else {
                LexerMode::XML_NAME_DOUBLE_QUOTE
            });

            *identifier = self.parse_name_attribute_value();

            self.reset_mode(save_mode);
        }

        *end_quote = self.parse_xml_attribute_end_quote(quote_kind);
    }

    /// `private void ParseXmlAttributeText(out SyntaxToken startQuote, SyntaxListBuilder<SyntaxToken> textTokens, out SyntaxToken endQuote)` (DocumentationCommentParser.cs:589).
    pub(crate) fn parse_xml_attribute_text(
        &mut self,
        start_quote: &mut Green,
        text_tokens: &mut SyntaxListBuilder,
        end_quote: &mut Green,
    ) {
        *start_quote = self.parse_xml_attribute_start_quote();
        let quote_kind = self.arena_ref().kind(*start_quote);

        // NOTE: Being a bit sneaky here - if the width isn't 0, we consumed something else in
        // place of the quote and we should continue parsing the attribute.
        if self.arena_ref().is_missing(*start_quote) && self.arena_ref().full_width(*start_quote) == 0 {
            *end_quote = self.base.arena().missing_token(quote_kind);
        } else {
            let save_mode = self.set_mode(if quote_kind == SyntaxKind::SingleQuoteToken {
                LexerMode::XML_ATTRIBUTE_TEXT_QUOTE
            } else {
                LexerMode::XML_ATTRIBUTE_TEXT_DOUBLE_QUOTE
            });

            while matches!(
                self.current_kind(),
                SyntaxKind::XmlTextLiteralToken
                    | SyntaxKind::XmlTextLiteralNewLineToken
                    | SyntaxKind::XmlEntityLiteralToken
                    | SyntaxKind::LessThanToken
            ) {
                let mut token = self.eat_token();
                if self.arena_ref().kind(token) == SyntaxKind::LessThanToken {
                    // TODO: It is possible that a non-literal gets in here. <, specifically. Is that ok?
                    token = self.with_xml_parse_error(token, XmlParseErrorCode::XML_LessThanInAttributeValue);
                }

                text_tokens.push(token);
            }

            self.reset_mode(save_mode);

            // NOTE: This will never consume a non-ascii quote, since non-ascii quotes
            // are legal in the attribute value and are consumed by the preceding loop.
            *end_quote = self.parse_xml_attribute_end_quote(quote_kind);
        }
    }

    /// `private SyntaxToken ParseXmlAttributeStartQuote()` (DocumentationCommentParser.cs:629).
    pub(crate) fn parse_xml_attribute_start_quote(&mut self) -> Green {
        let current = self.current_token();
        if self.is_non_ascii_quotation_mark(current) {
            return self.skip_non_ascii_quotation_mark();
        }

        let quote_kind = if self.current_kind() == SyntaxKind::SingleQuoteToken {
            SyntaxKind::SingleQuoteToken
        } else {
            SyntaxKind::DoubleQuoteToken
        };

        let mut start_quote = self.eat_token_report(quote_kind, false);
        if self.arena_ref().is_missing(start_quote) {
            start_quote = self.with_xml_parse_error(start_quote, XmlParseErrorCode::XML_StringLiteralNoStartQuote);
        }
        start_quote
    }

    /// `private SyntaxToken ParseXmlAttributeEndQuote(SyntaxKind quoteKind)` (DocumentationCommentParser.cs:648).
    pub(crate) fn parse_xml_attribute_end_quote(&mut self, quote_kind: SyntaxKind) -> Green {
        let current = self.current_token();
        if self.is_non_ascii_quotation_mark(current) {
            return self.skip_non_ascii_quotation_mark();
        }

        let mut end_quote = self.eat_token_report(quote_kind, false);
        if self.arena_ref().is_missing(end_quote) {
            end_quote = self.with_xml_parse_error(end_quote, XmlParseErrorCode::XML_StringLiteralNoEndQuote);
        }
        end_quote
    }

    /// `private SyntaxToken SkipNonAsciiQuotationMark()` (DocumentationCommentParser.cs:663).
    pub(crate) fn skip_non_ascii_quotation_mark(&mut self) -> Green {
        let mut quote = self.base.arena().missing_token(SyntaxKind::DoubleQuoteToken);
        let token = self.eat_token();
        quote = self.add_trailing_skipped_syntax(quote, token);
        quote = self.with_xml_parse_error(quote, XmlParseErrorCode::XML_StringLiteralNonAsciiQuote);
        quote
    }

    /// `private static bool IsNonAsciiQuotationMark(SyntaxToken token)` (DocumentationCommentParser.cs:676): a method,
    /// the token's text is in the arena; one UTF-16 code unit is one BMP character of 1 to 3 bytes.
    pub(crate) fn is_non_ascii_quotation_mark(&self, token: Green) -> bool {
        let text = self.arena_ref().token_text(token, self.src());
        let mut units = std::str::from_utf8(text).unwrap_or("").encode_utf16();
        matches!(
            (units.next(), units.next()),
            (Some(ch), None) if crate::lexer::character_info::is_non_ascii_quotation_mark(ch)
        )
    }

    /// `private XmlNameSyntax ParseXmlName()` (DocumentationCommentParser.cs:681).
    pub(crate) fn parse_xml_name(&mut self) -> Green {
        let mut id = self.eat_token_kind(SyntaxKind::IdentifierToken);
        let mut prefix = Green::NULL;
        if self.current_kind() == SyntaxKind::ColonToken {
            let mut colon = self.eat_token();

            let prefix_trailing_width = self.arena_ref().trailing_trivia_width(id) as i32;
            let colon_leading_width = self.arena_ref().leading_trivia_width(colon) as i32;

            if prefix_trailing_width > 0 || colon_leading_width > 0 {
                // NOTE: offset is relative to Start (not FullStart) of colon.
                let width = prefix_trailing_width + colon_leading_width;
                let offset = -width;
                let error =
                    self.base.arena().make_xml_diagnostic(offset, width, XmlParseErrorCode::XML_InvalidWhitespace, &[]);
                colon = self.with_additional_diagnostics(colon, &[error]);
            }

            prefix = self.base.arena().xml_prefix(id, colon);
            id = self.eat_token_kind(SyntaxKind::IdentifierToken);

            let colon_trailing_width = self.arena_ref().trailing_trivia_width(colon) as i32;
            let local_name_leading_width = self.arena_ref().leading_trivia_width(id) as i32;
            if colon_trailing_width > 0 || local_name_leading_width > 0 {
                // NOTE: offset is relative to Start (not FullStart) of identifier.
                let width = colon_trailing_width + local_name_leading_width;
                let offset = -width;
                let error =
                    self.base.arena().make_xml_diagnostic(offset, width, XmlParseErrorCode::XML_InvalidWhitespace, &[]);
                id = self.with_additional_diagnostics(id, &[error]);

                // CONSIDER: Another interpretation would be that the local part of this name is a missing identifier and the identifier
                // we've just consumed is actually part of something else (e.g. an attribute name).
            }
        }

        self.base.arena().xml_name(prefix, id)
    }

    /// `private XmlCommentSyntax ParseXmlComment()` (DocumentationCommentParser.cs:720).
    pub(crate) fn parse_xml_comment(&mut self) -> Green {
        let less_than_exclamation_minus_minus_token = self.eat_token_kind(SyntaxKind::XmlCommentStartToken);
        let save_mode = self.set_mode(LexerMode::XML_COMMENT_TEXT);
        let mut text_tokens = self.pool.allocate();
        while matches!(
            self.current_kind(),
            SyntaxKind::XmlTextLiteralToken | SyntaxKind::XmlTextLiteralNewLineToken | SyntaxKind::MinusMinusToken
        ) {
            let mut token = self.eat_token();
            if self.arena_ref().kind(token) == SyntaxKind::MinusMinusToken {
                // TODO: It is possible that a non-literal gets in here. --, specifically. Is that ok?
                token = self.with_xml_parse_error(token, XmlParseErrorCode::XML_IncorrectComment);
            }

            text_tokens.push(token);
        }

        let list = self.base.arena().list(&text_tokens);
        self.pool.free(text_tokens);

        let minus_minus_greater_than_token = self.eat_token_kind(SyntaxKind::XmlCommentEndToken);
        self.reset_mode(save_mode);
        self.base.arena().xml_comment(less_than_exclamation_minus_minus_token, list, minus_minus_greater_than_token)
    }

    /// `private XmlCDataSectionSyntax ParseXmlCDataSection()` (DocumentationCommentParser.cs:747).
    pub(crate) fn parse_xml_c_data_section(&mut self) -> Green {
        let start_c_data_token = self.eat_token_kind(SyntaxKind::XmlCDataStartToken);
        let save_mode = self.set_mode(LexerMode::XML_C_DATA_SECTION_TEXT);
        let mut text_tokens = self.pool.allocate();
        while matches!(self.current_kind(), SyntaxKind::XmlTextLiteralToken | SyntaxKind::XmlTextLiteralNewLineToken) {
            let token = self.eat_token();
            text_tokens.push(token);
        }

        let end_c_data_token = self.eat_token_kind(SyntaxKind::XmlCDataEndToken);
        self.reset_mode(save_mode);
        let a = self.base.arena();
        let list = a.list(&text_tokens);
        let result = a.xml_c_data_section(start_c_data_token, list, end_c_data_token);
        self.pool.free(text_tokens);
        result
    }

    /// `private XmlProcessingInstructionSyntax ParseXmlProcessingInstruction()` (DocumentationCommentParser.cs:763).
    pub(crate) fn parse_xml_processing_instruction(&mut self) -> Green {
        let start_processing_instruction_token = self.eat_token_kind(SyntaxKind::XmlProcessingInstructionStartToken);
        let save_mode = self.set_mode(LexerMode::XML_ELEMENT_TAG); //this mode accepts names
        let name = self.parse_xml_name();

        // NOTE: The XML spec says that name cannot be "xml" (case-insensitive comparison),
        // but Dev10 does not enforce this.

        self.set_mode(LexerMode::XML_PROCESSING_INSTRUCTION_TEXT); //this mode consumes text
        let mut text_tokens = self.pool.allocate();
        while matches!(self.current_kind(), SyntaxKind::XmlTextLiteralToken | SyntaxKind::XmlTextLiteralNewLineToken) {
            let text_token = self.eat_token();

            // NOTE: The XML spec says that the each text token must begin with a whitespace
            // character, but Dev10 does not enforce this.

            text_tokens.push(text_token);
        }

        let end_processing_instruction_token = self.eat_token_kind(SyntaxKind::XmlProcessingInstructionEndToken);
        self.reset_mode(save_mode);
        let a = self.base.arena();
        let list = a.list(&text_tokens);
        let result =
            a.xml_processing_instruction(start_processing_instruction_token, name, list, end_processing_instruction_token);
        self.pool.free(text_tokens);
        result
    }

    /// `private TNode WithXmlParseError<TNode>(TNode node, XmlParseErrorCode code) where TNode : CSharpSyntaxNode` (DocumentationCommentParser.cs:831).
    pub(crate) fn with_xml_parse_error(&mut self, node: Green, code: XmlParseErrorCode) -> Green {
        let width = self.arena_ref().width(node) as i32;
        let error = self.base.arena().make_xml_diagnostic(0, width, code, &[]);
        self.with_additional_diagnostics(node, &[error])
    }

    /// `private TNode WithXmlParseError<TNode>(TNode node, XmlParseErrorCode code, params string[] args) where TNode : CSharpSyntaxNode` (DocumentationCommentParser.cs:836).
    pub(crate) fn with_xml_parse_error_with_args(
        &mut self,
        node: Green,
        code: XmlParseErrorCode,
        args: &[DiagArg],
    ) -> Green {
        let width = self.arena_ref().width(node) as i32;
        let error = self.base.arena().make_xml_diagnostic(0, width, code, args);
        self.with_additional_diagnostics(node, &[error])
    }

    /// `private SyntaxToken WithXmlParseError(SyntaxToken node, XmlParseErrorCode code, params string[] args)` (DocumentationCommentParser.cs:841).
    pub(crate) fn with_xml_parse_error_with_args_2(
        &mut self,
        node: Green,
        code: XmlParseErrorCode,
        args: &[DiagArg],
    ) -> Green {
        let width = self.arena_ref().width(node) as i32;
        let error = self.base.arena().make_xml_diagnostic(0, width, code, args);
        self.with_additional_diagnostics(node, &[error])
    }

    /// `protected override TNode WithAdditionalDiagnostics<TNode>(TNode node, params DiagnosticInfo[] diagnostics)` (DocumentationCommentParser.cs:846).
    pub(crate) fn with_additional_diagnostics(&mut self, node: Green, diagnostics: &[DiagnosticId]) -> Green {
        // Don't attach any diagnostics to syntax nodes within a documentation comment if the DocumentationMode
        // is not at least Diagnose.
        if self.options().doc_mode >= DocumentationMode::Diagnose {
            self.base.arena().with_additional_diagnostics(node, diagnostics)
        } else {
            node
        }
    }

    /// `private CrefSyntax ParseCrefAttributeValue()` (DocumentationCommentParser.cs:887).
    pub(crate) fn parse_cref_attribute_value(&mut self) -> Green {
        let mut result;

        let type_ = self.parse_cref_type(true, true);
        if type_.is_null() {
            result = self.parse_member_cref();
        } else if self.is_end_of_cref_attribute() {
            result = self.base.arena().type_cref(type_);
        } else if self.arena_ref().kind(type_) != SyntaxKind::QualifiedName
            && self.current_kind() == SyntaxKind::OpenParenToken
        {
            // Special case for crefs like "string()" and "A::B()".
            let parameters = self.parse_cref_parameter_list();
            result = self.base.arena().name_member_cref(type_, parameters);
        } else {
            let dot = self.eat_token_kind(SyntaxKind::DotToken);
            let member = self.parse_member_cref();
            result = self.base.arena().qualified_cref(type_, dot, member);
        }

        let need_overall_error = !self.is_end_of_cref_attribute() || self.arena_ref().contains_diagnostics(result);

        if !self.is_end_of_cref_attribute() {
            let mut bad_tokens = self.pool.allocate();
            while !self.is_end_of_cref_attribute() {
                let token = self.eat_token();
                bad_tokens.push(token);
            }
            let skipped = self.base.arena().list(&bad_tokens);
            result = self.add_trailing_skipped_syntax(result, skipped);
            self.pool.free(bad_tokens);
        }

        if need_overall_error {
            let text = self.text_arg(result, true);
            result = self.add_error_args(result, ErrorCode::WRN_BadXMLRefSyntax, &[text]);
        }

        result
    }

    /// `private MemberCrefSyntax ParseMemberCref()` (DocumentationCommentParser.cs:938); past the nesting limit
    /// `extension` is a name member (see the module's documentation).
    pub(crate) fn parse_member_cref(&mut self) -> Green {
        match self.current_kind() {
            SyntaxKind::ThisKeyword => self.parse_indexer_member_cref(),
            SyntaxKind::OperatorKeyword => self.parse_operator_member_cref(),
            SyntaxKind::ExplicitKeyword | SyntaxKind::ImplicitKeyword => self.parse_conversion_operator_member_cref(),
            SyntaxKind::IdentifierToken
                if self.current_contextual_kind() == SyntaxKind::ExtensionKeyword
                    && self.depth < MAX_NESTED_SCAN_DEPTH =>
            {
                self.parse_possible_extension_member_cref()
            }
            _ => self.parse_name_member_cref(),
        }
    }

    /// `private NameMemberCrefSyntax ParseNameMemberCref()` (DocumentationCommentParser.cs:960).
    pub(crate) fn parse_name_member_cref(&mut self) -> Green {
        let name = self.parse_cref_name(true);
        let parameters = self.parse_cref_parameter_list();

        self.base.arena().name_member_cref(name, parameters)
    }

    /// `private IndexerMemberCrefSyntax ParseIndexerMemberCref()` (DocumentationCommentParser.cs:971).
    pub(crate) fn parse_indexer_member_cref(&mut self) -> Green {
        debug_assert!(self.current_kind() == SyntaxKind::ThisKeyword);
        let this_keyword = self.eat_token();
        let parameters = self.parse_bracketed_cref_parameter_list();

        self.base.arena().indexer_member_cref(this_keyword, parameters)
    }

    /// `private MemberCrefSyntax ParsePossibleExtensionMemberCref()` (DocumentationCommentParser.cs:985).
    pub(crate) fn parse_possible_extension_member_cref(&mut self) -> Green {
        debug_assert!(self.current_contextual_kind() == SyntaxKind::ExtensionKeyword);

        let identifier_token = self.eat_token();
        let type_arguments =
            if self.current_kind() == SyntaxKind::LessThanToken { self.parse_type_arguments(true) } else { Green::NULL };
        let parameters = if self.current_kind() == SyntaxKind::OpenParenToken {
            self.parse_cref_parameter_list()
        } else {
            Green::NULL
        };

        if parameters.is_null() || self.current_kind() != SyntaxKind::DotToken {
            let a = self.base.arena();
            let name = if !type_arguments.is_null() {
                a.generic_name(identifier_token, type_arguments)
            } else {
                a.identifier_name(identifier_token)
            };

            return a.name_member_cref(name, parameters);
        }

        let dot_token = self.eat_token_kind(SyntaxKind::DotToken);
        self.depth += 1;
        let mut member = self.parse_member_cref();
        self.depth -= 1;
        if self.arena_ref().kind(member) == SyntaxKind::ExtensionMemberCref {
            member = self.add_error_as_warning(member, ErrorCode::ERR_MisplacedExtension, &[]);
        }

        let extension_keyword = self.convert_to_keyword(identifier_token);
        self.base.arena().extension_member_cref(extension_keyword, type_arguments, parameters, dot_token, member)
    }

    /// `private OperatorMemberCrefSyntax ParseOperatorMemberCref()` (DocumentationCommentParser.cs:1016).
    pub(crate) fn parse_operator_member_cref(&mut self) -> Green {
        debug_assert!(self.current_kind() == SyntaxKind::OperatorKeyword);
        let mut operator_keyword = self.eat_token();
        let checked_keyword = self.try_eat_checked_keyword(false, &mut operator_keyword);

        let mut operator_token;

        if facts::is_any_overloadable_operator(self.current_kind()) {
            operator_token = self.eat_token();
        } else {
            operator_token = self.base.arena().missing_token(SyntaxKind::PlusToken);

            // Grab the offset and width before we consume the invalid keyword and change our position.
            let (offset, width) = self.get_diagnostic_span_for_missing_node_or_token(operator_token);

            let current = self.current_kind();
            if facts::is_unary_operator_declaration_token(current) || facts::is_binary_expression_operator_token(current)
            {
                let token = self.eat_token();
                operator_token = self.add_trailing_skipped_syntax(operator_token, token);
            }

            let raw_info = self.make_error(offset, width, ErrorCode::ERR_OvlOperatorExpected, &[]);
            let cref_info = self.make_error(
                offset,
                width,
                ErrorCode::WRN_ErrorOverride,
                &[DiagArg::Info(raw_info), DiagArg::Int(ErrorCode::ERR_OvlOperatorExpected.number() as i64)],
            );

            operator_token = self.with_additional_diagnostics(operator_token, &[cref_info]);
        }

        // Have to fake >>/>>> because it looks like the closing of nested type parameter lists (e.g. A<A<T>>).
        // Have to fake >= so the lexer doesn't mishandle >>=.
        let current = self.current_token();
        if self.arena_ref().kind(operator_token) == SyntaxKind::GreaterThanToken
            && self.no_trivia_between(operator_token, current)
        {
            if self.current_kind() == SyntaxKind::GreaterThanToken {
                let operator_token2 = self.eat_token();

                let current = self.current_token();
                if self.no_trivia_between(operator_token2, current)
                    && matches!(self.current_kind(), SyntaxKind::GreaterThanToken | SyntaxKind::GreaterThanEqualsToken)
                {
                    let operator_token3 = self.eat_token();

                    if self.arena_ref().kind(operator_token3) == SyntaxKind::GreaterThanToken {
                        operator_token = self.merge_tokens(
                            &[operator_token, operator_token2, operator_token3],
                            SyntaxKind::GreaterThanGreaterThanGreaterThanToken,
                        );

                        operator_token = self.check_feature_availability(
                            operator_token,
                            MessageID::IDS_FeatureUnsignedRightShift,
                            true,
                        );
                    } else {
                        operator_token = self.merge_tokens(
                            &[operator_token, operator_token2, operator_token3],
                            SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken,
                        );
                        operator_token = self.check_feature_availability(
                            operator_token,
                            MessageID::IDS_FeatureUserDefinedCompoundAssignmentOperators,
                            true,
                        );
                    }
                } else {
                    operator_token =
                        self.merge_tokens(&[operator_token, operator_token2], SyntaxKind::GreaterThanGreaterThanToken);
                }
            } else if self.current_kind() == SyntaxKind::EqualsToken {
                let operator_token2 = self.eat_token();
                operator_token =
                    self.merge_tokens(&[operator_token, operator_token2], SyntaxKind::GreaterThanEqualsToken);
            } else if self.current_kind() == SyntaxKind::GreaterThanEqualsToken {
                let operator_token2 = self.eat_token();
                operator_token = self
                    .merge_tokens(&[operator_token, operator_token2], SyntaxKind::GreaterThanGreaterThanEqualsToken);
                operator_token = self.check_feature_availability(
                    operator_token,
                    MessageID::IDS_FeatureUserDefinedCompoundAssignmentOperators,
                    true,
                );
            }
        }

        let compound = match self.arena_ref().kind(operator_token) {
            SyntaxKind::PlusToken => Some(SyntaxKind::PlusEqualsToken),
            SyntaxKind::MinusToken => Some(SyntaxKind::MinusEqualsToken),
            SyntaxKind::AsteriskToken => Some(SyntaxKind::AsteriskEqualsToken),
            SyntaxKind::SlashToken => Some(SyntaxKind::SlashEqualsToken),
            SyntaxKind::PercentToken => Some(SyntaxKind::PercentEqualsToken),
            SyntaxKind::AmpersandToken => Some(SyntaxKind::AmpersandEqualsToken),
            SyntaxKind::BarToken => Some(SyntaxKind::BarEqualsToken),
            SyntaxKind::CaretToken => Some(SyntaxKind::CaretEqualsToken),
            SyntaxKind::LessThanLessThanToken => Some(SyntaxKind::LessThanLessThanEqualsToken),
            SyntaxKind::GreaterThanGreaterThanToken => Some(SyntaxKind::GreaterThanGreaterThanEqualsToken),
            SyntaxKind::GreaterThanGreaterThanGreaterThanToken => {
                Some(SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken)
            }
            _ => None,
        };
        if let Some(kind) = compound {
            // `tryParseCompoundAssignmentOperatorToken(operatorToken, kind)`
            let current = self.current_token();
            if self.no_trivia_between(operator_token, current) && self.current_kind() == SyntaxKind::EqualsToken {
                debug_assert!(!matches!(
                    kind,
                    SyntaxKind::GreaterThanGreaterThanEqualsToken
                        | SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
                )); // Reaching this code path is not expected
                let operator_token2 = self.eat_token();
                operator_token = self.merge_tokens(&[operator_token, operator_token2], kind);
                operator_token = self.check_feature_availability(
                    operator_token,
                    MessageID::IDS_FeatureUserDefinedCompoundAssignmentOperators,
                    true,
                );
            }
        }

        debug_assert!(facts::is_any_overloadable_operator(self.arena_ref().kind(operator_token)));

        let parameters = self.parse_cref_parameter_list();

        self.base.arena().operator_member_cref(operator_keyword, checked_keyword, operator_token, parameters)
    }

    /// `LanguageParser.NoTriviaBetween(token1, token2)` (LanguageParser.cs:5033), over this parser's arena.
    fn no_trivia_between(&self, token1: Green, token2: Green) -> bool {
        let a = self.arena_ref();
        a.trailing_trivia_width(token1) == 0 && a.leading_trivia_width(token2) == 0
    }

    /// `SyntaxFactory.Token(first.GetLeadingTrivia(), kind, first.Text + ... + last.Text, first.ValueText + ... +
    /// last.ValueText, last.GetTrailingTrivia())`: the tokens, with no trivia between them, as one. Its text is the
    /// source's when the texts are adjacent there (missing ones have none), else a string of the arena.
    fn merge_tokens(&mut self, tokens: &[Green], kind: SyntaxKind) -> Green {
        let (Some(&first), Some(&last)) = (tokens.first(), tokens.last()) else { return Green::NULL };
        let source = self.src();
        let mut text = std::mem::take(&mut self.text);
        let mut value = std::mem::take(&mut self.other_text);
        text.clear();
        value.clear();
        let a = self.base.arena();
        let mut range: Option<(u32, u32)> = None;
        let mut contiguous = true;
        for &token in tokens {
            text.extend_from_slice(a.token_text(token, source));
            value.extend_from_slice(a.token_value_text(token, source));
            if a.width(token) == 0 {
                continue;
            }
            match (a.token_source_range(token), range) {
                (Some((start, width)), None) => range = Some((start, width)),
                (Some((start, width)), Some((s, w))) if s + w == start => range = Some((s, w + width)),
                _ => contiguous = false,
            }
        }
        let token_text = match range {
            Some((start, width)) if contiguous => crate::green::TokenText::Source { start, width },
            _ => {
                let (start, len) = a.intern(&String::from_utf8_lossy(&text));
                crate::green::TokenText::Owned { start, len }
            }
        };
        let leading = a.leading_trivia(first);
        let trailing = a.trailing_trivia(last);
        let token = a.create_token(leading, kind, kind, token_text, trailing);
        if value != text {
            a.set_token_value(token, &String::from_utf8_lossy(&value));
        }
        self.text = text;
        self.other_text = value;
        token
    }

    /// `private SyntaxToken TryEatCheckedKeyword(bool isConversion, ref SyntaxToken operatorKeyword)` (DocumentationCommentParser.cs:1176).
    pub(crate) fn try_eat_checked_keyword(&mut self, is_conversion: bool, operator_keyword: &mut Green) -> Green {
        // `tryEatCheckedOrHandleUnchecked(ref operatorKeyword)`
        let mut checked_keyword = if self.current_kind() == SyntaxKind::UncheckedKeyword {
            // if we encounter `operator unchecked`, we place the `unchecked` as skipped trivia on `operator`
            let token = self.eat_token();
            let misplaced_token = self.add_error_as_warning(token, ErrorCode::ERR_MisplacedUnchecked, &[]);
            *operator_keyword = self.add_trailing_skipped_syntax(*operator_keyword, misplaced_token);
            Green::NULL
        } else {
            self.try_eat_token(SyntaxKind::CheckedKeyword)
        };

        if !checked_keyword.is_null() && (is_conversion || facts::is_any_overloadable_operator(self.current_kind())) {
            checked_keyword = self.check_feature_availability(
                checked_keyword,
                MessageID::IDS_FeatureCheckedUserDefinedOperators,
                true,
            );
        }

        checked_keyword
    }

    /// `private ConversionOperatorMemberCrefSyntax ParseConversionOperatorMemberCref()` (DocumentationCommentParser.cs:1205).
    pub(crate) fn parse_conversion_operator_member_cref(&mut self) -> Green {
        debug_assert!(matches!(self.current_kind(), SyntaxKind::ExplicitKeyword | SyntaxKind::ImplicitKeyword));
        let implicit_or_explicit = self.eat_token();

        let mut operator_keyword = self.eat_token_kind(SyntaxKind::OperatorKeyword);
        let checked_keyword = self.try_eat_checked_keyword(true, &mut operator_keyword);

        let type_ = self.parse_cref_type(false, false);

        let parameters = self.parse_cref_parameter_list();

        self.base.arena().conversion_operator_member_cref(
            implicit_or_explicit,
            operator_keyword,
            checked_keyword,
            type_,
            parameters,
        )
    }

    /// `private CrefParameterListSyntax ParseCrefParameterList()` (DocumentationCommentParser.cs:1224).
    pub(crate) fn parse_cref_parameter_list(&mut self) -> Green {
        self.parse_base_cref_parameter_list(false)
    }

    /// `private CrefBracketedParameterListSyntax ParseBracketedCrefParameterList()` (DocumentationCommentParser.cs:1232).
    pub(crate) fn parse_bracketed_cref_parameter_list(&mut self) -> Green {
        self.parse_base_cref_parameter_list(true)
    }

    /// `private BaseCrefParameterListSyntax ParseBaseCrefParameterList(bool useSquareBrackets)` (DocumentationCommentParser.cs:1240).
    pub(crate) fn parse_base_cref_parameter_list(&mut self, use_square_brackets: bool) -> Green {
        let open_kind = if use_square_brackets { SyntaxKind::OpenBracketToken } else { SyntaxKind::OpenParenToken };
        let close_kind = if use_square_brackets { SyntaxKind::CloseBracketToken } else { SyntaxKind::CloseParenToken };

        if self.current_kind() != open_kind {
            return Green::NULL;
        }

        let open = self.eat_token_kind(open_kind);

        let mut list = self.pool.allocate();
        while self.current_kind() == SyntaxKind::CommaToken || self.is_possible_cref_parameter() {
            let parameter = self.parse_cref_parameter();
            list.push(parameter);

            if self.current_kind() != close_kind {
                let comma = self.eat_token_kind(SyntaxKind::CommaToken);
                if !self.arena_ref().is_missing(comma) || self.is_possible_cref_parameter() {
                    // Only do this if it won't be last in the list.
                    list.push(comma);
                } else {
                    // How could this scenario arise?  If it does, just expand the if-condition.
                    debug_assert!(self.current_kind() != SyntaxKind::CommaToken);
                }
            }
        }

        // NOTE: nothing follows a cref parameter list, so there's no reason to recover here.
        // Just let the cref-level recovery code handle any remaining tokens.

        let close = self.eat_token_kind(close_kind);

        let a = self.base.arena();
        let parameters = a.list(&list);
        let result = if use_square_brackets {
            a.cref_bracketed_parameter_list(open, parameters, close)
        } else {
            a.cref_parameter_list(open, parameters, close)
        };
        self.pool.free(list);
        result
    }

    /// `private bool IsPossibleCrefParameter()` (DocumentationCommentParser.cs:1293).
    pub(crate) fn is_possible_cref_parameter(&mut self) -> bool {
        let kind = self.current_kind();
        match kind {
            SyntaxKind::RefKeyword | SyntaxKind::OutKeyword | SyntaxKind::InKeyword | SyntaxKind::IdentifierToken => {
                true
            }
            _ => facts::is_predefined_type(kind),
        }
    }

    /// `private CrefParameterSyntax ParseCrefParameter()` (DocumentationCommentParser.cs:1314).
    pub(crate) fn parse_cref_parameter(&mut self) -> Green {
        let mut ref_kind_opt = Green::NULL;
        if matches!(self.current_kind(), SyntaxKind::RefKeyword | SyntaxKind::OutKeyword | SyntaxKind::InKeyword) {
            ref_kind_opt = self.eat_token();
        }

        let mut read_only_opt = Green::NULL;
        if self.current_kind() == SyntaxKind::ReadOnlyKeyword && !ref_kind_opt.is_null() {
            if self.arena_ref().kind(ref_kind_opt) != SyntaxKind::RefKeyword {
                // if we encounter `readonly` after `in` or `out`, we place the `readonly` as skipped trivia on the previous keyword
                let token = self.eat_token();
                let misplaced_token = self.add_error_as_warning(token, ErrorCode::ERR_RefReadOnlyWrongOrdering, &[]);
                ref_kind_opt = self.add_trailing_skipped_syntax(ref_kind_opt, misplaced_token);
            } else {
                read_only_opt = self.eat_token();
            }
        }

        let type_ = self.parse_cref_type(false, false);
        self.base.arena().cref_parameter(ref_kind_opt, read_only_opt, type_)
    }

    /// `private SimpleNameSyntax ParseCrefName(bool typeArgumentsMustBeIdentifiers)` (DocumentationCommentParser.cs:1350);
    /// past the nesting limit a name takes no type argument list (see the module's documentation).
    pub(crate) fn parse_cref_name(&mut self, type_arguments_must_be_identifiers: bool) -> Green {
        let identifier_token = self.eat_token_kind(SyntaxKind::IdentifierToken);

        if self.current_kind() != SyntaxKind::LessThanToken || self.depth >= MAX_NESTED_SCAN_DEPTH {
            return self.base.arena().identifier_name(identifier_token);
        }

        let type_arguments = self.parse_type_arguments(type_arguments_must_be_identifiers);
        self.base.arena().generic_name(identifier_token, type_arguments)
    }

    /// `private TypeArgumentListSyntax ParseTypeArguments(bool typeArgumentsMustBeIdentifiers)` (DocumentationCommentParser.cs:1362).
    pub(crate) fn parse_type_arguments(&mut self, type_arguments_must_be_identifiers: bool) -> Green {
        debug_assert!(self.current_kind() == SyntaxKind::LessThanToken);
        let mut open = self.eat_token();

        let mut list = self.pool.allocate();
        self.depth += 1;
        loop {
            let mut type_syntax = self.parse_cref_type(type_arguments_must_be_identifiers, false);

            if type_arguments_must_be_identifiers && self.arena_ref().kind(type_syntax) != SyntaxKind::IdentifierName {
                let info = self.make_error_code(ErrorCode::ERR_TypeParamMustBeIdentifier, &[]);
                // `$"{(int)ErrorCode.ERR_TypeParamMustBeIdentifier:d4}"`
                let (start, len) =
                    self.base.arena().intern(&format!("{:04}", ErrorCode::ERR_TypeParamMustBeIdentifier.number()));
                type_syntax = self.add_error_args(
                    type_syntax,
                    ErrorCode::WRN_ErrorOverride,
                    &[DiagArg::Info(info), DiagArg::Owned { start, len }],
                );
            }

            list.push(type_syntax);

            let current_kind = self.current_kind();
            if current_kind == SyntaxKind::CommaToken
                || current_kind == SyntaxKind::IdentifierToken
                || facts::is_predefined_type(current_kind)
            {
                // NOTE: if the current token is an identifier or predefined type, then we're
                // actually inserting a missing commas.
                let comma = self.eat_token_kind(SyntaxKind::CommaToken);
                list.push(comma);
            } else {
                break;
            }
        }
        self.depth -= 1;

        let close = self.eat_token_kind(SyntaxKind::GreaterThanToken);

        open = self.check_feature_availability(open, MessageID::IDS_FeatureGenerics, true);

        let a = self.base.arena();
        let arguments = a.list(&list);
        let result = a.type_argument_list(open, arguments, close);
        self.pool.free(list);
        result
    }

    /// `private TypeSyntax ParseCrefType(bool typeArgumentsMustBeIdentifiers, bool checkForMember = false)` (DocumentationCommentParser.cs:1420).
    pub(crate) fn parse_cref_type(&mut self, type_arguments_must_be_identifiers: bool, check_for_member: bool) -> Green {
        let type_without_suffix = self.parse_cref_type_helper(type_arguments_must_be_identifiers, check_for_member);
        if type_arguments_must_be_identifiers {
            type_without_suffix
        } else {
            self.parse_cref_type_suffix(type_without_suffix)
        }
    }

    /// `private TypeSyntax ParseCrefTypeHelper(bool typeArgumentsMustBeIdentifiers, bool checkForMember = false)` (DocumentationCommentParser.cs:1440).
    pub(crate) fn parse_cref_type_helper(
        &mut self,
        type_arguments_must_be_identifiers: bool,
        check_for_member: bool,
    ) -> Green {
        let mut left_name;

        if facts::is_predefined_type(self.current_kind()) {
            // e.g. "int"
            // NOTE: a predefined type will not fit into a NameSyntax, so we'll return
            // immediately.  The upshot is that you can only dot into a predefined type
            // once (e.g. not "int.A.B"), which is fine because we know that none of them
            // have nested types.
            let keyword = self.eat_token();
            return self.base.arena().predefined_type(keyword);
        } else if self.current_kind() == SyntaxKind::IdentifierToken && self.peek_kind(1) == SyntaxKind::ColonColonToken
        {
            // e.g. "A::B"
            let mut alias = self.eat_token();
            if self.arena_ref().contextual_kind(alias) == SyntaxKind::GlobalKeyword {
                alias = self.convert_to_keyword(alias);
            }

            alias = self.check_feature_availability(alias, MessageID::IDS_FeatureGlobalNamespace, true);

            let colon_colon = self.eat_token();
            let name = self.parse_cref_name(type_arguments_must_be_identifiers);
            let a = self.base.arena();
            let alias_name = a.identifier_name(alias);
            left_name = a.alias_qualified_name(alias_name, colon_colon, name);
        } else {
            // e.g. "A"
            let reset_point = self.base.get_reset_point();
            left_name = self.parse_cref_name(type_arguments_must_be_identifiers);
            if check_for_member
                && (self.arena_ref().is_missing(left_name) || self.current_kind() != SyntaxKind::DotToken)
            {
                // If this isn't the first part of a dotted name, then we prefer to represent it
                // as a MemberCrefSyntax.
                self.base.reset(&reset_point);
                self.base.release(&reset_point);

                return Green::NULL;
            }
            self.base.release(&reset_point);
        }

        while self.current_kind() == SyntaxKind::DotToken {
            // NOTE: we make a lot of these, but we'll reset, at most, one time.
            let reset_point = self.base.get_reset_point();

            let dot = self.eat_token();

            let right_name = self.parse_cref_name(type_arguments_must_be_identifiers);

            if check_for_member
                && (self.arena_ref().is_missing(right_name) || self.current_kind() != SyntaxKind::DotToken)
            {
                self.base.reset(&reset_point); // Go back to before the dot - it must have been the trailing dot.
                self.base.release(&reset_point);

                return left_name;
            }

            self.base.release(&reset_point);

            left_name = self.base.arena().qualified_name(left_name, dot, right_name);
        }

        left_name
    }

    /// `private TypeSyntax ParseCrefTypeSuffix(TypeSyntax type)` (DocumentationCommentParser.cs:1514).
    pub(crate) fn parse_cref_type_suffix(&mut self, type_: Green) -> Green {
        let mut type_ = type_;
        if self.current_kind() == SyntaxKind::QuestionToken {
            let question = self.eat_token();
            type_ = self.base.arena().nullable_type(type_, question);
        }

        while self.current_kind() == SyntaxKind::AsteriskToken {
            let asterisk = self.eat_token();
            type_ = self.base.arena().pointer_type(type_, asterisk);
        }

        if self.current_kind() == SyntaxKind::OpenBracketToken {
            let a = self.base.arena();
            let omitted_token = a.token(SyntaxKind::OmittedArraySizeExpressionToken);
            let omitted_array_size_expression_instance = a.omitted_array_size_expression(omitted_token);
            let mut rank_list = self.pool.allocate();
            while self.current_kind() == SyntaxKind::OpenBracketToken {
                let open = self.eat_token();
                let mut dimension_list = self.pool.allocate();
                while self.current_kind() != SyntaxKind::CloseBracketToken {
                    if self.current_kind() == SyntaxKind::CommaToken {
                        // NOTE: trivia will be attached to comma, not omitted array size
                        dimension_list.push(omitted_array_size_expression_instance);
                        let comma = self.eat_token();
                        dimension_list.push(comma);
                    } else {
                        // CONSIDER: if we expect people to try to put expressions in between
                        // the commas, then it might be more reasonable to recover by skipping
                        // tokens until we hit a CloseBracketToken (or some other terminator).
                        break;
                    }
                }

                // Don't end on a comma.
                // If the omitted size would be the only element, then skip it unless sizes were expected.
                if dimension_list.len() & 1 == 0 {
                    dimension_list.push(omitted_array_size_expression_instance);
                }

                // Eat the close brace and we're done.
                let close = self.eat_token_kind(SyntaxKind::CloseBracketToken);

                let a = self.base.arena();
                let sizes = a.list(&dimension_list);
                rank_list.push(a.array_rank_specifier(open, sizes, close));
                self.pool.free(dimension_list);
            }

            let a = self.base.arena();
            let rank_specifiers = a.list(&rank_list);
            type_ = a.array_type(type_, rank_specifiers);
            self.pool.free(rank_list);
        }
        type_
    }

    /// `private bool IsEndOfCrefAttribute` (DocumentationCommentParser.cs:1586).
    pub(crate) fn is_end_of_cref_attribute(&mut self) -> bool {
        match self.current_kind() {
            SyntaxKind::SingleQuoteToken => self.mode().and(LexerMode::XML_CREF_QUOTE) == LexerMode::XML_CREF_QUOTE,
            SyntaxKind::DoubleQuoteToken => {
                self.mode().and(LexerMode::XML_CREF_DOUBLE_QUOTE) == LexerMode::XML_CREF_DOUBLE_QUOTE
            }
            SyntaxKind::EndOfFileToken | SyntaxKind::EndOfDocumentationCommentToken => true,
            SyntaxKind::BadToken => {
                // If it's a real '<' (not &lt;, etc), then we assume it's the beginning
                // of the next XML element.
                let current = self.current_token();
                self.arena_ref().token_text(current, self.src()) == facts::get_text(SyntaxKind::LessThanToken).as_bytes()
                    || self.is_non_ascii_quotation_mark(current)
            }
            _ => false,
        }
    }

    /// `private bool InCref` (DocumentationCommentParser.cs:1613).
    pub(crate) fn in_cref(&mut self) -> bool {
        in_cref_mode(self.mode())
    }

    /// `private IdentifierNameSyntax ParseNameAttributeValue()` (DocumentationCommentParser.cs:1632).
    pub(crate) fn parse_name_attribute_value(&mut self) -> Green {
        // Never report a parse error - just fail to bind the name later on.
        let mut identifier_token = self.eat_token_report(SyntaxKind::IdentifierToken, false);

        if !self.is_end_of_name_attribute() {
            let mut bad_tokens = self.pool.allocate();
            while !self.is_end_of_name_attribute() {
                let token = self.eat_token();
                bad_tokens.push(token);
            }
            let skipped = self.base.arena().list(&bad_tokens);
            identifier_token = self.add_trailing_skipped_syntax(identifier_token, skipped);
            self.pool.free(bad_tokens);
        }

        self.base.arena().identifier_name(identifier_token)
    }

    /// `private bool IsEndOfNameAttribute` (DocumentationCommentParser.cs:1654).
    pub(crate) fn is_end_of_name_attribute(&mut self) -> bool {
        match self.current_kind() {
            SyntaxKind::SingleQuoteToken => self.mode().and(LexerMode::XML_NAME_QUOTE) == LexerMode::XML_NAME_QUOTE,
            SyntaxKind::DoubleQuoteToken => {
                self.mode().and(LexerMode::XML_NAME_DOUBLE_QUOTE) == LexerMode::XML_NAME_DOUBLE_QUOTE
            }
            SyntaxKind::EndOfFileToken | SyntaxKind::EndOfDocumentationCommentToken => true,
            SyntaxKind::BadToken => {
                // If it's a real '<' (not &lt;, etc), then we assume it's the beginning
                // of the next XML element.
                let current = self.current_token();
                self.arena_ref().token_text(current, self.src()) == facts::get_text(SyntaxKind::LessThanToken).as_bytes()
                    || self.is_non_ascii_quotation_mark(current)
            }
            _ => false,
        }
    }
}
