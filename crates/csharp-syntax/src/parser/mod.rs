//! `LanguageParser` (Roslyn's `Parser/LanguageParser.cs`, `LanguageParser_Patterns.cs`,
//! `LanguageParser_InterpolatedString.cs`) and `DocumentationCommentParser`, every method a Rust function in C#
//! order. The file split follows `LanguageParser.cs` so that four lanes port it without touching each other's files:
//!
//! | file | holds (LanguageParser.cs lines) | lane |
//! |---|---|---|
//! | `mod.rs` | the state, the entry points, `IsTerminator`, the reset points, `ParseCommaSeparatedSyntaxList`, `ConsumeUnexpectedTokens` (1-235, 14420-) | shared |
//! | `declarations.rs` | namespaces, usings, attributes, type and member declarations, accessors, parameters, fields, variables, enums, names (236-6162) | A |
//! | `types.rs` | type parameters and arguments, `ScanType`, `ParseType`, tuples, function pointers (6163-8247) | A |
//! | `statements.rs` | statements, local declarations and functions (8248-11122) | B |
//! | `expressions.rs` | expressions, lambdas, initializers, queries (11123-14419) | C |
//! | `patterns.rs` | `LanguageParser_Patterns.cs` | C |
//! | `interpolated_string.rs` | `LanguageParser_InterpolatedString.cs` | D |
//! | `doc_comment_parser.rs` | `DocumentationCommentParser.cs` | D |
//!
//! A C# overload takes a suffix after the first (`_with_<last parameter>`); `ref` and `out` parameters are `&mut`, a
//! nullable list builder `Option<SyntaxListBuilder>`.
//!
//! `StackGuard.EnsureSufficientExecutionStack(_recursionDepth)` is `ensure_sufficient_execution_stack`: past
//! `MAX_RECURSION_DEPTH` the parse is abandoned (every further token reads as the end of file, so every loop ends and
//! the recursion unwinds) and `ParseWithStackGuard` returns `CreateForGlobalFailure`, the tree Roslyn makes when it
//! runs out of stack.

// Roslyn's names, parameter types and parameter lists.
#![allow(clippy::enum_variant_names, clippy::ptr_arg, clippy::too_many_arguments)]

pub(crate) mod declarations;
pub(crate) mod doc_comment_parser;
pub(crate) mod expressions;
pub(crate) mod interpolated_string;
pub(crate) mod patterns;
pub(crate) mod statements;
pub(crate) mod types;

use std::ops::{Deref, DerefMut};

pub(crate) use crate::error_code::{ErrorCode, MessageID, XmlParseErrorCode};
pub(crate) use crate::facts;
pub(crate) use crate::green::{DiagArg, DiagnosticId, Green, SyntaxListBuilder, SyntaxListPool};
pub(crate) use crate::lexer::{InterpolatedStringKind, Interpolation, LexerMode};
pub(crate) use crate::syntax_kind::SyntaxKind;
pub(crate) use crate::syntax_parser::SyntaxParser;
pub(crate) use declarations::*;
pub(crate) use expressions::*;
pub(crate) use types::*;

use psi_builder::stack::StackGuard;

use crate::lexer::Lexer;
use crate::syntax_parser::{ParserKind, SyntaxParserBuffers};
use crate::tree::Entry;

/// How deep `_recursionDepth` may go before the parse is abandoned as Roslyn abandons it when
/// `EnsureSufficientExecutionStack` throws. Roslyn's limit is its thread's stack (64 MB in the oracle); the port's is
/// this count, the same on every target, and the stack itself (`psi_builder::stack::STACK_BUDGET`), whichever comes
/// first: a level takes 371 bytes for parentheses on macOS arm64 and up to 880 (a property pattern; 1041 on Windows
/// x64), so the heaviest forms reach the stack budget first, every form past 700 levels (`tests/robustness.rs`). A
/// local function's 1.9 KB counts as 3 levels (`LOCAL_FUNCTION_BODY_DEPTH`). The corpora of tools/csharp-oracle reach
/// 678 (`hugeexpr1.cs` of dotnet/runtime); DESIGN.md, "C# nesting", compares the limit with Roslyn's.
pub(crate) const MAX_RECURSION_DEPTH: i32 = 1000;

/// The levels a local function's body adds to `_recursionDepth` (not in Roslyn): a level of nested local functions
/// takes 1.9 KB of stack (eight frames from `ParseStatementCore` through the declarator to the body's statements), and
/// 1000 of them overflowed a 1 MB stack.
pub(crate) const LOCAL_FUNCTION_BODY_DEPTH: i32 = 2;

