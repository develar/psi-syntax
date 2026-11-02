//! `DocumentationCommentParser.cs`: the XML of documentation comments and crefs, run by the lexer on each
//! documentation comment it meets (`Lexer.LexXmlDocComment`) over the same lexer in the XML modes (lane D). Every
//! method in C# order; see `parser/mod.rs` for the conventions.

use super::*;
use crate::lexer::Lexer;
use crate::syntax_parser::{ParserKind, SyntaxParserBuffers};

/// `DocumentationCommentParser`: a `SyntaxParser` (`Deref`) with mode resets over the lexer that lexes the comment.
pub struct DocumentationCommentParser<'l, 't> {
    pub(crate) base: SyntaxParser<'l, 't>,
    /// `_pool`.
    pub(crate) pool: SyntaxListPool,
    /// `_isDelimited`.
    pub(crate) is_delimited: bool,
    /// `_attributesSeen` (DocumentationCommentParser.cs:321): the attribute names of the element being parsed.
    pub(crate) attributes_seen: Vec<Vec<u8>>,
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
    pub fn new(lexer: &'l mut Lexer<'t>, buffers: SyntaxParserBuffers, pool: SyntaxListPool) -> Self {
        DocumentationCommentParser {
            base: SyntaxParser::new(lexer, LexerMode::NONE, ParserKind::DocumentationComment, true, false, buffers),
            pool,
            is_delimited: false,
            attributes_seen: Vec::new(),
        }
    }