/// `LanguageParser`: the C# parser over a `SyntaxParser` (`Deref` reaches its methods: `self.eat_token()`,
/// `self.current_kind()`, `self.arena()`, ...).
pub struct LanguageParser<'l, 't> {
    pub(crate) base: SyntaxParser<'l, 't>,
    /// `_pool`.
    pub(crate) pool: SyntaxListPool,
    /// `_syntaxFactoryContext.IsInAsync`.
    pub(crate) is_in_async: bool,
    /// `_syntaxFactoryContext.ForceConditionalAccessExpression`.
    pub(crate) force_conditional_access_expression: bool,
    /// `_syntaxFactoryContext.IsInQuery`.
    pub(crate) is_in_query: bool,
    /// `_syntaxFactoryContext.IsInFieldKeywordContext`.
    pub(crate) is_in_field_keyword_context: bool,
    /// `_recursionDepth`.
    pub(crate) recursion_depth: i32,
    /// The deepest `_recursionDepth` a stack check saw, its holes' parsers' included (`ParsedStrings`), not in Roslyn.
    pub(crate) peak_recursion_depth: i32,
    /// Where the parse of the file started on the stack, shared by its holes' parsers, and the lowest frame a stack
    /// check was made from (`ensure_sufficient_execution_stack`, `ParsedStrings`), not in Roslyn.
    pub(crate) stack: StackGuard,
    pub(crate) lowest_frame: usize,
    /// `_termState`.
    pub(crate) term_state: TerminatorState,
    /// The lexer's position when the recursion limit or the stack budget was hit (the
    /// `InsufficientExecutionStackException`), if it was.
    pub(crate) stack_exhausted_at: Option<u32>,
    /// The finished tuple type scans of this parse (`scan_tuple_type`), not in Roslyn.
    pub(crate) scanned_tuples: ScannedTuples,
    /// The interpolated and raw string tokens this parser made into expressions
    /// (`parse_interpolated_or_raw_string_token`), not in Roslyn.
    pub(crate) parsed_strings: interpolated_string::ParsedStrings,
    /// The `primary ?` pairs of a conditional access chain still to be built
    /// (`try_parse_conditional_access_expression`), not in Roslyn.
    pub(crate) pending_conditional_accesses: Vec<(Green, Green)>,
}

impl<'l, 't> Deref for LanguageParser<'l, 't> {
    type Target = SyntaxParser<'l, 't>;

    #[inline]
    fn deref(&self) -> &SyntaxParser<'l, 't> {
        &self.base
    }
}

impl<'l, 't> DerefMut for LanguageParser<'l, 't> {
    #[inline]
    fn deref_mut(&mut self) -> &mut SyntaxParser<'l, 't> {
        &mut self.base
    }
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `LanguageParser(lexer, oldTree: null, changes: null, lexerMode)`: `allowModeReset: false`, prelexing.
    pub fn new(
        lexer: &'l mut Lexer<'t>,
        lexer_mode: LexerMode,
        buffers: SyntaxParserBuffers,
        pool: SyntaxListPool,
        stack: StackGuard,
    ) -> LanguageParser<'l, 't> {
        LanguageParser {
            base: SyntaxParser::new(lexer, lexer_mode, ParserKind::Language, false, true, buffers),
            pool,
            is_in_async: false,
            force_conditional_access_expression: false,
            is_in_query: false,
            is_in_field_keyword_context: false,
            recursion_depth: 0,
            peak_recursion_depth: 0,
            stack,
            lowest_frame: usize::MAX,
            term_state: TerminatorState::END_OF_FILE,
            stack_exhausted_at: None,
            scanned_tuples: ScannedTuples::default(),
            parsed_strings: interpolated_string::ParsedStrings::default(),
            pending_conditional_accesses: Vec::new(),
        }
    }

    /// `Dispose()`: the buffers back for reuse.
    pub fn into_buffers(self) -> (SyntaxParserBuffers, SyntaxListPool) {
        (self.base.into_buffers(), self.pool)
    }

    /// What `CSharpSyntaxTree.ParseText` or `SyntaxFactory.Parse*` (with `consumeFullText: true`) parses for `entry`;
    /// null when the entry finds nothing (`ParseMemberDeclaration`).
    pub fn parse_entry(&mut self, entry: Entry) -> Green {
        let node = match entry {
            Entry::CompilationUnit => return self.parse_compilation_unit(),
            // `Parser::parse` lexes these (`SyntaxFactory.ParseTokens`): no parse
            Entry::Tokens => return Green::NULL,
            Entry::Expression => self.parse_expression(),
            Entry::Statement => self.parse_statement(),
            Entry::Member => self.parse_member_declaration(),
            Entry::TypeName => self.parse_type_name(),
            Entry::Name => self.parse_name(),
            Entry::ParameterList => self.parse_parenthesized_parameter_list(false),
            Entry::BracketedParameterList => self.parse_bracketed_parameter_list(),
            Entry::ArgumentList => self.parse_parenthesized_argument_list(),
            Entry::BracketedArgumentList => self.parse_bracketed_argument_list(),
            Entry::AttributeArgumentList => {
                let node = self.parse_attribute_argument_list();
                if node.is_null() {
                    // `?? new AttributeArgumentListSyntax(MissingToken(OpenParenToken), null, MissingToken(CloseParenToken))`
                    let a = self.arena();
                    let open = a.missing_token(SyntaxKind::OpenParenToken);
                    let close = a.missing_token(SyntaxKind::CloseParenToken);
                    a.attribute_argument_list(open, Green::NULL, close)
                } else {
                    node
                }
            }
        };
        if node.is_null() {
            return node;
        }
        self.consume_unexpected_tokens(node)
    }

    /// `StackGuard.EnsureSufficientExecutionStack(_recursionDepth)` after `_recursionDepth++`: past the limit, or past
    /// the stack a parse may take (`psi_builder::stack::STACK_BUDGET` from where the file's parse started, Roslyn's
    /// own measure), the parse is abandoned (see the module's documentation).
    pub(crate) fn ensure_sufficient_execution_stack(&mut self) {
        let frame = psi_builder::stack::here();
        self.lowest_frame = self.lowest_frame.min(frame);
        self.peak_recursion_depth = self.peak_recursion_depth.max(self.recursion_depth);
        if (self.recursion_depth > MAX_RECURSION_DEPTH || self.stack.is_past(frame)) && self.stack_exhausted_at.is_none()
        {
            self.stack_exhausted_at = Some(self.lexer.position());
            self.base.abandon();
        }
    }

    /// `private static bool IsSomeWord(SyntaxKind kind)` (LanguageParser.cs:49).
    pub(crate) fn is_some_word(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::IdentifierToken || facts::is_keyword_kind(kind)
    }
}

/// `TerminatorState` (LanguageParser.cs:58).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub(crate) struct TerminatorState(pub(crate) u32);

impl TerminatorState {
    pub(crate) const END_OF_FILE: TerminatorState = TerminatorState(0);
    pub(crate) const IS_NAMESPACE_MEMBER_START_OR_STOP: TerminatorState = TerminatorState(1 << 0);
    pub(crate) const IS_ATTRIBUTE_DECLARATION_TERMINATOR: TerminatorState = TerminatorState(1 << 1);
    pub(crate) const IS_POSSIBLE_AGGREGATE_CLAUSE_START_OR_STOP: TerminatorState = TerminatorState(1 << 2);
    pub(crate) const IS_POSSIBLE_MEMBER_START_OR_STOP: TerminatorState = TerminatorState(1 << 3);
    pub(crate) const IS_END_OF_RETURN_TYPE: TerminatorState = TerminatorState(1 << 4);
    pub(crate) const IS_END_OF_PARAMETER_LIST: TerminatorState = TerminatorState(1 << 5);
    pub(crate) const IS_END_OF_FIELD_DECLARATION: TerminatorState = TerminatorState(1 << 6);
    pub(crate) const IS_POSSIBLE_END_OF_VARIABLE_DECLARATION: TerminatorState = TerminatorState(1 << 7);
    pub(crate) const IS_END_OF_TYPE_ARGUMENT_LIST: TerminatorState = TerminatorState(1 << 8);
    pub(crate) const IS_POSSIBLE_STATEMENT_START_OR_STOP: TerminatorState = TerminatorState(1 << 9);
    pub(crate) const IS_END_OF_FIXED_STATEMENT: TerminatorState = TerminatorState(1 << 10);
    pub(crate) const IS_END_OF_TRY_BLOCK: TerminatorState = TerminatorState(1 << 11);
    pub(crate) const IS_END_OF_CATCH_CLAUSE: TerminatorState = TerminatorState(1 << 12);
    pub(crate) const IS_END_OF_FILTER_CLAUSE: TerminatorState = TerminatorState(1 << 13);
    pub(crate) const IS_END_OF_CATCH_BLOCK: TerminatorState = TerminatorState(1 << 14);
    pub(crate) const IS_END_OF_DO_WHILE_EXPRESSION: TerminatorState = TerminatorState(1 << 15);
    pub(crate) const IS_END_OF_FOR_STATEMENT_ARGUMENT: TerminatorState = TerminatorState(1 << 16);
    pub(crate) const IS_END_OF_DECLARATION_CLAUSE: TerminatorState = TerminatorState(1 << 17);
    pub(crate) const IS_END_OF_ARGUMENT_LIST: TerminatorState = TerminatorState(1 << 18);
    pub(crate) const IS_SWITCH_SECTION_START: TerminatorState = TerminatorState(1 << 19);
    pub(crate) const IS_END_OF_TYPE_PARAMETER_LIST: TerminatorState = TerminatorState(1 << 20);
    pub(crate) const IS_END_OF_METHOD_SIGNATURE: TerminatorState = TerminatorState(1 << 21);
    pub(crate) const IS_END_OF_NAME_IN_EXPLICIT_INTERFACE: TerminatorState = TerminatorState(1 << 22);
    pub(crate) const IS_END_OF_FUNCTION_POINTER_PARAMETER_LIST: TerminatorState = TerminatorState(1 << 23);
    pub(crate) const IS_END_OF_FUNCTION_POINTER_PARAMETER_LIST_ERRORED: TerminatorState = TerminatorState(1 << 24);
    pub(crate) const IS_END_OF_FUNCTION_POINTER_CALLING_CONVENTION: TerminatorState = TerminatorState(1 << 25);
    pub(crate) const IS_END_OF_TYPE_SIGNATURE: TerminatorState = TerminatorState(1 << 26);
    pub(crate) const IS_EXPRESSION_OR_PATTERN_IN_CASE_LABEL_OF_SWITCH_STATEMENT: TerminatorState =
        TerminatorState(1 << 27);
    pub(crate) const IS_PATTERN_IN_SWITCH_EXPRESSION_ARM: TerminatorState = TerminatorState(1 << 28);
}