    /// `Dispose()`: the buffers back for reuse.
    pub fn into_buffers(self) -> (SyntaxParserBuffers, SyntaxListPool) {
        (self.base.into_buffers(), self.pool)
    }
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
    if base.in_cref {
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
    if base.in_cref {
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

// `WithAdditionalDiagnostics` (DocumentationCommentParser.cs:846), the third override, is
// `SyntaxParser::with_additional_diagnostics` with `ParserKind::DocumentationComment`: no diagnostic is attached
// below `DocumentationMode::Diagnose`.

// DocumentationCommentParser.cs:37: `private readonly SyntaxListPool _pool = new SyntaxListPool();`

// DocumentationCommentParser.cs:38: `private bool _isDelimited;`

// DocumentationCommentParser.cs:40: `internal DocumentationCommentParser(Lexer lexer) : base(lexer, LexerMode.None, oldTree: null, changes: null, allowModeReset: true)`

impl<'l, 't> DocumentationCommentParser<'l, 't> {
    /// `internal void ReInitialize(LexerMode modeflags)` (DocumentationCommentParser.cs:45).
    pub(crate) fn re_initialize(&mut self, modeflags: LexerMode) {
        // TODO(port): DocumentationCommentParser.cs:45
    }

    /// `private LexerMode SetMode(LexerMode mode)` (DocumentationCommentParser.cs:52).
    pub(crate) fn set_mode(&mut self, mode: LexerMode) -> LexerMode {
        // TODO(port): DocumentationCommentParser.cs:52
        Default::default()
    }

    /// `private void ResetMode(LexerMode mode)` (DocumentationCommentParser.cs:59).
    pub(crate) fn reset_mode(&mut self, mode: LexerMode) {
        // TODO(port): DocumentationCommentParser.cs:59
    }

    /// `public DocumentationCommentTriviaSyntax ParseDocumentationComment(out bool isTerminated)` (DocumentationCommentParser.cs:64).
    pub(crate) fn parse_documentation_comment(&mut self, is_terminated: &mut bool) -> Green {
        // TODO(port): DocumentationCommentParser.cs:64
        Default::default()
    }

    /// `public void ParseRemainder(SyntaxListBuilder<XmlNodeSyntax> nodes)` (DocumentationCommentParser.cs:99).
    pub(crate) fn parse_remainder(&mut self, nodes: &mut SyntaxListBuilder) {
        // TODO(port): DocumentationCommentParser.cs:99
    }

    /// `private void ParseXmlNodes(SyntaxListBuilder<XmlNodeSyntax> nodes)` (DocumentationCommentParser.cs:131).
    pub(crate) fn parse_xml_nodes(&mut self, nodes: &mut SyntaxListBuilder) {
        // TODO(port): DocumentationCommentParser.cs:131
    }

    /// `private XmlNodeSyntax ParseXmlNode()` (DocumentationCommentParser.cs:145).
    pub(crate) fn parse_xml_node(&mut self) -> Green {
        // TODO(port): DocumentationCommentParser.cs:145
        Default::default()
    }

    /// `private bool IsXmlNodeStartOrStop()` (DocumentationCommentParser.cs:169).
    pub(crate) fn is_xml_node_start_or_stop(&mut self) -> bool {
        // TODO(port): DocumentationCommentParser.cs:169
        Default::default()
    }

    /// `private XmlNodeSyntax ParseXmlText()` (DocumentationCommentParser.cs:187).
    pub(crate) fn parse_xml_text(&mut self) -> Green {
        // TODO(port): DocumentationCommentParser.cs:187
        Default::default()
    }

    /// `private XmlNodeSyntax ParseXmlElement()` (DocumentationCommentParser.cs:202).
    pub(crate) fn parse_xml_element(&mut self) -> Green {
        // TODO(port): DocumentationCommentParser.cs:202
        Default::default()
    }

    /// `private static bool MatchingXmlNames(XmlNameSyntax name, XmlNameSyntax endName)` (DocumentationCommentParser.cs:297).
    pub(crate) fn matching_xml_names(name: Green, end_name: Green) -> bool {
        // TODO(port): DocumentationCommentParser.cs:297
        Default::default()
    }

    // DocumentationCommentParser.cs:321: `private readonly HashSet<string> _attributesSeen = new HashSet<string>();`
    /// `private void ParseXmlAttributes(ref XmlNameSyntax elementName, SyntaxListBuilder<XmlAttributeSyntax> attrs)` (DocumentationCommentParser.cs:323).
    pub(crate) fn parse_xml_attributes(&mut self, element_name: &mut Green, attrs: &mut SyntaxListBuilder) {
        // TODO(port): DocumentationCommentParser.cs:323
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
        list: &mut SyntaxListBuilder,
        is_not_expected_function: fn(&mut DocumentationCommentParser<'_, '_>) -> bool,
        abort_function: fn(&mut DocumentationCommentParser<'_, '_>) -> bool,
        error: XmlParseErrorCode,
    ) -> SkipResult {
        // TODO(port): DocumentationCommentParser.cs:371
        Default::default()
    }

    /// `private XmlAttributeSyntax ParseXmlAttribute(XmlNameSyntax elementName)` (DocumentationCommentParser.cs:438).
    pub(crate) fn parse_xml_attribute(&mut self, element_name: Green) -> Green {
        // TODO(port): DocumentationCommentParser.cs:438
        Default::default()
    }

    /// `private static bool XmlElementSupportsNameAttribute(XmlNameSyntax elementName)` (DocumentationCommentParser.cs:502).
    pub(crate) fn xml_element_supports_name_attribute(element_name: Green) -> bool {
        // TODO(port): DocumentationCommentParser.cs:502
        Default::default()
    }

    /// `private bool IsVerbatimCref()` (DocumentationCommentParser.cs:517).
    pub(crate) fn is_verbatim_cref(&mut self) -> bool {
        // TODO(port): DocumentationCommentParser.cs:517
        Default::default()
    }

    /// `private void ParseCrefAttribute(out SyntaxToken startQuote, out CrefSyntax cref, out SyntaxToken endQuote)` (DocumentationCommentParser.cs:553).
    pub(crate) fn parse_cref_attribute(&mut self, start_quote: &mut Green, cref: &mut Green, end_quote: &mut Green) {
        // TODO(port): DocumentationCommentParser.cs:553
    }

    /// `private void ParseNameAttribute(out SyntaxToken startQuote, out IdentifierNameSyntax identifier, out SyntaxToken endQuote)` (DocumentationCommentParser.cs:571).
    pub(crate) fn parse_name_attribute(
        &mut self,
        start_quote: &mut Green,
        identifier: &mut Green,
        end_quote: &mut Green,
    ) {
        // TODO(port): DocumentationCommentParser.cs:571
    }

    /// `private void ParseXmlAttributeText(out SyntaxToken startQuote, SyntaxListBuilder<SyntaxToken> textTokens, out SyntaxToken endQuote)` (DocumentationCommentParser.cs:589).
    pub(crate) fn parse_xml_attribute_text(
        &mut self,
        start_quote: &mut Green,
        text_tokens: &mut SyntaxListBuilder,
        end_quote: &mut Green,
    ) {
        // TODO(port): DocumentationCommentParser.cs:589
    }

    /// `private SyntaxToken ParseXmlAttributeStartQuote()` (DocumentationCommentParser.cs:629).
    pub(crate) fn parse_xml_attribute_start_quote(&mut self) -> Green {
        // TODO(port): DocumentationCommentParser.cs:629
        Default::default()
    }

    /// `private SyntaxToken ParseXmlAttributeEndQuote(SyntaxKind quoteKind)` (DocumentationCommentParser.cs:648).
    pub(crate) fn parse_xml_attribute_end_quote(&mut self, quote_kind: SyntaxKind) -> Green {
        // TODO(port): DocumentationCommentParser.cs:648
        Default::default()
    }

    /// `private SyntaxToken SkipNonAsciiQuotationMark()` (DocumentationCommentParser.cs:663).
    pub(crate) fn skip_non_ascii_quotation_mark(&mut self) -> Green {
        // TODO(port): DocumentationCommentParser.cs:663
        Default::default()
    }

    /// `private static bool IsNonAsciiQuotationMark(SyntaxToken token)` (DocumentationCommentParser.cs:676).
    pub(crate) fn is_non_ascii_quotation_mark(token: Green) -> bool {
        // TODO(port): DocumentationCommentParser.cs:676
        Default::default()
    }

    /// `private XmlNameSyntax ParseXmlName()` (DocumentationCommentParser.cs:681).
    pub(crate) fn parse_xml_name(&mut self) -> Green {
        // TODO(port): DocumentationCommentParser.cs:681
        Default::default()
    }

    /// `private XmlCommentSyntax ParseXmlComment()` (DocumentationCommentParser.cs:720).
    pub(crate) fn parse_xml_comment(&mut self) -> Green {
        // TODO(port): DocumentationCommentParser.cs:720
        Default::default()
    }

    /// `private XmlCDataSectionSyntax ParseXmlCDataSection()` (DocumentationCommentParser.cs:747).
    pub(crate) fn parse_xml_c_data_section(&mut self) -> Green {
        // TODO(port): DocumentationCommentParser.cs:747
        Default::default()
    }

    /// `private XmlProcessingInstructionSyntax ParseXmlProcessingInstruction()` (DocumentationCommentParser.cs:763).
    pub(crate) fn parse_xml_processing_instruction(&mut self) -> Green {
        // TODO(port): DocumentationCommentParser.cs:763
        Default::default()
    }

    /// `private TNode WithXmlParseError<TNode>(TNode node, XmlParseErrorCode code) where TNode : CSharpSyntaxNode` (DocumentationCommentParser.cs:831).
    pub(crate) fn with_xml_parse_error(&mut self, node: Green, code: XmlParseErrorCode) -> Green {
        // TODO(port): DocumentationCommentParser.cs:831
        Default::default()
    }

    /// `private TNode WithXmlParseError<TNode>(TNode node, XmlParseErrorCode code, params string[] args) where TNode : CSharpSyntaxNode` (DocumentationCommentParser.cs:836).
    pub(crate) fn with_xml_parse_error_with_args(
        &mut self,
        node: Green,
        code: XmlParseErrorCode,
        args: &[&str],
    ) -> Green {
        // TODO(port): DocumentationCommentParser.cs:836
        Default::default()
    }

    /// `private SyntaxToken WithXmlParseError(SyntaxToken node, XmlParseErrorCode code, params string[] args)` (DocumentationCommentParser.cs:841).
    pub(crate) fn with_xml_parse_error_with_args_2(
        &mut self,
        node: Green,
        code: XmlParseErrorCode,
        args: &[&str],
    ) -> Green {
        // TODO(port): DocumentationCommentParser.cs:841
        Default::default()
    }

    /// `protected override TNode WithAdditionalDiagnostics<TNode>(TNode node, params DiagnosticInfo[] diagnostics)` (DocumentationCommentParser.cs:846).
    pub(crate) fn with_additional_diagnostics(&mut self, node: Green, diagnostics: &[DiagnosticId]) -> Green {
        // TODO(port): DocumentationCommentParser.cs:846
        Default::default()
    }

    /// `private CrefSyntax ParseCrefAttributeValue()` (DocumentationCommentParser.cs:887).
    pub(crate) fn parse_cref_attribute_value(&mut self) -> Green {
        // TODO(port): DocumentationCommentParser.cs:887
        Default::default()
    }

    /// `private MemberCrefSyntax ParseMemberCref()` (DocumentationCommentParser.cs:938).
    pub(crate) fn parse_member_cref(&mut self) -> Green {
        // TODO(port): DocumentationCommentParser.cs:938
        Default::default()
    }

    /// `private NameMemberCrefSyntax ParseNameMemberCref()` (DocumentationCommentParser.cs:960).
    pub(crate) fn parse_name_member_cref(&mut self) -> Green {
        // TODO(port): DocumentationCommentParser.cs:960
        Default::default()
    }

    /// `private IndexerMemberCrefSyntax ParseIndexerMemberCref()` (DocumentationCommentParser.cs:971).
    pub(crate) fn parse_indexer_member_cref(&mut self) -> Green {
        // TODO(port): DocumentationCommentParser.cs:971
        Default::default()
    }

    /// `private MemberCrefSyntax ParsePossibleExtensionMemberCref()` (DocumentationCommentParser.cs:985).
    pub(crate) fn parse_possible_extension_member_cref(&mut self) -> Green {
        // TODO(port): DocumentationCommentParser.cs:985
        Default::default()
    }

    /// `private OperatorMemberCrefSyntax ParseOperatorMemberCref()` (DocumentationCommentParser.cs:1016).
    pub(crate) fn parse_operator_member_cref(&mut self) -> Green {
        // TODO(port): DocumentationCommentParser.cs:1016
        Default::default()
    }

    /// `private SyntaxToken TryEatCheckedKeyword(bool isConversion, ref SyntaxToken operatorKeyword)` (DocumentationCommentParser.cs:1176).
    pub(crate) fn try_eat_checked_keyword(&mut self, is_conversion: bool, operator_keyword: &mut Green) -> Green {
        // TODO(port): DocumentationCommentParser.cs:1176
        Default::default()
    }

    /// `private ConversionOperatorMemberCrefSyntax ParseConversionOperatorMemberCref()` (DocumentationCommentParser.cs:1205).
    pub(crate) fn parse_conversion_operator_member_cref(&mut self) -> Green {
        // TODO(port): DocumentationCommentParser.cs:1205
        Default::default()
    }

    /// `private CrefParameterListSyntax ParseCrefParameterList()` (DocumentationCommentParser.cs:1224).
    pub(crate) fn parse_cref_parameter_list(&mut self) -> Green {
        // TODO(port): DocumentationCommentParser.cs:1224
        Default::default()
    }

    /// `private CrefBracketedParameterListSyntax ParseBracketedCrefParameterList()` (DocumentationCommentParser.cs:1232).
    pub(crate) fn parse_bracketed_cref_parameter_list(&mut self) -> Green {
        // TODO(port): DocumentationCommentParser.cs:1232
        Default::default()
    }

    /// `private BaseCrefParameterListSyntax ParseBaseCrefParameterList(bool useSquareBrackets)` (DocumentationCommentParser.cs:1240).
    pub(crate) fn parse_base_cref_parameter_list(&mut self, use_square_brackets: bool) -> Green {
        // TODO(port): DocumentationCommentParser.cs:1240
        Default::default()
    }

    /// `private bool IsPossibleCrefParameter()` (DocumentationCommentParser.cs:1293).
    pub(crate) fn is_possible_cref_parameter(&mut self) -> bool {
        // TODO(port): DocumentationCommentParser.cs:1293
        Default::default()
    }

    /// `private CrefParameterSyntax ParseCrefParameter()` (DocumentationCommentParser.cs:1314).
    pub(crate) fn parse_cref_parameter(&mut self) -> Green {
        // TODO(port): DocumentationCommentParser.cs:1314
        Default::default()
    }

    /// `private SimpleNameSyntax ParseCrefName(bool typeArgumentsMustBeIdentifiers)` (DocumentationCommentParser.cs:1350).
    pub(crate) fn parse_cref_name(&mut self, type_arguments_must_be_identifiers: bool) -> Green {
        // TODO(port): DocumentationCommentParser.cs:1350
        Default::default()
    }

    /// `private TypeArgumentListSyntax ParseTypeArguments(bool typeArgumentsMustBeIdentifiers)` (DocumentationCommentParser.cs:1362).
    pub(crate) fn parse_type_arguments(&mut self, type_arguments_must_be_identifiers: bool) -> Green {
        // TODO(port): DocumentationCommentParser.cs:1362
        Default::default()
    }

    /// `private TypeSyntax ParseCrefType(bool typeArgumentsMustBeIdentifiers, bool checkForMember = false)` (DocumentationCommentParser.cs:1420).
    pub(crate) fn parse_cref_type(
        &mut self,
        type_arguments_must_be_identifiers: bool,
        check_for_member: bool,
    ) -> Green {
        // TODO(port): DocumentationCommentParser.cs:1420
        Default::default()
    }

    /// `private TypeSyntax ParseCrefTypeHelper(bool typeArgumentsMustBeIdentifiers, bool checkForMember = false)` (DocumentationCommentParser.cs:1440).
    pub(crate) fn parse_cref_type_helper(
        &mut self,
        type_arguments_must_be_identifiers: bool,
        check_for_member: bool,
    ) -> Green {
        // TODO(port): DocumentationCommentParser.cs:1440
        Default::default()
    }

    /// `private TypeSyntax ParseCrefTypeSuffix(TypeSyntax type)` (DocumentationCommentParser.cs:1514).
    pub(crate) fn parse_cref_type_suffix(&mut self, type_: Green) -> Green {
        // TODO(port): DocumentationCommentParser.cs:1514
        Default::default()
    }

    /// `private bool IsEndOfCrefAttribute` (DocumentationCommentParser.cs:1586).
    pub(crate) fn is_end_of_cref_attribute(&mut self) -> bool {
        // TODO(port): DocumentationCommentParser.cs:1586
        Default::default()
    }

    /// `private bool InCref` (DocumentationCommentParser.cs:1613).
    pub(crate) fn in_cref(&mut self) -> bool {
        // TODO(port): DocumentationCommentParser.cs:1613
        Default::default()
    }

    /// `private IdentifierNameSyntax ParseNameAttributeValue()` (DocumentationCommentParser.cs:1632).
    pub(crate) fn parse_name_attribute_value(&mut self) -> Green {
        // TODO(port): DocumentationCommentParser.cs:1632
        Default::default()
    }

    /// `private bool IsEndOfNameAttribute` (DocumentationCommentParser.cs:1654).
    pub(crate) fn is_end_of_name_attribute(&mut self) -> bool {
        // TODO(port): DocumentationCommentParser.cs:1654
        Default::default()
    }
}