impl TerminatorState {
    /// `(value & flag) != 0`.
    #[inline]
    pub(crate) fn contains(self, flag: TerminatorState) -> bool {
        self.0 & flag.0 != 0
    }
}

impl std::ops::BitOr for TerminatorState {
    type Output = TerminatorState;

    #[inline]
    fn bitor(self, other: TerminatorState) -> TerminatorState {
        TerminatorState(self.0 | other.0)
    }
}

impl std::ops::BitAnd for TerminatorState {
    type Output = TerminatorState;

    #[inline]
    fn bitand(self, other: TerminatorState) -> TerminatorState {
        TerminatorState(self.0 & other.0)
    }
}

/// `LastTerminatorState` (LanguageParser.cs:92).
const LAST_TERMINATOR_STATE: u32 = TerminatorState::IS_PATTERN_IN_SWITCH_EXPRESSION_ARM.0;

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private bool IsTerminator()` (LanguageParser.cs:94).
    pub(crate) fn is_terminator(&mut self) -> bool {
        if self.current_kind() == SyntaxKind::EndOfFileToken {
            return true;
        }
        let mut i = 1u32;
        while i <= LAST_TERMINATOR_STATE {
            let state = TerminatorState(self.term_state.0 & i);
            let terminates = match state {
                TerminatorState::IS_NAMESPACE_MEMBER_START_OR_STOP => self.is_namespace_member_start_or_stop(),
                TerminatorState::IS_ATTRIBUTE_DECLARATION_TERMINATOR => self.is_attribute_declaration_terminator(),
                TerminatorState::IS_POSSIBLE_AGGREGATE_CLAUSE_START_OR_STOP => {
                    self.is_possible_aggregate_clause_start_or_stop()
                }
                TerminatorState::IS_POSSIBLE_MEMBER_START_OR_STOP => self.is_possible_member_start_or_stop(),
                TerminatorState::IS_END_OF_RETURN_TYPE => self.is_end_of_return_type(),
                TerminatorState::IS_END_OF_PARAMETER_LIST => self.is_end_of_parameter_list(),
                TerminatorState::IS_END_OF_FIELD_DECLARATION => self.is_end_of_field_declaration(),
                TerminatorState::IS_POSSIBLE_END_OF_VARIABLE_DECLARATION => {
                    self.is_possible_end_of_variable_declaration()
                }
                TerminatorState::IS_END_OF_TYPE_ARGUMENT_LIST => self.is_end_of_type_argument_list(),
                TerminatorState::IS_POSSIBLE_STATEMENT_START_OR_STOP => self.is_possible_statement_start_or_stop(),
                TerminatorState::IS_END_OF_FIXED_STATEMENT => self.is_end_of_fixed_statement(),
                TerminatorState::IS_END_OF_TRY_BLOCK => self.is_end_of_try_block(),
                TerminatorState::IS_END_OF_CATCH_CLAUSE => self.is_end_of_catch_clause(),
                TerminatorState::IS_END_OF_FILTER_CLAUSE => self.is_end_of_filter_clause(),
                TerminatorState::IS_END_OF_CATCH_BLOCK => self.is_end_of_catch_block(),
                TerminatorState::IS_END_OF_DO_WHILE_EXPRESSION => self.is_end_of_do_while_expression(),
                TerminatorState::IS_END_OF_FOR_STATEMENT_ARGUMENT => self.is_end_of_for_statement_argument(),
                TerminatorState::IS_END_OF_DECLARATION_CLAUSE => self.is_end_of_declaration_clause(),
                TerminatorState::IS_END_OF_ARGUMENT_LIST => self.is_end_of_argument_list(),
                TerminatorState::IS_SWITCH_SECTION_START => self.is_possible_switch_section(),
                TerminatorState::IS_END_OF_TYPE_PARAMETER_LIST => self.is_end_of_type_parameter_list(),
                TerminatorState::IS_END_OF_METHOD_SIGNATURE => self.is_end_of_method_signature(),
                TerminatorState::IS_END_OF_NAME_IN_EXPLICIT_INTERFACE => self.is_end_of_name_in_explicit_interface(),
                TerminatorState::IS_END_OF_FUNCTION_POINTER_PARAMETER_LIST => {
                    self.is_end_of_function_pointer_parameter_list(false)
                }
                TerminatorState::IS_END_OF_FUNCTION_POINTER_PARAMETER_LIST_ERRORED => {
                    self.is_end_of_function_pointer_parameter_list(true)
                }
                TerminatorState::IS_END_OF_FUNCTION_POINTER_CALLING_CONVENTION => {
                    self.is_end_of_function_pointer_calling_convention()
                }
                TerminatorState::IS_END_OF_TYPE_SIGNATURE => self.is_end_of_type_signature(),
                _ => false,
            };
            if terminates {
                return true;
            }
            i <<= 1;
        }
        false
    }

    // `GetOldParent(node)` (LanguageParser.cs:139): incremental parsing only, not ported.
}

/// `NamespaceBodyBuilder` (LanguageParser.cs:144): the lists a namespace body is parsed into.
#[derive(Default)]
pub(crate) struct NamespaceBodyBuilder {
    pub(crate) externs: SyntaxListBuilder,
    pub(crate) usings: SyntaxListBuilder,
    pub(crate) attributes: SyntaxListBuilder,
    pub(crate) members: SyntaxListBuilder,
}

impl NamespaceBodyBuilder {
    /// `new NamespaceBodyBuilder(pool)`.
    pub(crate) fn new(pool: &mut SyntaxListPool) -> NamespaceBodyBuilder {
        NamespaceBodyBuilder {
            externs: pool.allocate(),
            usings: pool.allocate(),
            attributes: pool.allocate(),
            members: pool.allocate(),
        }
    }

    /// `Free(pool)`.
    pub(crate) fn free(self, pool: &mut SyntaxListPool) {
        pool.free(self.members);
        pool.free(self.attributes);
        pool.free(self.usings);
        pool.free(self.externs);
    }
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `internal CompilationUnitSyntax ParseCompilationUnit()` (LanguageParser.cs:168).
    pub(crate) fn parse_compilation_unit(&mut self) -> Green {
        self.parse_with_stack_guard(
            |this| this.parse_compilation_unit_core(),
            |this| {
                let eof = this.arena().token(SyntaxKind::EndOfFileToken);
                this.arena().compilation_unit(Green::NULL, Green::NULL, Green::NULL, Green::NULL, eof)
            },
        )
    }

    /// `internal CompilationUnitSyntax ParseCompilationUnitCore()` (LanguageParser.cs:180).
    pub(crate) fn parse_compilation_unit_core(&mut self) -> Green {
        let mut tmp = Green::NULL;
        let mut initial_bad_nodes: Option<SyntaxListBuilder> = None;
        let mut body = NamespaceBodyBuilder::new(&mut self.pool);
        self.parse_namespace_body(&mut tmp, &mut body, &mut initial_bad_nodes, SyntaxKind::CompilationUnit);
        let eof = self.eat_token_kind(SyntaxKind::EndOfFileToken);
        let a = self.base.arena();
        let externs = a.list(&body.externs);
        let usings = a.list(&body.usings);
        let attributes = a.list(&body.attributes);
        let members = a.list(&body.members);
        let mut result = a.compilation_unit(externs, usings, attributes, members, eof);
        if let Some(bad) = initial_bad_nodes {
            // attach initial bad nodes as leading trivia on first token
            let skipped = self.base.arena().list(&bad);
            result = self.add_leading_skipped_syntax(result, skipped);
            self.pool.free(bad);
        }
        body.free(&mut self.pool);
        result
    }

    /// `internal TNode ParseWithStackGuard<TNode>(Func<LanguageParser, TNode> parseFunc, Func<LanguageParser, TNode>
    /// createEmptyNodeFunc)` (LanguageParser.cs:207): the parse, or when it ran out of depth the empty node with the
    /// whole text skipped (`CreateForGlobalFailure`).
    pub(crate) fn parse_with_stack_guard(
        &mut self,
        parse_func: fn(&mut LanguageParser<'_, '_>) -> Green,
        create_empty_node_func: fn(&mut LanguageParser<'_, '_>) -> Green,
    ) -> Green {
        // If this value is non-zero then we are nesting calls to ParseWithStackGuard which should not be
        // happening.  It's not a bug but it's inefficient and should be changed.
        debug_assert_eq!(self.recursion_depth, 0);
        let result = parse_func(self);
        // An arena past its budget (`green::budget_for`) fails as an exhausted stack does.
        match self.stack_exhausted_at.or(self.base.budget_exhausted_at) {
            None => result,
            Some(position) => {
                let empty = create_empty_node_func(self);
                self.create_for_global_failure(position as i32, empty)
            }
        }
    }

    /// `private TNode CreateForGlobalFailure<TNode>(int position, TNode node)` (LanguageParser.cs:223).
    pub(crate) fn create_for_global_failure(&mut self, position: i32, node: Green) -> Green {
        // Turn the complete input into a single skipped token. This avoids running the lexer, and therefore
        // the preprocessor directive parser, which may itself run into the same problem that caused the
        // original failure.
        let start = self.base.lexer.window_start();
        let width = self.base.lexer.end() - start;
        let a = self.base.arena();
        let bad = a.create_token(
            crate::green::TriviaList::EMPTY,
            SyntaxKind::BadToken,
            SyntaxKind::BadToken,
            crate::green::TokenText::Source { start, width },
            crate::green::TriviaList::EMPTY,
        );
        let file_as_trivia = a.skipped_tokens_trivia(bad);
        let node = self.add_leading_skipped_syntax(node, file_as_trivia);
        self.force_end_of_file(); // force the scanner to report that it is at the end of the input.
        self.add_error_at(node, position, 0, ErrorCode::ERR_InsufficientStack, &[])
    }

    // `IsIncremental`, `IsIncrementalAndFactoryContextMatches`, `MatchesFactoryContext` (LanguageParser.cs:14425-14450):
    // incremental parsing only, not ported. `IsInAsync`, `ForceConditionalAccessExpression`, `IsInQuery`,
    // `IsInFieldKeywordContext` (LanguageParser.cs:14452-14474) are the fields of the same names.

    /// `private SeparatedSyntaxList<TNode> ParseCommaSeparatedSyntaxList<TNode>( ref SyntaxToken openToken, SyntaxKind closeTokenKind, Func<LanguageParser, bool> isPossibleElement, Func<LanguageParser, TNode> parseElement, SkipBadTokens<TNode> skipBadTokens, bool allowTrailingSeparator, bool requireOneElement, bool allowSemicolonAsSeparator) where TNode : GreenNode` (LanguageParser.cs:14508).
    pub(crate) fn parse_comma_separated_syntax_list(
        &mut self,
        open_token: &mut Green,
        close_token_kind: SyntaxKind,
        is_possible_element: fn(&mut LanguageParser<'_, '_>) -> bool,
        parse_element: fn(&mut LanguageParser<'_, '_>) -> Green,
        skip_bad_tokens: SkipBadTokens,
        allow_trailing_separator: bool,
        require_one_element: bool,
        allow_semicolon_as_separator: bool,
    ) -> Green {
        self.parse_comma_separated_syntax_list_with_allow_semicolon_as_separator(
            open_token,
            close_token_kind,
            is_possible_element,
            parse_element,
            None,
            skip_bad_tokens,
            allow_trailing_separator,
            require_one_element,
            allow_semicolon_as_separator,
        )
    }

    /// `private SeparatedSyntaxList<TNode> ParseCommaSeparatedSyntaxList<TNode>( ref SyntaxToken openToken, SyntaxKind closeTokenKind, Func<LanguageParser, bool> isPossibleElement, Func<LanguageParser, TNode> parseElement, Func<TNode, bool>? immediatelyAbort, SkipBadTokens<TNode> skipBadTokens, bool allowTrailingSeparator, bool requireOneElement, bool allowSemicolonAsSeparator) where TNode : GreenNode` (LanguageParser.cs:14530).
    /// `immediatelyAbort` also takes the parser, to read the arena.
    pub(crate) fn parse_comma_separated_syntax_list_with_allow_semicolon_as_separator(
        &mut self,
        open_token: &mut Green,
        close_token_kind: SyntaxKind,
        is_possible_element: fn(&mut LanguageParser<'_, '_>) -> bool,
        parse_element: fn(&mut LanguageParser<'_, '_>) -> Green,
        immediately_abort: Option<fn(&mut LanguageParser<'_, '_>, Green) -> bool>,
        skip_bad_tokens: SkipBadTokens,
        allow_trailing_separator: bool,
        require_one_element: bool,
        allow_semicolon_as_separator: bool,
    ) -> Green {
        // If we ever want this function to parse out separated lists with a different separator, we can
        // parameterize this method on this value.
        let separator_token_kind = SyntaxKind::CommaToken;
        let mut nodes = self.pool.allocate();
        let mut require_one_element = require_one_element;

        // shouldParseSeparatorOrElement
        let should_parse_separator_or_element = |this: &mut LanguageParser<'_, '_>| -> bool {
            // if we're on a separator, we def should parse it out as such.
            if this.current_kind() == separator_token_kind {
                return true;
            }
            // We're not on a valid separator, but we want to be resilient for the user accidentally using the wrong
            // one in common cases.
            if allow_semicolon_as_separator && this.current_kind() == SyntaxKind::SemicolonToken {
                return true;
            }
            if is_possible_element(this) {
                return true;
            }
            false
        };

        'try_again: loop {
            if require_one_element || self.current_kind() != close_token_kind {
                if require_one_element || should_parse_separator_or_element(self) {
                    // first argument
                    let mut node = parse_element(self);
                    nodes.push(node);

                    // now that we've gotten one element, we don't require any more.
                    require_one_element = false;

                    // Ensure that if parsing separators/elements doesn't move us forward, that we always bail out from
                    // parsing this list.
                    let mut last_token_position = -1;

                    while immediately_abort.map(|f| f(self, node)) != Some(true)
                        && self.is_making_progress(&mut last_token_position, true)
                    {
                        if self.current_kind() == close_token_kind {
                            break;
                        }

                        if should_parse_separator_or_element(self) {
                            // If we got a semicolon instead of comma, consume it with error and act as if it were a
                            // comma. Note: we do not change the kind of the token, so we can end up with a separated
                            // syntax list whose separators are a mix of commas and semicolons.  That is ok and is part
                            // of the expected contract of separated lists.  There will still be a diagnostic on the
                            // token letting the user know there is an error.  This allows us to recover gracefully,
                            // especially for higher levels like the IDE.
                            let separator = if self.current_kind() == SyntaxKind::SemicolonToken {
                                self.eat_token_even_with_incorrect_kind(separator_token_kind)
                            } else {
                                self.eat_token_kind(separator_token_kind)
                            };
                            nodes.push(separator);

                            if allow_trailing_separator {
                                // check for exit case after legal trailing comma
                                if self.current_kind() == close_token_kind {
                                    break;
                                } else if !is_possible_element(self) {
                                    continue 'try_again;
                                }
                            }

                            node = parse_element(self);
                            nodes.push(node);
                            continue;
                        }

                        // Something we didn't recognize, try to skip tokens, reporting that we expected a separator here.
                        if skip_bad_tokens(self, open_token, &mut nodes, separator_token_kind, close_token_kind)
                            == PostSkipAction::Abort
                        {
                            break;
                        }
                    }
                } else if skip_bad_tokens(self, open_token, &mut nodes, SyntaxKind::IdentifierToken, close_token_kind)
                    == PostSkipAction::Continue
                {
                    // Something we didn't recognize, try to skip tokens, reporting that we expected an identifier here.
                    // While 'identifier' may not be completely accurate in terms of what the list needs, it's a
                    // generally good 'catch all' indicating that some name/expr was needed, where something else
                    // invalid was found.
                    continue 'try_again;
                }
            }
            break;
        }

        let list = self.base.arena().list(&nodes);
        self.pool.free(nodes);
        list
    }

    /// `private DisposableResetPoint GetDisposableResetPoint(bool resetOnDispose)` (LanguageParser.cs:14634).
    pub(crate) fn get_disposable_reset_point(&mut self, reset_on_dispose: bool) -> DisposableResetPoint {
        DisposableResetPoint { reset_on_dispose, reset_point: self.get_reset_point() }
    }

    /// `private new ResetPoint GetResetPoint()` (LanguageParser.cs:14637).
    pub(crate) fn get_reset_point(&mut self) -> ResetPoint {
        ResetPoint {
            base_reset_point: self.base.get_reset_point(),
            terminator_state: self.term_state,
            is_in_async: self.is_in_async,
            is_in_query: self.is_in_query,
            is_in_field_keyword_context: self.is_in_field_keyword_context,
        }
    }

    /// `private void Reset(ref ResetPoint state)` (LanguageParser.cs:14647).
    pub(crate) fn reset(&mut self, state: &mut ResetPoint) {
        self.term_state = state.terminator_state;
        self.is_in_async = state.is_in_async;
        self.is_in_query = state.is_in_query;
        self.is_in_field_keyword_context = state.is_in_field_keyword_context;
        self.base.reset(&state.base_reset_point);
    }

    /// `private void Release(ref ResetPoint state)` (LanguageParser.cs:14656).
    pub(crate) fn release(&mut self, state: &mut ResetPoint) {
        self.base.release(&state.base_reset_point);
    }

    /// `DisposableResetPoint.Reset()` (LanguageParser.cs:14675).
    pub(crate) fn reset_disposable(&mut self, point: &mut DisposableResetPoint) {
        self.reset(&mut point.reset_point);
    }

    /// `DisposableResetPoint.Dispose()` (LanguageParser.cs:14678): resets when made so, then releases.
    pub(crate) fn dispose_reset_point(&mut self, mut point: DisposableResetPoint) {
        if point.reset_on_dispose {
            self.reset(&mut point.reset_point);
        }
        self.release(&mut point.reset_point);
    }

    /// `internal TNode ConsumeUnexpectedTokens<TNode>(TNode node)` (LanguageParser.cs:14709).
    pub(crate) fn consume_unexpected_tokens(&mut self, node: Green) -> Green {
        if self.current_kind() == SyntaxKind::EndOfFileToken {
            return node;
        }
        let mut b = self.pool.allocate();
        while self.current_kind() != SyntaxKind::EndOfFileToken {
            let token = self.eat_token();
            b.push(token);
        }
        let first = b[0];
        let trailing_trash = self.base.arena().list(&b);
        self.pool.free(b);
        let node = self.add_error_args(node, ErrorCode::ERR_UnexpectedToken, &[DiagArg::TokenText(first)]);
        self.add_trailing_skipped_syntax(node, trailing_trash)
    }

    /// `private static bool ContainsErrorDiagnostic(GreenNode node)` (LanguageParser.cs:14726): whether the node or
    /// anything in it (not its trivia) has a diagnostic of error severity.
    pub(crate) fn contains_error_diagnostic(&mut self, node: Green) -> bool {
        let a = self.base.arena_ref();
        // Quick check to avoid any unnecessary work.
        if !a.contains_diagnostics(node) {
            return false;
        }
        let mut stack = vec![node];
        let mut children = Vec::new();
        while let Some(current) = stack.pop() {
            if !a.contains_diagnostics(current) {
                continue;
            }
            if a.diagnostics(current).any(|d| a.diagnostic(d).code.severity() == crate::Severity::Error) {
                return true;
            }
            a.child_nodes_and_tokens(current, &mut children);
            stack.extend_from_slice(&children);
        }
        false
    }
}

/// `private delegate PostSkipAction SkipBadTokens<TNode>(LanguageParser parser, ref SyntaxToken openToken,
/// SeparatedSyntaxListBuilder<TNode> builder, SyntaxKind expectedKind, SyntaxKind closeTokenKind)`
/// (LanguageParser.cs:14476).
pub(crate) type SkipBadTokens =
    fn(&mut LanguageParser<'_, '_>, &mut Green, &mut SyntaxListBuilder, SyntaxKind, SyntaxKind) -> PostSkipAction;

/// `DisposableResetPoint` (LanguageParser.cs:14661): a reset point released, after a reset when `reset_on_dispose`,
/// by `dispose_reset_point` (Rust has no `using`: a lane calls it where the C# `using` scope ends).
pub(crate) struct DisposableResetPoint {
    pub(crate) reset_on_dispose: bool,
    pub(crate) reset_point: ResetPoint,
}

/// `LanguageParser.ResetPoint` (LanguageParser.cs:14686): the base reset point and the parser's own state.
#[derive(Copy, Clone, Debug)]
pub(crate) struct ResetPoint {
    pub(crate) base_reset_point: crate::syntax_parser::ResetPoint,
    pub(crate) terminator_state: TerminatorState,
    pub(crate) is_in_async: bool,
    pub(crate) is_in_query: bool,
    pub(crate) is_in_field_keyword_context: bool,
}

/// `DeclarationModifiers` (`Declarations/DeclarationModifiers.cs`), what `GetModifierExcludingScoped` returns.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub(crate) struct DeclarationModifiers(pub(crate) u32);

// Every value of Roslyn's enum, whether the parser reads it or not.
#[allow(dead_code)]
impl DeclarationModifiers {
    pub(crate) const NONE: DeclarationModifiers = DeclarationModifiers(0);
    pub(crate) const ABSTRACT: DeclarationModifiers = DeclarationModifiers(1 << 0);
    pub(crate) const SEALED: DeclarationModifiers = DeclarationModifiers(1 << 1);
    pub(crate) const STATIC: DeclarationModifiers = DeclarationModifiers(1 << 2);
    pub(crate) const NEW: DeclarationModifiers = DeclarationModifiers(1 << 3);
    pub(crate) const PUBLIC: DeclarationModifiers = DeclarationModifiers(1 << 4);
    pub(crate) const PROTECTED: DeclarationModifiers = DeclarationModifiers(1 << 5);
    pub(crate) const INTERNAL: DeclarationModifiers = DeclarationModifiers(1 << 6);
    pub(crate) const PROTECTED_INTERNAL: DeclarationModifiers = DeclarationModifiers(1 << 7);
    pub(crate) const PRIVATE: DeclarationModifiers = DeclarationModifiers(1 << 8);
    pub(crate) const PRIVATE_PROTECTED: DeclarationModifiers = DeclarationModifiers(1 << 9);
    pub(crate) const READ_ONLY: DeclarationModifiers = DeclarationModifiers(1 << 10);
    pub(crate) const CONST: DeclarationModifiers = DeclarationModifiers(1 << 11);
    pub(crate) const VOLATILE: DeclarationModifiers = DeclarationModifiers(1 << 12);
    pub(crate) const EXTERN: DeclarationModifiers = DeclarationModifiers(1 << 13);
    pub(crate) const PARTIAL: DeclarationModifiers = DeclarationModifiers(1 << 14);
    pub(crate) const UNSAFE: DeclarationModifiers = DeclarationModifiers(1 << 15);
    pub(crate) const FIXED: DeclarationModifiers = DeclarationModifiers(1 << 16);
    pub(crate) const VIRTUAL: DeclarationModifiers = DeclarationModifiers(1 << 17);
    pub(crate) const OVERRIDE: DeclarationModifiers = DeclarationModifiers(1 << 18);
    pub(crate) const INDEXER: DeclarationModifiers = DeclarationModifiers(1 << 19);
    pub(crate) const ASYNC: DeclarationModifiers = DeclarationModifiers(1 << 20);
    pub(crate) const REF: DeclarationModifiers = DeclarationModifiers(1 << 21);
    pub(crate) const REQUIRED: DeclarationModifiers = DeclarationModifiers(1 << 22);
    pub(crate) const SCOPED: DeclarationModifiers = DeclarationModifiers(1 << 23);
    pub(crate) const FILE: DeclarationModifiers = DeclarationModifiers(1 << 24);
    pub(crate) const CLOSED: DeclarationModifiers = DeclarationModifiers(1 << 25);
    pub(crate) const SAFE: DeclarationModifiers = DeclarationModifiers(1 << 26);
    pub(crate) const ALL: DeclarationModifiers = DeclarationModifiers((1 << 27) - 1);
    pub(crate) const UNSET: DeclarationModifiers = DeclarationModifiers(1 << 27);
    pub(crate) const ACCESSIBILITY_MASK: DeclarationModifiers = DeclarationModifiers(
        Self::PRIVATE_PROTECTED.0
            | Self::PRIVATE.0
            | Self::PROTECTED.0
            | Self::INTERNAL.0
            | Self::PROTECTED_INTERNAL.0
            | Self::PUBLIC.0,
    );
}
