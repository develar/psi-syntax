//! `LanguageParser.cs` 8248-11122: statements, blocks, local declarations and local functions (lane B). Every method in C# order; see `parser/mod.rs` for the conventions.
//!
//! A C# local function (`parseSwitchHeader`, `eatVariableDeclarationOrInitializers`, ...) is a nested `fn` or inline
//! code of its method. `ParserSyntaxContextResetter` is a save and restore of the four context flags around the code
//! its `using` covers.
//!
//! Not in Roslyn: `ParseDesignation` recursing into nested parentheses counts `_recursionDepth`, as
//! `ParseStatementCore` does, so that a deep `var ((((...` stays within the stack.

use super::*;
use crate::green::{GreenArena, TokenText};
use crate::syntax::types as nt;

impl<'l, 't> LanguageParser<'l, 't> {
    /// `public StatementSyntax ParseStatement()` (LanguageParser.cs:8248).
    pub(crate) fn parse_statement(&mut self) -> Green {
        self.parse_with_stack_guard(
            |this| {
                let statement = this.parse_possibly_attributed_statement();
                if statement.is_null() { this.parse_expression_statement(Green::NULL) } else { statement }
            },
            |this| {
                let semicolon = this.arena().missing_token(SyntaxKind::SemicolonToken);
                this.arena().empty_statement(Green::NULL, semicolon)
            },
        )
    }

    /// `private StatementSyntax ParsePossiblyAttributedStatement()` (LanguageParser.cs:8255).
    pub(crate) fn parse_possibly_attributed_statement(&mut self) -> Green {
        let attributes = self.parse_statement_attribute_declarations();
        self.parse_statement_core(attributes, false)
    }

    /// `private SyntaxList<AttributeListSyntax> ParseStatementAttributeDeclarations()` (LanguageParser.cs:8258).
    pub(crate) fn parse_statement_attribute_declarations(&mut self) -> Green {
        if self.current_kind() != SyntaxKind::OpenBracketToken {
            return Green::NULL;
        }

        // See if we should treat this as a collection expression.  At the top-level or statement-level, this should
        // only be considered a collection if followed by a `.`, `?` or `!` (indicating it's a value, not an
        // attribute).
        let mut reset_point = self.get_reset_point();

        // Grab the first part as a collection expression.
        self.parse_collection_expression();

        // Continue consuming element access expressions for `[x][y]...`.  We have to determine if this is a
        // collection expression being indexed into, or if it's a sequence of attributes.
        let mut had_bracket_argument_list = false;
        while self.current_kind() == SyntaxKind::OpenBracketToken {
            self.parse_bracketed_argument_list();
            had_bracket_argument_list = true;
        }

        // Check the next token to see if it indicates the `[...]` sequence we have is a term or not. This is the
        // same set of tokens that ParsePostFixExpression looks for.
        //
        // Note `SyntaxKind.DotToken` handles both the `[...].Name` case as well as the `[...]..Range` case.
        let mut is_collection_expression = matches!(
            self.current_kind(),
            SyntaxKind::DotToken
                | SyntaxKind::QuestionToken
                | SyntaxKind::ExclamationToken
                | SyntaxKind::PlusPlusToken
                | SyntaxKind::MinusMinusToken
                | SyntaxKind::MinusGreaterThanToken
        );

        // Now look for another set of items that indicate that we're not an attribute, but instead are a collection
        // expression misplaced in an invalid top level expression-statement. (like `[] + b`).  These are
        // technically invalid. But checking for this allows us to parse effectively to then give a good semantic
        // error later on. These cases came from: ParseExpressionContinued
        is_collection_expression = is_collection_expression
            || Self::is_expected_binary_operator(self.current_kind())
            || Self::is_expected_assignment_operator(self.current_kind())
            || (matches!(self.current_contextual_kind(), SyntaxKind::SwitchKeyword | SyntaxKind::WithKeyword)
                && self.peek_kind(1) == SyntaxKind::OpenBraceToken);

        if !is_collection_expression && had_bracket_argument_list && self.current_kind() == SyntaxKind::OpenParenToken {
            // There are a few things that could be happening here:
            //
            // First is that we have an actual collection expression that we're invoking.  For example:
            //
            //      `[() => {}][rand.NextInt() % x]();`
            //
            // Second would be the start of a local function that returns a tuple.  For example:
            //
            //      `[Attr] (A, B) LocalFunc() { }
            //
            // Have to figure out what the parenthesized thing is in order to parse this.  By parsing out a type
            // and looking for an identifier next, we handle the cases of:
            //
            //      `[Attr] (A, B) LocalFunc() { }
            //      `[Attr] (A, B)[] LocalFunc() { }
            //      `[Attr] (A, B)[,] LocalFunc() { }
            //      `[Attr] (A, B)? LocalFunc() { }
            //      `[Attr] (A, B)* LocalFunc() { }
            //
            // etc.
            //
            // Note: we do not accept the naked `[...](...)` as an invocation of a collection expression.  Collection
            // literals never have a type that itself could possibly be invoked, so this ensures a more natural parse
            // with what users may be expecting here.
            let return_type = self.parse_return_type();
            is_collection_expression = self.contains_error_diagnostic(return_type) || !self.is_true_identifier();
        }

        // If this was a collection expression, not an attribute declaration, return no attributes so that the
        // caller will parse this out as a collection expression. Otherwise re-parse the code as the actual
        // attribute declarations.
        self.reset(&mut reset_point);
        let attributes = if is_collection_expression { Green::NULL } else { self.parse_attribute_declarations(true) };
        self.release(&mut reset_point);

        attributes
    }

    /// `private StatementSyntax ParseStatementCore(SyntaxList<AttributeListSyntax> attributes, bool isGlobal)` (LanguageParser.cs:8346).
    ///
    /// `isGlobal`: if we're being called while parsing a C# top-level statements (Script or Simple Program). At the
    /// top level in Script, we allow most statements *except* for local-decls/local-funcs. Those will instead be
    /// parsed out as script-fields/methods.
    pub(crate) fn parse_statement_core(&mut self, attributes: Green, is_global: bool) -> Green {
        let reused = self.try_reuse_statement(attributes, is_global);
        if !reused.is_null() {
            return reused;
        }

        let mut reset_point_before_statement = self.get_reset_point();
        // try
        self.recursion_depth += 1;
        self.ensure_sufficient_execution_stack();

        let result = 'body: {
            // Main switch to handle processing almost any statement.
            match self.current_kind() {
                SyntaxKind::FixedKeyword => break 'body self.parse_fixed_statement(attributes),
                SyntaxKind::BreakKeyword => break 'body self.parse_break_statement(attributes),
                SyntaxKind::ContinueKeyword => break 'body self.parse_continue_statement(attributes),
                SyntaxKind::TryKeyword | SyntaxKind::CatchKeyword | SyntaxKind::FinallyKeyword => {
                    break 'body self.parse_try_statement(attributes);
                }
                SyntaxKind::CheckedKeyword | SyntaxKind::UncheckedKeyword => {
                    break 'body self.parse_checked_statement(attributes);
                }
                SyntaxKind::DoKeyword => break 'body self.parse_do_statement(attributes),
                SyntaxKind::ForKeyword => break 'body self.parse_for_or_for_each_statement(attributes),
                SyntaxKind::ForEachKeyword => break 'body self.parse_for_each_statement(attributes, Green::NULL),
                SyntaxKind::GotoKeyword => break 'body self.parse_goto_statement(attributes),
                SyntaxKind::IfKeyword => break 'body self.parse_if_statement(attributes),
                // Including 'else' keyword to handle 'else without if' error cases
                SyntaxKind::ElseKeyword => break 'body self.parse_misplaced_else(attributes),
                SyntaxKind::LockKeyword => break 'body self.parse_lock_statement(attributes),
                SyntaxKind::ReturnKeyword => break 'body self.parse_return_statement(attributes),
                // case: error recovery case.
                SyntaxKind::SwitchKeyword | SyntaxKind::CaseKeyword => {
                    break 'body self.parse_switch_statement(attributes);
                }
                SyntaxKind::ThrowKeyword => break 'body self.parse_throw_statement(attributes),
                SyntaxKind::UnsafeKeyword => {
                    let result = self.try_parse_statement_starting_with_unsafe(attributes);
                    if !result.is_null() {
                        break 'body result;
                    }
                }
                SyntaxKind::UsingKeyword => break 'body self.parse_statement_starting_with_using(attributes),
                SyntaxKind::WhileKeyword => break 'body self.parse_while_statement(attributes),
                SyntaxKind::OpenBraceToken => break 'body self.parse_block(attributes),
                SyntaxKind::SemicolonToken => {
                    let semicolon = self.eat_token();
                    break 'body self.arena().empty_statement(attributes, semicolon);
                }
                SyntaxKind::IdentifierToken => {
                    let result = self.try_parse_statement_starting_with_identifier(attributes, is_global);
                    if !result.is_null() {
                        break 'body result;
                    }
                }
                _ => {}
            }

            self.parse_statement_core_rest(attributes, is_global, &mut reset_point_before_statement)
        };

        // finally
        self.recursion_depth -= 1;
        self.release(&mut reset_point_before_statement);
        result
    }

    /// `private StatementSyntax TryReuseStatement(SyntaxList<AttributeListSyntax> attributes, bool isGlobal)` (LanguageParser.cs:8428):
    /// never anything, as `IsIncrementalAndFactoryContextMatches` is false without incremental parsing.
    pub(crate) fn try_reuse_statement(&mut self, _attributes: Green, _is_global: bool) -> Green {
        Green::NULL
    }

    /// `private StatementSyntax ParseStatementCoreRest(SyntaxList<AttributeListSyntax> attributes, bool isGlobal, ref ResetPoint resetPointBeforeStatement)` (LanguageParser.cs:8441).
    pub(crate) fn parse_statement_core_rest(
        &mut self,
        attributes: Green,
        is_global: bool,
        reset_point_before_statement: &mut ResetPoint,
    ) -> Green {
        let is_global = is_global && self.is_script();

        if !self.is_possible_local_declaration_statement(is_global) {
            return self.parse_expression_statement(attributes);
        }

        if is_global {
            // if we're at the global script level, then we don't support local-decls or
            // local-funcs. The caller instead will look for those and parse them as
            // fields/methods in the global script scope.
            return Green::NULL;
        }

        let begins_with_await = self.current_contextual_kind() == SyntaxKind::AwaitKeyword;
        let mut result = self.parse_local_declaration_statement(attributes);

        // didn't get any sort of statement.  This was something else entirely
        // (like just a `}`).  No need to retry anything here.  Just reset back
        // to where we started from and bail entirely from parsing a statement.
        if result.is_null() {
            self.reset(reset_point_before_statement);
            return Green::NULL;
        }

        if self.arena_ref().contains_diagnostics(result) && begins_with_await && !self.is_in_async {
            // Local decl had issues.  We were also starting with 'await' in a non-async
            // context. Retry parsing this as if we were in an 'async' context as it's much
            // more likely that this was a misplace await-expr' than a local decl.
            //
            // The user will still get a later binding error about an await-expr in a non-async
            // context.
            self.reset(reset_point_before_statement);

            // using var _ = new ParserSyntaxContextResetter(this, isInAsyncContext: true);
            let saved = save_syntax_context(self);
            self.is_in_async = true;
            result = self.parse_expression_statement(attributes);
            restore_syntax_context(self, saved);
        }

        // Didn't want to retry as an `await expr`.  Just return what we actually
        // produced.
        result
    }

    /// `private StatementSyntax TryParseStatementStartingWithIdentifier(SyntaxList<AttributeListSyntax> attributes, bool isGlobal)` (LanguageParser.cs:8491).
    pub(crate) fn try_parse_statement_starting_with_identifier(&mut self, attributes: Green, is_global: bool) -> Green {
        if self.current_contextual_kind() == SyntaxKind::AwaitKeyword && self.peek_kind(1) == SyntaxKind::ForEachKeyword
        {
            let await_token = self.eat_contextual_token(SyntaxKind::AwaitKeyword);
            return self.parse_for_each_statement(attributes, await_token);
        } else if self.is_possible_await_using() {
            if self.peek_kind(2) == SyntaxKind::OpenParenToken {
                // `await using Type ...` is handled below in ParseLocalDeclarationStatement
                let await_token = self.eat_contextual_token(SyntaxKind::AwaitKeyword);
                return self.parse_using_statement(attributes, await_token);
            }
        } else if self.is_possible_labeled_statement() {
            return self.parse_labeled_statement(attributes);
        } else if self.is_possible_yield_statement() {
            return self.parse_yield_statement(attributes);
        } else if self.is_possible_await_expression_statement() {
            return self.parse_expression_statement(attributes);
        } else if self.is_query_expression(true, is_global && self.is_script()) {
            let query = self.parse_query_expression(Precedence::EXPRESSION);
            return self.parse_expression_statement_with_expression(attributes, query);
        }

        Green::NULL
    }

    /// `private StatementSyntax ParseStatementStartingWithUsing(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:8526).
    pub(crate) fn parse_statement_starting_with_using(&mut self, attributes: Green) -> Green {
        if self.peek_kind(1) == SyntaxKind::OpenParenToken {
            self.parse_using_statement(attributes, Green::NULL)
        } else {
            self.parse_local_declaration_statement(attributes)
        }
    }

    /// `private StatementSyntax TryParseStatementStartingWithUnsafe(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:8530).
    ///
    /// Checking for brace or parentheses to disambiguate between unsafe statement, unsafe expression, and unsafe
    /// local function
    pub(crate) fn try_parse_statement_starting_with_unsafe(&mut self, attributes: Green) -> Green {
        match self.peek_kind(1) {
            SyntaxKind::OpenParenToken => {
                self.parse_expression_statement_or_local_function_starting_with_unsafe(attributes)
            }
            SyntaxKind::OpenBraceToken => self.parse_unsafe_statement(attributes),
            _ => Green::NULL,
        }
    }

    /// `private StatementSyntax ParseExpressionStatementOrLocalFunctionStartingWithUnsafe(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:8540).
    pub(crate) fn parse_expression_statement_or_local_function_starting_with_unsafe(
        &mut self,
        attributes: Green,
    ) -> Green {
        let mut reset_point = self.get_disposable_reset_point(false);

        let result = self.parse_local_declaration_statement(attributes);
        if self.arena_ref().kind(result) == SyntaxKind::LocalFunctionStatement {
            self.dispose_reset_point(reset_point);
            return result;
        }

        self.reset_disposable(&mut reset_point);
        let result = self.parse_expression_statement(attributes);
        self.dispose_reset_point(reset_point);
        result
    }

    /// `private bool IsPossibleAwaitUsing()` (LanguageParser.cs:8554).
    pub(crate) fn is_possible_await_using(&mut self) -> bool {
        self.current_contextual_kind() == SyntaxKind::AwaitKeyword && self.peek_kind(1) == SyntaxKind::UsingKeyword
    }

    /// `private bool IsPossibleLabeledStatement()` (LanguageParser.cs:8557).
    pub(crate) fn is_possible_labeled_statement(&mut self) -> bool {
        self.peek_kind(1) == SyntaxKind::ColonToken && self.is_true_identifier()
    }

    /// `private bool IsPossibleYieldStatement()` (LanguageParser.cs:8562).
    pub(crate) fn is_possible_yield_statement(&mut self) -> bool {
        self.current_contextual_kind() == SyntaxKind::YieldKeyword
            && matches!(self.peek_kind(1), SyntaxKind::ReturnKeyword | SyntaxKind::BreakKeyword)
    }

    /// `private bool IsPossibleLocalDeclarationStatement(bool isGlobalScriptLevel)` (LanguageParser.cs:8568).
    pub(crate) fn is_possible_local_declaration_statement(&mut self, is_global_script_level: bool) -> bool {
        // This method decides whether to parse a statement as a
        // declaration or as an expression statement. In the old
        // compiler it would simply call IsLocalDeclaration.

        let tk = self.current_kind();
        if tk == SyntaxKind::RefKeyword
            || Self::is_declaration_modifier(tk) // treat `static int x = 2;` as a local variable declaration
            || (facts::is_predefined_type(tk)
                && !matches!(
                    self.peek_kind(1),
                    SyntaxKind::DotToken // e.g. `int.Parse()` is an expression
                        | SyntaxKind::OpenParenToken // e.g. `int (x, y)` is an error decl expression
                ))
        {
            return true;
        }

        // note: `using (` and `await using (` are already handled in ParseStatementCore.
        if tk == SyntaxKind::UsingKeyword {
            debug_assert!(self.peek_kind(1) != SyntaxKind::OpenParenToken);
            return true;
        }

        if self.is_possible_await_using() {
            debug_assert!(self.peek_kind(2) != SyntaxKind::OpenParenToken);
            return true;
        }

        if self.is_definite_scoped_modifier(false, false) {
            return true;
        }

        let tk = self.current_contextual_kind();

        let is_possible_modifier = Self::is_additional_local_function_modifier(tk)
            && (!matches!(tk, SyntaxKind::AsyncKeyword | SyntaxKind::SafeKeyword | SyntaxKind::ScopedKeyword)
                || self.should_contextual_keyword_be_treated_as_modifier(true));
        if is_possible_modifier {
            return true;
        }

        self.is_possible_first_typed_identifier_in_local_declaration_statement(is_global_script_level)
    }

    /// `private bool IsPossibleFirstTypedIdentifierInLocalDeclarationStatement(bool isGlobalScriptLevel)` (LanguageParser.cs:8615).
    pub(crate) fn is_possible_first_typed_identifier_in_local_declaration_statement(
        &mut self,
        is_global_script_level: bool,
    ) -> bool {
        let current = self.current_token();
        let next = self.peek_token(1);
        if let Some(typed_identifier) = self.is_possible_typed_identifier_start(current, next, false) {
            return typed_identifier;
        }

        // It's common to have code like the following:
        //
        //      Task.
        //      await Task.Delay()
        //
        // In this case we don't want to parse this as a local declaration like:
        //
        //      Task.await Task
        //
        // This does not represent user intent, and it causes all sorts of problems to higher
        // layers.  This is because both the parse tree is strange, and the symbol tables have
        // entries that throw things off (like a bogus 'Task' local).
        //
        // Note that we explicitly do this check when we see that the code spreads over multiple
        // lines.  We don't want this if the user has actually written "X.Y z"
        let tk = self.current_contextual_kind();

        if tk == SyntaxKind::IdentifierToken {
            let token1 = self.peek_token(1);
            if self.arena_ref().kind(token1) == SyntaxKind::DotToken
                && trailing_trivia_has_end_of_line(self.arena_ref(), token1)
                && self.peek_kind(2) == SyntaxKind::IdentifierToken
                && self.peek_kind(3) == SyntaxKind::IdentifierToken
            {
                // We have something like:
                //
                //      X.
                //      Y z
                //
                // This is only a local declaration if we have:
                //
                //      X.Y z;
                //      X.Y z = ...
                //      X.Y z, ...
                //      X.Y z( ...      (local function)
                //      X.Y z<W...      (local function)
                //
                let token4_kind = self.peek_kind(4);
                if token4_kind != SyntaxKind::SemicolonToken
                    && token4_kind != SyntaxKind::EqualsToken
                    && token4_kind != SyntaxKind::CommaToken
                    && token4_kind != SyntaxKind::OpenParenToken
                    && token4_kind != SyntaxKind::LessThanToken
                {
                    return false;
                }
            }
        }

        let reset_point = self.get_disposable_reset_point(true);

        let result = 'result: {
            let st = self.scan_type(false);

            // We could always return true for st == AliasQualName in addition to MustBeType on the first line, however, we want it to return false in the case where
            // CurrentToken.Kind != SyntaxKind.Identifier so that error cases, like: A::N(), are not parsed as variable declarations and instead are parsed as A.N() where we can give
            // a better error message saying "did you meant to use a '.'?"
            if st == ScanTypeFlags::MUST_BE_TYPE
                && !matches!(self.current_kind(), SyntaxKind::DotToken | SyntaxKind::OpenParenToken)
            {
                break 'result true;
            }

            if st == ScanTypeFlags::NOT_TYPE {
                break 'result false;
            }

            if self.current_kind() != SyntaxKind::IdentifierToken {
                // In the case of something like:
                //     List<SomeType>
                //     if
                // we know that we're in an error case, as the following keyword must be the start of a new statement.
                // We'd prefer to assume that this is an incomplete local declaration over an expression, as it's more likely
                // the user is just in the middle of writing a local declaration, and not an expression.
                break 'result st == ScanTypeFlags::GENERIC_TYPE_OR_EXPRESSION
                    && (self.is_definite_statement()
                        || self.is_type_declaration_start()
                        || Self::is_accessibility_modifier(self.current_kind()));
            }

            // T? and T* might start an expression, we need to parse further to disambiguate:
            if is_global_script_level {
                if st == ScanTypeFlags::POINTER_OR_MULTIPLICATION {
                    break 'result false;
                } else if st == ScanTypeFlags::NULLABLE_TYPE {
                    break 'result self
                        .is_possible_declaration_statement_following_nullable_type(is_global_script_level);
                }
            }

            true
        };

        self.dispose_reset_point(reset_point);
        result
    }

    /// `private bool IsPossibleTopLevelUsingLocalDeclarationStatement()` (LanguageParser.cs:8719).
    pub(crate) fn is_possible_top_level_using_local_declaration_statement(&mut self) -> bool {
        if self.current_kind() != SyntaxKind::UsingKeyword {
            return false;
        }

        let tk = self.peek_kind(1);

        if tk == SyntaxKind::RefKeyword {
            return true;
        }

        if Self::is_declaration_modifier(tk) {
            // treat `const int x = 2;` as a local variable declaration
            if tk != SyntaxKind::StaticKeyword {
                // For `static` we still need to make sure we have a typed identifier after it, because `using static type;` is a valid using directive.
                return true;
            }
        } else if facts::is_predefined_type(tk) {
            return true;
        }

        let reset_point = self.get_disposable_reset_point(true);

        // Skip 'using' keyword
        self.eat_token();

        let result = if self.is_definite_scoped_modifier(false, false) {
            true
        } else {
            if tk == SyntaxKind::StaticKeyword {
                // Skip 'static' keyword
                self.eat_token();
            }

            self.is_possible_first_typed_identifier_in_local_declaration_statement(false)
        };

        self.dispose_reset_point(reset_point);
        result
    }

    /// `private bool IsPossibleDeclarationStatementFollowingNullableType(bool isGlobalScriptLevel)` (LanguageParser.cs:8765).
    ///
    /// Looks ahead for a declaration of a field, property or method declaration following a nullable type T?.
    pub(crate) fn is_possible_declaration_statement_following_nullable_type(
        &mut self,
        is_global_script_level: bool,
    ) -> bool {
        if self.is_field_declaration(false, is_global_script_level) {
            return self.is_possible_field_declaration_following_nullable_type();
        }

        let mut explicit_interface_opt = Green::NULL;
        let mut identifier_or_this_opt = Green::NULL;
        let mut type_parameter_list_opt = Green::NULL;
        self.parse_member_name(
            &mut explicit_interface_opt,
            &mut identifier_or_this_opt,
            &mut type_parameter_list_opt,
            false,
        );

        if explicit_interface_opt.is_null() && identifier_or_this_opt.is_null() && type_parameter_list_opt.is_null() {
            return false;
        }

        // looks like a property:
        //      T? Goo {
        //
        // Importantly, we don't consider `T? Goo =>` to be the start of a property.  This is because it's legal to write:
        //      T ? Goo => Goo : Bar => Bar
        if self.current_kind() == SyntaxKind::OpenBraceToken {
            return true;
        }

        // don't accept indexers:
        if self.arena_ref().kind(identifier_or_this_opt) == SyntaxKind::ThisKeyword {
            return false;
        }

        self.is_possible_method_declaration_following_nullable_type()
    }

    /// `private bool IsPossibleFieldDeclarationFollowingNullableType()` (LanguageParser.cs:8806).
    ///
    /// At least one variable declaration terminated by a semicolon or a comma.
    ///   idf;
    ///   idf,
    ///   idf = <expr>;
    ///   idf = <expr>,
    pub(crate) fn is_possible_field_declaration_following_nullable_type(&mut self) -> bool {
        if self.current_kind() != SyntaxKind::IdentifierToken {
            return false;
        }

        self.eat_token();

        if self.current_kind() == SyntaxKind::EqualsToken {
            let save_term = self.term_state;
            self.term_state = self.term_state | TerminatorState::IS_END_OF_FIELD_DECLARATION;
            self.eat_token();
            self.parse_variable_initializer();
            self.term_state = save_term;
        }

        matches!(self.current_kind(), SyntaxKind::CommaToken | SyntaxKind::SemicolonToken)
    }

    /// `private bool IsPossibleMethodDeclarationFollowingNullableType()` (LanguageParser.cs:8827).
    pub(crate) fn is_possible_method_declaration_following_nullable_type(&mut self) -> bool {
        let save_term = self.term_state;
        self.term_state = self.term_state | TerminatorState::IS_END_OF_METHOD_SIGNATURE;

        let param_list = self.parse_parenthesized_parameter_list(false);

        self.term_state = save_term;
        let a = self.arena_ref();
        let separated_parameters = a.slot(param_list, nt::ParameterListSyntax::PARAMETERS);
        let close_paren_missing = a.is_missing(a.slot(param_list, nt::ParameterListSyntax::CLOSE_PAREN_TOKEN));

        // parsed full signature:
        if !close_paren_missing {
            // (...) {
            // (...) where
            if self.current_kind() == SyntaxKind::OpenBraceToken
                || self.current_contextual_kind() == SyntaxKind::WhereKeyword
            {
                return true;
            }

            // disambiguates conditional expressions
            // (...) :
            if self.current_kind() == SyntaxKind::ColonToken {
                return false;
            }
        }

        let a = self.arena_ref();
        let count = list_count(a, separated_parameters);

        // no parameters, just an open paren followed by a token that doesn't belong to a parameter definition:
        if count == 0 {
            return false;
        }

        let parameter = list_item(a, separated_parameters, 0);

        // has an attribute:
        //   ([Attr]
        if list_count(a, a.slot(parameter, nt::ParameterSyntax::ATTRIBUTE_LISTS)) > 0 {
            return true;
        }

        // has params modifier:
        //   (params
        let modifiers = a.slot(parameter, nt::ParameterSyntax::MODIFIERS);
        for i in 0..list_count(a, modifiers) {
            if a.kind(list_item(a, modifiers, i)) == SyntaxKind::ParamsKeyword {
                return true;
            }
        }

        let type_ = a.slot(parameter, nt::ParameterSyntax::TYPE);
        let identifier = a.slot(parameter, nt::ParameterSyntax::IDENTIFIER);
        if type_.is_null() {
            // has arglist:
            //   (__arglist
            if a.kind(identifier) == SyntaxKind::ArgListKeyword {
                return true;
            }
        } else if a.kind(type_) == SyntaxKind::NullableType {
            // nullable type with modifiers
            //   (ref T?
            //   (out T?
            if list_count(a, modifiers) > 0 {
                return true;
            }

            // nullable type, identifier, and separator or closing parent
            //   (T ? idf,
            //   (T ? idf)
            if !a.is_missing(identifier)
                && (count >= 2 && !a.is_missing(list_item(a, separated_parameters, 1))
                    || count == 1 && !close_paren_missing)
            {
                return true;
            }
        } else if a.kind(type_) == SyntaxKind::IdentifierName
            && a.contextual_kind(a.slot(type_, nt::IdentifierNameSyntax::IDENTIFIER)) == SyntaxKind::FromKeyword
        {
            // assume that "from" is meant to be a query start ("from" bound to a type is rare):
            // (from
            return false;
        } else {
            // has a name and a non-nullable type:
            //   (T idf
            //   (ref T idf
            //   (out T idf
            if !a.is_missing(identifier) {
                return true;
            }
        }

        false
    }

    /// `private bool IsAnonymousDelegateExpression()` (LanguageParser.cs:8939).
    ///
    /// Determines if the current 'delegate' keyword starts an anonymous delegate expression rather than a delegate
    /// type declaration: true if this is an anonymous delegate expression (e.g., delegate { } or delegate (params)
    /// { }), false if this is likely a delegate type declaration (e.g., delegate Type Name(params);)
    pub(crate) fn is_anonymous_delegate_expression(&mut self) -> bool {
        debug_assert!(self.current_kind() == SyntaxKind::DelegateKeyword);

        let next_token = self.peek_kind(1);

        // delegate { } is definitely an anonymous delegate
        if next_token == SyntaxKind::OpenBraceToken {
            return true;
        }

        // If not followed by '(', it's a delegate type declaration
        if next_token != SyntaxKind::OpenParenToken {
            return false;
        }

        // Now we have 'delegate (' - need to distinguish:
        // - Anonymous delegate: delegate (params) { }
        // - Delegate declaration: delegate (TupleType) Name(params);
        //
        // Try to parse what's in the parentheses as a tuple type, and check if
        // it's followed by an identifier (which would indicate a delegate declaration).

        let reset_point = self.get_disposable_reset_point(true);

        // Skip 'delegate'
        self.eat_token();

        debug_assert!(self.current_kind() == SyntaxKind::OpenParenToken);
        // Skip '('
        self.eat_token();

        // Try to scan as a tuple type
        let mut last_token_of_type = Green::NULL;
        let scan_result = self.scan_tuple_type(&mut last_token_of_type);

        // If it successfully scanned as a tuple type and is followed by an identifier,
        // it's a delegate type declaration with a tuple return type.
        // Otherwise, assume it's an anonymous delegate expression.
        let result = !(scan_result == ScanTypeFlags::TUPLE_TYPE && self.current_kind() == SyntaxKind::IdentifierToken);

        self.dispose_reset_point(reset_point);
        result
    }

    /// `private bool IsPossibleNewExpression()` (LanguageParser.cs:8987).
    pub(crate) fn is_possible_new_expression(&mut self) -> bool {
        debug_assert!(self.current_kind() == SyntaxKind::NewKeyword);

        // skip new
        let next_token = self.peek_token(1);
        let next_kind = self.arena_ref().kind(next_token);

        // new { }
        // new [ ]
        if matches!(next_kind, SyntaxKind::OpenBraceToken | SyntaxKind::OpenBracketToken) {
            return true;
        }

        //
        // Declaration with new modifier vs. new expression
        // Parse it as an expression if the type is not followed by an identifier or this keyword.
        //
        // Member declarations:
        //   new T Idf ...
        //   new T this ...
        //   new partial Idf    ("partial" as a type name)
        //   new partial this   ("partial" as a type name)
        //   new partial T Idf
        //   new partial T this
        //   new <modifier>
        //   new <class|interface|struct|enum>
        //   new partial <class|interface|struct|enum>
        //
        // New expressions:
        //   new T []
        //   new T { }
        //   new <non-type>
        //   new partial []
        //
        if facts::get_base_type_declaration_kind(next_kind) != SyntaxKind::None {
            return false;
        }

        let modifier = self.get_modifier_excluding_scoped(next_token);
        if modifier == DeclarationModifiers::PARTIAL {
            if facts::is_predefined_type(self.peek_kind(2)) {
                return false;
            }

            // class, struct, enum, interface keywords, but also other modifiers that are not allowed after
            // partial keyword but start class declaration, so we can assume the user just swapped them.
            if Self::is_type_modifier_or_type_keyword(self.peek_kind(2)) {
                return false;
            }
        } else if modifier != DeclarationModifiers::NONE {
            return false;
        }

        let token2 = self.peek_token(2);
        if let Some(typed_identifier) = self.is_possible_typed_identifier_start(next_token, token2, true) {
            // new Idf Idf
            // new Idf .
            // new partial T
            // new partial .
            return !typed_identifier;
        }

        let reset_point = self.get_disposable_reset_point(true);

        // skips new keyword
        self.eat_token();
        let st = self.scan_type(false);

        let result = !self.is_possible_member_name() || st == ScanTypeFlags::NOT_TYPE;
        self.dispose_reset_point(reset_point);
        result
    }

    /// `private bool? IsPossibleTypedIdentifierStart(SyntaxToken current, SyntaxToken next, bool allowThisKeyword)` (LanguageParser.cs:9073).
    ///
    /// Some(true) if the current token can be the first token of a typed identifier (a type name followed by an
    /// identifier), Some(false) if it definitely can't be, None if we need to scan further to find out.
    pub(crate) fn is_possible_typed_identifier_start(
        &mut self,
        current: Green,
        next: Green,
        allow_this_keyword: bool,
    ) -> Option<bool> {
        if self.is_true_identifier_with_token(current) {
            match self.arena_ref().kind(next) {
                // tokens that can be in type names...
                SyntaxKind::DotToken
                | SyntaxKind::AsteriskToken
                | SyntaxKind::QuestionToken
                | SyntaxKind::OpenBracketToken
                | SyntaxKind::LessThanToken
                | SyntaxKind::ColonColonToken => return None,

                SyntaxKind::OpenParenToken => {
                    if is_identifier_var(self.arena_ref(), current) {
                        // potentially either a tuple type in a local declaration (true), or
                        // a tuple lvalue in a deconstruction assignment (false).
                        return None;
                    } else {
                        return Some(false);
                    }
                }

                SyntaxKind::IdentifierToken => return Some(self.is_true_identifier_with_token(next)),

                SyntaxKind::ThisKeyword => return Some(allow_this_keyword),

                _ => return Some(false),
            }
        }

        None
    }

    /// `private BlockSyntax ParsePossiblyAttributedBlock()` (LanguageParser.cs:9114).
    pub(crate) fn parse_possibly_attributed_block(&mut self) -> Green {
        let attributes = self.parse_attribute_declarations(false);
        self.parse_block(attributes)
    }

    /// `private BlockSyntax ParseMethodOrAccessorBodyBlock(SyntaxList<AttributeListSyntax> attributes, bool isAccessorBody)` (LanguageParser.cs:9122).
    ///
    /// Used to parse the block-body for a method or accessor.  For blocks that appear *inside* method bodies, call
    /// `parse_block`. `is_accessor_body`: if is true, then we produce a special diagnostic if the open brace is
    /// missing.
    pub(crate) fn parse_method_or_accessor_body_block(&mut self, attributes: Green, is_accessor_body: bool) -> Green {
        // Check again for incremental re-use.  This way if a method signature is edited we can
        // still quickly re-sync on the body. (Not ported: no incremental parsing.)

        // There's a special error code for a missing token after an accessor keyword
        let mut open_brace = if is_accessor_body && self.current_kind() != SyntaxKind::OpenBraceToken {
            let missing = self.arena().missing_token(SyntaxKind::OpenBraceToken);
            let code = if self.is_feature_enabled(MessageID::IDS_FeatureExpressionBodiedAccessor) {
                ErrorCode::ERR_SemiOrLBraceOrArrowExpected
            } else {
                ErrorCode::ERR_SemiOrLBraceExpected
            };
            self.add_error(missing, code)
        } else {
            self.eat_token_kind(SyntaxKind::OpenBraceToken)
        };

        let mut statements = self.pool.allocate();
        self.parse_statements(&mut open_brace, &mut statements, false);

        // Force creation a many-children list, even if only 1, 2, or 3 elements in the statement list: the
        // many-children list Roslyn makes for a large enough list is the same tree as the list itself here.
        let _ = self.is_large_enough_non_empty_statement_list(&mut statements);
        let statements_list = self.base.arena().list(&statements);
        let close_brace = self.eat_token_kind(SyntaxKind::CloseBraceToken);
        let block = self.arena().block(attributes, open_brace, statements_list, close_brace);

        self.pool.free(statements);
        block
    }

    /// `private BlockSyntax ParseBlock(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:9162).
    ///
    /// Used to parse normal blocks that appear inside method bodies.  For the top level block of a method/accessor
    /// use `parse_method_or_accessor_body_block`.
    pub(crate) fn parse_block(&mut self, attributes: Green) -> Green {
        // Check again for incremental re-use, since ParseBlock is called from a bunch of places
        // other than ParseStatementCore() (not ported: no incremental parsing).

        let mut open_brace = self.eat_token_kind(SyntaxKind::OpenBraceToken);

        let mut statements = self.pool.allocate();
        self.parse_statements(&mut open_brace, &mut statements, false);

        let statements_list = self.pool.to_list_and_free(self.base.arena(), statements);
        let close_brace = self.eat_token_kind(SyntaxKind::CloseBraceToken);
        self.arena().block(attributes, open_brace, statements_list, close_brace)
    }

    /// `private static bool IsLargeEnoughNonEmptyStatementList(SyntaxListBuilder<StatementSyntax> statements)` (LanguageParser.cs:9184).
    ///
    /// Is this statement list non-empty, and large enough to make using weak children beneficial?
    pub(crate) fn is_large_enough_non_empty_statement_list(&self, statements: &mut SyntaxListBuilder) -> bool {
        match statements.as_slice() {
            [] => false,
            // If we have a single statement, it might be small, like "return null", or large,
            // like a loop or if or switch with many statements inside. Use the width as a proxy for
            // how big it is. If it's small, its better to forgo a many children list anyway, since the
            // weak reference would consume as much memory as is saved.
            [statement] => self.arena_ref().width(*statement) > 60,
            // For 2 or more statements, go ahead and create a many-children lists.
            _ => true,
        }
    }

    /// `private void ParseStatements(ref CSharpSyntaxNode previousNode, SyntaxListBuilder<StatementSyntax> statements, bool stopOnSwitchSections)` (LanguageParser.cs:9205).
    pub(crate) fn parse_statements(
        &mut self,
        previous_node: &mut Green,
        statements: &mut SyntaxListBuilder,
        stop_on_switch_sections: bool,
    ) {
        let save_term = self.term_state;
        self.term_state = self.term_state | TerminatorState::IS_POSSIBLE_STATEMENT_START_OR_STOP; // partial statements can abort if a new statement starts
        if stop_on_switch_sections {
            self.term_state = self.term_state | TerminatorState::IS_SWITCH_SECTION_START;
        }

        let mut last_token_position = -1;
        while !matches!(self.current_kind(), SyntaxKind::CloseBraceToken | SyntaxKind::EndOfFileToken)
            && !(stop_on_switch_sections && self.is_possible_switch_section())
            && self.is_making_progress(&mut last_token_position, true)
        {
            if self.is_possible_statement() {
                let statement = self.parse_possibly_attributed_statement();
                if !statement.is_null() {
                    statements.push(statement);
                    continue;
                }
            }

            let mut trailing_trivia = Green::NULL;
            let action =
                self.skip_bad_statement_list_tokens(statements, SyntaxKind::CloseBraceToken, &mut trailing_trivia);
            if !trailing_trivia.is_null() {
                *previous_node = self.add_trailing_skipped_syntax(*previous_node, trailing_trivia);
            }

            if action == PostSkipAction::Abort {
                break;
            }
        }

        self.term_state = save_term;
    }

    /// `private bool IsPossibleStatementStartOrStop()` (LanguageParser.cs:9245).
    pub(crate) fn is_possible_statement_start_or_stop(&mut self) -> bool {
        self.current_kind() == SyntaxKind::SemicolonToken || self.is_possible_statement()
    }

    /// `private PostSkipAction SkipBadStatementListTokens(SyntaxListBuilder<StatementSyntax> statements, SyntaxKind expected, out GreenNode trailingTrivia)` (LanguageParser.cs:9251).
    pub(crate) fn skip_bad_statement_list_tokens(
        &mut self,
        statements: &mut SyntaxListBuilder,
        expected: SyntaxKind,
        trailing_trivia: &mut Green,
    ) -> PostSkipAction {
        self.skip_bad_list_tokens_with_expected_kind_helper(
            statements,
            // We know we have a bad statement, so it can't be a local
            // function, meaning we shouldn't consider accessibility
            // modifiers to be the start of a statement
            |p| !p.is_possible_statement(),
            |p, _| p.current_kind() == SyntaxKind::CloseBraceToken,
            expected,
            SyntaxKind::None,
            trailing_trivia,
        )
    }

    /// `private bool IsDefiniteStatement()` (LanguageParser.cs:9265).
    pub(crate) fn is_definite_statement(&mut self) -> bool {
        let tk = self.current_kind();
        // Only those cases that can be certain start a new statement, regardless of context
        matches!(
            tk,
            SyntaxKind::FixedKeyword
                | SyntaxKind::BreakKeyword
                | SyntaxKind::ContinueKeyword
                | SyntaxKind::TryKeyword
                | SyntaxKind::ConstKeyword
                | SyntaxKind::DoKeyword
                | SyntaxKind::ForKeyword
                | SyntaxKind::ForEachKeyword
                | SyntaxKind::GotoKeyword
                | SyntaxKind::IfKeyword
                | SyntaxKind::ElseKeyword
                | SyntaxKind::LockKeyword
                | SyntaxKind::ReturnKeyword
                | SyntaxKind::UnsafeKeyword
                | SyntaxKind::UsingKeyword
                | SyntaxKind::WhileKeyword
                | SyntaxKind::VolatileKeyword
                | SyntaxKind::ExternKeyword
                | SyntaxKind::CaseKeyword // for parsing an errant case without a switch.
        )
    }

    /// `private bool IsPossibleStatement()` (LanguageParser.cs:9297).
    pub(crate) fn is_possible_statement(&mut self) -> bool {
        if self.is_definite_statement() {
            return true;
        }

        let tk = self.current_kind();
        match tk {
            SyntaxKind::CheckedKeyword
            | SyntaxKind::UncheckedKeyword
            | SyntaxKind::ThrowKeyword
            | SyntaxKind::SwitchKeyword
            | SyntaxKind::OpenBraceToken
            | SyntaxKind::SemicolonToken
            | SyntaxKind::StaticKeyword
            | SyntaxKind::ReadOnlyKeyword
            | SyntaxKind::RefKeyword
            | SyntaxKind::OpenBracketToken => true,

            SyntaxKind::IdentifierToken => self.is_true_identifier(),

            _ => Self::is_predefined_type(tk) || self.is_possible_expression(),
        }
    }

    /// `private FixedStatementSyntax ParseFixedStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:9328).
    pub(crate) fn parse_fixed_statement(&mut self, attributes: Green) -> Green {
        let fixed = self.eat_token_kind(SyntaxKind::FixedKeyword);
        let open_paren = self.eat_token_kind(SyntaxKind::OpenParenToken);

        let save_term = self.term_state;
        self.term_state = self.term_state | TerminatorState::IS_END_OF_FIXED_STATEMENT;
        let decl = self.parse_parenthesized_variable_declaration(VariableFlags::NONE, Green::NULL);
        self.term_state = save_term;

        let close_paren = self.eat_token_kind(SyntaxKind::CloseParenToken);
        let statement = self.parse_embedded_statement();
        self.arena().fixed_statement(attributes, fixed, open_paren, decl, close_paren, statement)
    }

    /// `private bool IsEndOfFixedStatement()` (LanguageParser.cs:9347).
    pub(crate) fn is_end_of_fixed_statement(&mut self) -> bool {
        matches!(
            self.current_kind(),
            SyntaxKind::CloseParenToken | SyntaxKind::OpenBraceToken | SyntaxKind::SemicolonToken
        )
    }

    /// `private StatementSyntax ParseEmbeddedStatement()` (LanguageParser.cs:9352).
    pub(crate) fn parse_embedded_statement(&mut self) -> Green {
        // ParseEmbeddedStatement is called through many recursive statement parsing cases. We
        // keep the body exceptionally simple, and we optimize for the common case, to ensure it
        // is inlined into the callers.  Otherwise the overhead of this single method can have a
        // deep impact on the number of recursive calls we can make (more than a hundred during
        // empirical testing).

        let statement = self.parse_possibly_attributed_statement();

        // `StatementSyntax parseEmbeddedStatementRest(StatementSyntax statement)`
        if statement.is_null() {
            // The consumers of embedded statements are expecting to receive a non-null statement
            // yet there are several error conditions that can lead ParseStatementCore to return
            // null.  When that occurs create an error empty Statement and return it to the caller.
            let semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
            return self.arena().empty_statement(Green::NULL, semicolon);
        }

        // In scripts, stand-alone expression statements may not be followed by semicolons.
        // ParseExpressionStatement hides the error.
        // However, embedded expression statements are required to be followed by semicolon.
        if self.arena_ref().kind(statement) == SyntaxKind::ExpressionStatement && self.is_script() {
            let a = self.arena_ref();
            let attribute_lists = a.slot(statement, nt::ExpressionStatementSyntax::ATTRIBUTE_LISTS);
            let expression = a.slot(statement, nt::ExpressionStatementSyntax::EXPRESSION);
            let semicolon_token = a.slot(statement, nt::ExpressionStatementSyntax::SEMICOLON_TOKEN);

            // Do not add a new error if the same error was already added.
            if a.is_missing(semicolon_token)
                && !a.diagnostics(semicolon_token).any(|d| a.diagnostic(d).code == ErrorCode::ERR_SemicolonExpected)
            {
                let semicolon_token = self.add_error(semicolon_token, ErrorCode::ERR_SemicolonExpected);
                return self.arena().update_node(statement, &[attribute_lists, expression, semicolon_token]);
            }
        }

        statement
    }

    /// `private BreakStatementSyntax ParseBreakStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:9394).
    pub(crate) fn parse_break_statement(&mut self, attributes: Green) -> Green {
        let break_keyword = self.eat_token_kind(SyntaxKind::BreakKeyword);
        let name = if self.is_true_identifier() {
            self.parse_identifier_name(ErrorCode::ERR_IdentifierExpected)
        } else {
            Green::NULL
        };
        let semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
        self.arena().break_statement(attributes, break_keyword, name, semicolon)
    }

    /// `private ContinueStatementSyntax ParseContinueStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:9403).
    pub(crate) fn parse_continue_statement(&mut self, attributes: Green) -> Green {
        let continue_keyword = self.eat_token_kind(SyntaxKind::ContinueKeyword);
        let name = if self.is_true_identifier() {
            self.parse_identifier_name(ErrorCode::ERR_IdentifierExpected)
        } else {
            Green::NULL
        };
        let semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
        self.arena().continue_statement(attributes, continue_keyword, name, semicolon)
    }

    /// `private TryStatementSyntax ParseTryStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:9412).
    pub(crate) fn parse_try_statement(&mut self, attributes: Green) -> Green {
        debug_assert!(matches!(
            self.current_kind(),
            SyntaxKind::TryKeyword | SyntaxKind::CatchKeyword | SyntaxKind::FinallyKeyword
        ));

        // `BlockSyntax missingBlock()`
        fn missing_block(a: &mut GreenArena) -> Green {
            let open_brace = a.missing_token(SyntaxKind::OpenBraceToken);
            let close_brace = a.missing_token(SyntaxKind::CloseBraceToken);
            a.block(Green::NULL, open_brace, Green::NULL, close_brace)
        }

        // We are called into on try/catch/finally, so eating the try may actually fail.
        let try_ = self.eat_token_kind(SyntaxKind::TryKeyword);

        let mut try_block = if self.arena_ref().is_missing(try_) {
            // If there was no actual `try`, then we got here because of a misplaced `catch`/`finally`.  In that
            // case just synthesize a fully missing try-block.  We will already have issued a diagnostic on the
            // `try` keyword, so we don't need to issue any more.

            debug_assert!(self.arena_ref().contains_diagnostics(try_));
            debug_assert!(matches!(self.current_kind(), SyntaxKind::CatchKeyword | SyntaxKind::FinallyKeyword));

            missing_block(self.arena())
        } else {
            let save_term = self.term_state;
            self.term_state = self.term_state | TerminatorState::IS_END_OF_TRY_BLOCK;
            let block = self.parse_possibly_attributed_block();
            self.term_state = save_term;
            block
        };

        let mut catch_clauses: Option<SyntaxListBuilder> = None;
        let mut finally_clause = Green::NULL;
        if self.current_kind() == SyntaxKind::CatchKeyword {
            let mut clauses = self.pool.allocate();
            while self.current_kind() == SyntaxKind::CatchKeyword {
                let clause = self.parse_catch_clause();
                clauses.push(clause);
            }
            catch_clauses = Some(clauses);
        }

        if self.current_kind() == SyntaxKind::FinallyKeyword {
            let finally_keyword = self.eat_token();
            let block = self.parse_possibly_attributed_block();
            finally_clause = self.arena().finally_clause(finally_keyword, block);
        }

        if catch_clauses.is_none() && finally_clause.is_null() {
            if !self.contains_error_diagnostic(try_block) {
                try_block = self.add_error_to_last_token(try_block, ErrorCode::ERR_ExpectedEndTry);
            }

            // synthesize missing tokens for "finally { }":
            let a = self.arena();
            let finally_keyword = a.missing_token(SyntaxKind::FinallyKeyword);
            let block = missing_block(a);
            finally_clause = a.finally_clause(finally_keyword, block);
        }

        let catches = match catch_clauses {
            Some(clauses) => self.pool.to_list_and_free(self.base.arena(), clauses),
            None => Green::NULL,
        };
        self.arena().try_statement(attributes, try_, try_block, catches, finally_clause)
    }

    /// `private bool IsEndOfTryBlock()` (LanguageParser.cs:9483).
    pub(crate) fn is_end_of_try_block(&mut self) -> bool {
        matches!(
            self.current_kind(),
            SyntaxKind::CloseBraceToken | SyntaxKind::CatchKeyword | SyntaxKind::FinallyKeyword
        )
    }

    /// `private CatchClauseSyntax ParseCatchClause()` (LanguageParser.cs:9488).
    pub(crate) fn parse_catch_clause(&mut self) -> Green {
        debug_assert!(self.current_kind() == SyntaxKind::CatchKeyword);

        let catch_ = self.eat_token();

        let mut decl = Green::NULL;
        let save_term = self.term_state;

        if self.current_kind() == SyntaxKind::OpenParenToken {
            let open_paren = self.eat_token();

            self.term_state = self.term_state | TerminatorState::IS_END_OF_CATCH_CLAUSE;
            let type_ = self.parse_type(ParseTypeMode::Normal);
            let mut name = Green::NULL;
            if self.is_true_identifier() {
                name = self.parse_identifier_token(ErrorCode::ERR_IdentifierExpected);
            }

            self.term_state = save_term;

            let close_paren = self.eat_token_kind(SyntaxKind::CloseParenToken);
            decl = self.arena().catch_declaration(open_paren, type_, name, close_paren);
        }

        let mut filter = Green::NULL;

        let keyword_kind = self.current_contextual_kind();
        if keyword_kind == SyntaxKind::WhenKeyword || keyword_kind == SyntaxKind::IfKeyword {
            let mut when_keyword = self.eat_contextual_token(SyntaxKind::WhenKeyword);
            if keyword_kind == SyntaxKind::IfKeyword {
                // The initial design of C# exception filters called for the use of the
                // "if" keyword in this position.  We've since changed to "when", but
                // the error recovery experience for early adopters (and for old source
                // stored in the symbol server) will be better if we consume "if" as
                // though it were "when".
                let if_keyword = self.eat_token();
                when_keyword = self.add_trailing_skipped_syntax(when_keyword, if_keyword);
            }

            self.term_state = self.term_state | TerminatorState::IS_END_OF_FILTER_CLAUSE;
            let open_paren = self.eat_token_kind(SyntaxKind::OpenParenToken);
            let filter_expression = self.parse_expression_for_parenthesized_construct();

            self.term_state = save_term;
            let close_paren = self.eat_token_kind(SyntaxKind::CloseParenToken);
            filter = self.arena().catch_filter_clause(when_keyword, open_paren, filter_expression, close_paren);
        }

        self.term_state = self.term_state | TerminatorState::IS_END_OF_CATCH_BLOCK;
        let block = self.parse_possibly_attributed_block();
        self.term_state = save_term;

        self.arena().catch_clause(catch_, decl, filter, block)
    }

    /// `private bool IsEndOfCatchClause()` (LanguageParser.cs:9547).
    pub(crate) fn is_end_of_catch_clause(&mut self) -> bool {
        matches!(
            self.current_kind(),
            SyntaxKind::CloseParenToken
                | SyntaxKind::OpenBraceToken
                | SyntaxKind::CloseBraceToken
                | SyntaxKind::CatchKeyword
                | SyntaxKind::FinallyKeyword
        )
    }

    /// `private bool IsEndOfFilterClause()` (LanguageParser.cs:9556).
    pub(crate) fn is_end_of_filter_clause(&mut self) -> bool {
        matches!(
            self.current_kind(),
            SyntaxKind::CloseParenToken
                | SyntaxKind::OpenBraceToken
                | SyntaxKind::CloseBraceToken
                | SyntaxKind::CatchKeyword
                | SyntaxKind::FinallyKeyword
        )
    }

    /// `private bool IsEndOfCatchBlock()` (LanguageParser.cs:9564).
    pub(crate) fn is_end_of_catch_block(&mut self) -> bool {
        matches!(
            self.current_kind(),
            SyntaxKind::CloseBraceToken | SyntaxKind::CatchKeyword | SyntaxKind::FinallyKeyword
        )
    }

    /// `private StatementSyntax ParseCheckedStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:9571).
    pub(crate) fn parse_checked_statement(&mut self, attributes: Green) -> Green {
        debug_assert!(matches!(self.current_kind(), SyntaxKind::CheckedKeyword | SyntaxKind::UncheckedKeyword));

        if self.peek_kind(1) == SyntaxKind::OpenParenToken {
            return self.parse_expression_statement(attributes);
        }

        let keyword = self.eat_token();
        let kind = facts::get_check_statement(self.arena_ref().kind(keyword));
        let block = self.parse_possibly_attributed_block();
        self.arena().checked_statement(kind, attributes, keyword, block)
    }

    /// `private DoStatementSyntax ParseDoStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:9588).
    pub(crate) fn parse_do_statement(&mut self, attributes: Green) -> Green {
        debug_assert!(self.current_kind() == SyntaxKind::DoKeyword);
        let do_ = self.eat_token_kind(SyntaxKind::DoKeyword);
        let statement = self.parse_embedded_statement();
        let while_ = self.eat_token_kind(SyntaxKind::WhileKeyword);
        let open_paren = self.eat_token_kind(SyntaxKind::OpenParenToken);

        let save_term = self.term_state;
        self.term_state = self.term_state | TerminatorState::IS_END_OF_DO_WHILE_EXPRESSION;
        let expression = self.parse_expression_for_parenthesized_construct();
        self.term_state = save_term;

        let close_paren = self.eat_token_kind(SyntaxKind::CloseParenToken);
        let semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
        self.arena().do_statement(attributes, do_, statement, while_, open_paren, expression, close_paren, semicolon)
    }

    /// `private bool IsEndOfDoWhileExpression()` (LanguageParser.cs:9612).
    pub(crate) fn is_end_of_do_while_expression(&mut self) -> bool {
        matches!(self.current_kind(), SyntaxKind::CloseParenToken | SyntaxKind::SemicolonToken)
    }

    /// `private StatementSyntax ParseForOrForEachStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:9617).
    pub(crate) fn parse_for_or_for_each_statement(&mut self, attributes: Green) -> Green {
        // Check if the user wrote the following accidentally:
        //
        // for (SomeType t in
        //
        // instead of
        //
        // foreach (SomeType t in
        //
        // In that case, parse it as a foreach, but given the appropriate message that a
        // 'foreach' keyword was expected.
        let mut reset_point = self.get_disposable_reset_point(false);

        debug_assert!(self.current_kind() == SyntaxKind::ForKeyword);
        self.eat_token();
        let looks_like_foreach = {
            let open_paren = self.eat_token();
            self.arena_ref().kind(open_paren) == SyntaxKind::OpenParenToken
        } && self.scan_type(false) != ScanTypeFlags::NOT_TYPE
            && {
                let identifier = self.eat_token();
                self.arena_ref().kind(identifier) == SyntaxKind::IdentifierToken
            }
            && {
                let in_keyword = self.eat_token();
                self.arena_ref().kind(in_keyword) == SyntaxKind::InKeyword
            };
        let result = if looks_like_foreach {
            // Looks like a foreach statement.  Parse it that way instead
            self.reset_disposable(&mut reset_point);
            self.parse_for_each_statement(attributes, Green::NULL)
        } else {
            // Normal for statement.
            self.reset_disposable(&mut reset_point);
            self.parse_for_statement(attributes)
        };
        self.dispose_reset_point(reset_point);
        result
    }

    /// `private ForStatementSyntax ParseForStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:9650).
    pub(crate) fn parse_for_statement(&mut self, attributes: Green) -> Green {
        debug_assert!(self.current_kind() == SyntaxKind::ForKeyword);

        // `(VariableDeclarationSyntax variableDeclaration, SeparatedSyntaxList<ExpressionSyntax> initializers) eatVariableDeclarationOrInitializers()`
        fn eat_variable_declaration_or_initializers(
            this: &mut LanguageParser<'_, '_>,
            open_paren: &mut Green,
        ) -> (Green, Green) {
            let mut reset_point = this.get_disposable_reset_point(false);

            // Here can be either a declaration or an expression statement list.  Scan
            // for a declaration first.
            let mut is_declaration = false;

            if this.current_contextual_kind() == SyntaxKind::ScopedKeyword {
                if this.peek_kind(1) == SyntaxKind::RefKeyword {
                    is_declaration = true;
                } else {
                    this.eat_token();
                    is_declaration = this.scan_type(false) != ScanTypeFlags::NOT_TYPE
                        && this.current_kind() == SyntaxKind::IdentifierToken;
                    this.reset_disposable(&mut reset_point);
                }
            } else if this.current_kind() == SyntaxKind::RefKeyword {
                is_declaration = true;
            }

            if !is_declaration {
                is_declaration = !this.is_query_expression(true, false)
                    && this.scan_type(false) != ScanTypeFlags::NOT_TYPE
                    && this.is_true_identifier();
                this.reset_disposable(&mut reset_point);
            }

            let result = if is_declaration {
                let scoped_keyword = this.parse_possible_scoped_keyword(false, false);
                (
                    this.parse_parenthesized_variable_declaration(VariableFlags::FOR_STATEMENT, scoped_keyword),
                    Green::NULL,
                )
            } else if this.current_kind() != SyntaxKind::SemicolonToken {
                // Not a type followed by an identifier, so it must be the initializer expression list.
                //
                // Do not consume semicolons here as they are used to separate the initializers out from the
                // condition of the for loop.
                (Green::NULL, parse_for_statement_expression_list(this, open_paren, false))
            } else {
                (Green::NULL, Green::NULL)
            };
            this.dispose_reset_point(reset_point);
            result
        }

        // `SyntaxToken eatCommaOrSemicolon()`
        fn eat_comma_or_semicolon(this: &mut LanguageParser<'_, '_>) -> Green {
            if this.current_kind() == SyntaxKind::CommaToken {
                this.eat_token_as_kind(SyntaxKind::SemicolonToken)
            } else {
                this.eat_token_kind(SyntaxKind::SemicolonToken)
            }
        }

        // `SyntaxToken eatUnexpectedTokensAndCloseParenToken()`
        fn eat_unexpected_tokens_and_close_paren_token(this: &mut LanguageParser<'_, '_>) -> Green {
            let mut skipped_tokens = this.pool.allocate();

            while matches!(this.current_kind(), SyntaxKind::SemicolonToken | SyntaxKind::CommaToken) {
                let token = this.eat_token_even_with_incorrect_kind(SyntaxKind::CloseParenToken);
                skipped_tokens.push(token);
            }

            let result = this.eat_token_kind(SyntaxKind::CloseParenToken);
            let skipped = this.pool.to_list_and_free(this.base.arena(), skipped_tokens);
            this.add_leading_skipped_syntax(result, skipped)
        }

        // Parses out a sequence of expressions.  Both for the initializer section (the `for (initializer1,
        // initializer2, ...` section), as well as the incrementor section (the `for (;; incrementor1, incrementor2,
        // ...` section).
        // `SeparatedSyntaxList<ExpressionSyntax> parseForStatementExpressionList(ref SyntaxToken startToken, bool allowSemicolonAsSeparator)`
        fn parse_for_statement_expression_list(
            this: &mut LanguageParser<'_, '_>,
            start_token: &mut Green,
            allow_semicolon_as_separator: bool,
        ) -> Green {
            this.parse_comma_separated_syntax_list(
                start_token,
                SyntaxKind::CloseParenToken,
                |this| this.is_possible_expression(),
                |this| this.parse_expression_core(),
                skip_bad_for_statement_expression_list_tokens,
                false,
                false,
                allow_semicolon_as_separator,
            )
        }

        // `static PostSkipAction skipBadForStatementExpressionListTokens(LanguageParser @this, ref SyntaxToken startToken, SeparatedSyntaxListBuilder<ExpressionSyntax> list, SyntaxKind expectedKind, SyntaxKind closeKind)`
        fn skip_bad_for_statement_expression_list_tokens(
            this: &mut LanguageParser<'_, '_>,
            start_token: &mut Green,
            list: &mut SyntaxListBuilder,
            expected_kind: SyntaxKind,
            close_kind: SyntaxKind,
        ) -> PostSkipAction {
            if matches!(this.current_kind(), SyntaxKind::CloseParenToken | SyntaxKind::SemicolonToken) {
                return PostSkipAction::Abort;
            }

            this.skip_bad_separated_list_tokens_with_expected_kind(
                start_token,
                list,
                |p| p.current_kind() != SyntaxKind::CommaToken && !p.is_possible_expression(),
                |p, close_kind| p.current_kind() == close_kind || p.current_kind() == SyntaxKind::SemicolonToken,
                expected_kind,
                close_kind,
            )
        }

        let save_term = self.term_state;
        self.term_state = self.term_state | TerminatorState::IS_END_OF_FOR_STATEMENT_ARGUMENT;

        let for_token = self.eat_token_kind(SyntaxKind::ForKeyword);
        let mut open_paren = self.eat_token_kind(SyntaxKind::OpenParenToken);
        let (variable_declaration, initializers) = eat_variable_declaration_or_initializers(self, &mut open_paren);

        let first_semicolon_token = eat_comma_or_semicolon(self);
        let condition = if !matches!(self.current_kind(), SyntaxKind::SemicolonToken | SyntaxKind::CommaToken) {
            self.parse_expression_core()
        } else {
            Green::NULL
        };

        // Used to place skipped tokens we run into when parsing the incrementors list.
        let mut second_semicolon_token = eat_comma_or_semicolon(self);

        // Do allow semicolons (with diagnostics) in the incrementors list.  This allows us to consume
        // accidental extra incrementors that should have been separated by commas.
        let incrementors = if self.current_kind() != SyntaxKind::CloseParenToken {
            parse_for_statement_expression_list(self, &mut second_semicolon_token, true)
        } else {
            Green::NULL
        };

        let close_paren = eat_unexpected_tokens_and_close_paren_token(self);
        let statement = self.parse_embedded_statement();
        let for_statement = self.arena().for_statement(
            attributes,
            for_token,
            open_paren,
            variable_declaration,
            initializers,
            first_semicolon_token,
            condition,
            second_semicolon_token,
            incrementors,
            close_paren,
            statement,
        );

        self.term_state = save_term;

        for_statement
    }

    /// `private bool IsEndOfForStatementArgument()` (LanguageParser.cs:9787).
    pub(crate) fn is_end_of_for_statement_argument(&mut self) -> bool {
        matches!(
            self.current_kind(),
            SyntaxKind::SemicolonToken | SyntaxKind::CloseParenToken | SyntaxKind::OpenBraceToken
        )
    }

    /// `private CommonForEachStatementSyntax ParseForEachStatement( SyntaxList<AttributeListSyntax> attributes, SyntaxToken awaitTokenOpt)` (LanguageParser.cs:9792).
    pub(crate) fn parse_for_each_statement(&mut self, attributes: Green, await_token_opt: Green) -> Green {
        // Can be a 'for' keyword if the user typed: 'for (SomeType t in'
        debug_assert!(matches!(self.current_kind(), SyntaxKind::ForEachKeyword | SyntaxKind::ForKeyword));

        // Syntax for foreach is either:
        //  foreach [await] ( <type> <identifier> in <expr> ) <embedded-statement>
        // or
        //  foreach [await] ( <deconstruction-declaration> in <expr> ) <embedded-statement>

        // If we're at a 'for', then consume it and attach
        // it as skipped text to the missing 'foreach' token.
        let foreach = if self.current_kind() == SyntaxKind::ForKeyword {
            let skipped_for_token = self.eat_token();
            let skipped_for_token = self.add_error_args(
                skipped_for_token,
                ErrorCode::ERR_SyntaxError,
                &[DiagArg::Str(facts::get_text(SyntaxKind::ForEachKeyword))],
            );
            self.convert_to_missing_with_trailing_trivia(skipped_for_token, SyntaxKind::ForEachKeyword)
        } else {
            self.eat_token_kind(SyntaxKind::ForEachKeyword)
        };

        let open_paren = self.eat_token_kind(SyntaxKind::OpenParenToken);

        let variable = self.parse_expression_or_declaration(ParseTypeMode::Normal, true);
        let mut in_ = self.eat_token_code(SyntaxKind::InKeyword, ErrorCode::ERR_InExpected, true);
        if !self.is_valid_foreach_variable(variable) {
            in_ = self.add_error(in_, ErrorCode::ERR_BadForeachDecl);
        }

        let expression = self.parse_expression_core();
        let close_paren = self.eat_token_kind(SyntaxKind::CloseParenToken);
        let statement = self.parse_embedded_statement();

        if self.arena_ref().kind(variable) == SyntaxKind::DeclarationExpression {
            let decl = variable;
            let designation = self.arena_ref().slot(decl, nt::DeclarationExpressionSyntax::DESIGNATION);
            let designation_kind = self.arena_ref().kind(designation);
            if designation_kind != SyntaxKind::ParenthesizedVariableDesignation {
                // if we see a foreach declaration that isn't a deconstruction, we use the old form of foreach syntax node.
                let identifier = match designation_kind {
                    SyntaxKind::SingleVariableDesignation => {
                        self.arena_ref().slot(designation, nt::SingleVariableDesignationSyntax::IDENTIFIER)
                    }
                    SyntaxKind::DiscardDesignation => {
                        // revert the identifier from its contextual underscore back to an identifier.
                        let discard =
                            self.arena_ref().slot(designation, nt::DiscardDesignationSyntax::UNDERSCORE_TOKEN);
                        debug_assert!(self.arena_ref().kind(discard) == SyntaxKind::UnderscoreToken);
                        // `SyntaxToken.WithValue(SyntaxKind.IdentifierToken, discard.LeadingTrivia.Node,
                        // discard.Text, discard.ValueText, discard.TrailingTrivia.Node)`: a new token, no diagnostics
                        let source = self.source();
                        let a = self.arena();
                        let text = String::from_utf8_lossy(a.token_text(discard, source)).into_owned();
                        let value = String::from_utf8_lossy(a.token_value_text(discard, source)).into_owned();
                        let (start, len) = a.intern(&text);
                        let leading = a.leading_trivia(discard);
                        let trailing = a.trailing_trivia(discard);
                        let token = a.create_token(
                            leading,
                            SyntaxKind::IdentifierToken,
                            SyntaxKind::IdentifierToken,
                            TokenText::Owned { start, len },
                            trailing,
                        );
                        if value != text {
                            a.set_token_value(token, &value);
                        }
                        token
                    }
                    // `throw ExceptionUtilities.UnexpectedValue(decl.designation.Kind)`: a designation is one of the
                    // three; anything else keeps the variable form.
                    _ => Green::NULL,
                };

                if !identifier.is_null() {
                    let type_ = self.arena_ref().slot(decl, nt::DeclarationExpressionSyntax::TYPE);
                    return self.arena().for_each_statement(
                        attributes,
                        await_token_opt,
                        foreach,
                        open_paren,
                        type_,
                        identifier,
                        in_,
                        expression,
                        close_paren,
                        statement,
                    );
                }
            }
        }

        self.arena().for_each_variable_statement(
            attributes,
            await_token_opt,
            foreach,
            open_paren,
            variable,
            in_,
            expression,
            close_paren,
            statement,
        )
    }

    /// `private ExpressionSyntax ParseExpressionOrDeclaration(ParseTypeMode mode, bool permitTupleDesignation)` (LanguageParser.cs:9889).
    ///
    /// Parse an expression where a declaration expression would be permitted. This is suitable for use after
    /// the `out` keyword in an argument list, or in the elements of a tuple literal (because they may
    /// be on the left-hand-side of a positional subpattern). The first element of a tuple is handled slightly
    /// differently, as we check for the comma before concluding that the identifier should cause a
    /// disambiguation. For example, for the input `(A < B , C > D)`, we treat this as a tuple with
    /// two elements, because if we considered the `A<B,C>` to be a type, it wouldn't be a tuple at
    /// all. Since we don't have such a thing as a one-element tuple (even for positional subpattern), the
    /// absence of the comma after the `D` means we don't treat the `D` as contributing to the
    /// disambiguation of the expression/type. See also ScanTypeArgumentList where these disambiguation rules are
    /// encoded.
    pub(crate) fn parse_expression_or_declaration(
        &mut self,
        mode: ParseTypeMode,
        permit_tuple_designation: bool,
    ) -> Green {
        let mut is_scoped = false;
        if self.is_possible_declaration_expression(mode, permit_tuple_designation, &mut is_scoped) {
            self.parse_declaration_expression(mode, is_scoped)
        } else {
            self.parse_sub_expression(Precedence::EXPRESSION)
        }
    }

    /// `private bool IsPossibleDeclarationExpression(ParseTypeMode mode, bool permitTupleDesignation, out bool isScoped)` (LanguageParser.cs:9896).
    pub(crate) fn is_possible_declaration_expression(
        &mut self,
        mode: ParseTypeMode,
        permit_tuple_designation: bool,
        is_scoped: &mut bool,
    ) -> bool {
        debug_assert!(matches!(
            mode,
            ParseTypeMode::Normal | ParseTypeMode::FirstElementOfPossibleTupleLiteral | ParseTypeMode::AfterTupleComma
        ));
        *is_scoped = false;

        if self.is_in_async && self.current_contextual_kind() == SyntaxKind::AwaitKeyword {
            // can't be a declaration expression.
            return false;
        }

        let mut reset_point = self.get_disposable_reset_point(true);

        let result = 'result: {
            if self.current_contextual_kind() == SyntaxKind::ScopedKeyword {
                self.eat_token();
                if self.scan_type(false) != ScanTypeFlags::NOT_TYPE
                    && self.current_kind() == SyntaxKind::IdentifierToken
                {
                    match mode {
                        ParseTypeMode::FirstElementOfPossibleTupleLiteral => {
                            if self.peek_kind(1) == SyntaxKind::CommaToken {
                                *is_scoped = true;
                                break 'result true;
                            }
                        }

                        ParseTypeMode::AfterTupleComma => {
                            if matches!(self.peek_kind(1), SyntaxKind::CommaToken | SyntaxKind::CloseParenToken) {
                                *is_scoped = true;
                                break 'result true;
                            }
                        }

                        _ => {
                            // The other case where we disambiguate between a declaration and expression is before the `in` of a foreach loop.
                            // There we err on the side of accepting a declaration.
                            *is_scoped = true;
                            break 'result true;
                        }
                    }
                }

                self.reset_disposable(&mut reset_point);
            }

            let type_is_var = self.is_var_type();
            let mut last_token_of_type = Green::NULL;
            if self.scan_type_with_last_token_of_type(mode, &mut last_token_of_type) == ScanTypeFlags::NOT_TYPE {
                break 'result false;
            }

            // check for a designation
            let last_kind = self.arena_ref().kind(last_token_of_type);
            if !self.scan_designation(permit_tuple_designation && (type_is_var || Self::is_predefined_type(last_kind)))
            {
                break 'result false;
            }

            match mode {
                ParseTypeMode::FirstElementOfPossibleTupleLiteral => self.current_kind() == SyntaxKind::CommaToken,
                ParseTypeMode::AfterTupleComma => {
                    matches!(self.current_kind(), SyntaxKind::CommaToken | SyntaxKind::CloseParenToken)
                }
                // The other case where we disambiguate between a declaration and expression is before the `in` of a foreach loop.
                // There we err on the side of accepting a declaration.
                _ => true,
            }
        };

        self.dispose_reset_point(reset_point);
        result
    }

    /// `private bool IsVarType()` (LanguageParser.cs:9972).
    ///
    /// Is the following set of tokens, interpreted as a type, the type `var`?
    pub(crate) fn is_var_type(&mut self) -> bool {
        let current = self.current_token();
        if !is_identifier_var(self.arena_ref(), current) {
            return false;
        }

        !matches!(
            self.peek_kind(1),
            SyntaxKind::DotToken
                | SyntaxKind::ColonColonToken
                | SyntaxKind::OpenBracketToken
                | SyntaxKind::AsteriskToken
                | SyntaxKind::QuestionToken
                | SyntaxKind::LessThanToken
        )
    }

    /// `private static bool IsValidForeachVariable(ExpressionSyntax variable)` (LanguageParser.cs:9993).
    pub(crate) fn is_valid_foreach_variable(&self, variable: Green) -> bool {
        let a = self.arena_ref();
        match a.kind(variable) {
            // e.g. `foreach (var (x, y) in e)`
            SyntaxKind::DeclarationExpression => true,
            // e.g. `foreach ((var x, var y) in e)`
            SyntaxKind::TupleExpression => true,
            // e.g. `foreach (_ in e)`
            SyntaxKind::IdentifierName => {
                a.contextual_kind(a.slot(variable, nt::IdentifierNameSyntax::IDENTIFIER)) == SyntaxKind::UnderscoreToken
            }
            _ => false,
        }
    }

    /// `private GotoStatementSyntax ParseGotoStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10011).
    pub(crate) fn parse_goto_statement(&mut self, attributes: Green) -> Green {
        debug_assert!(self.current_kind() == SyntaxKind::GotoKeyword);

        let goto = self.eat_token_kind(SyntaxKind::GotoKeyword);

        let mut case_or_default = Green::NULL;
        let mut arg = Green::NULL;
        let kind;

        if matches!(self.current_kind(), SyntaxKind::CaseKeyword | SyntaxKind::DefaultKeyword) {
            case_or_default = self.eat_token();
            if self.arena_ref().kind(case_or_default) == SyntaxKind::CaseKeyword {
                kind = SyntaxKind::GotoCaseStatement;
                arg = self.parse_expression_core();
            } else {
                kind = SyntaxKind::GotoDefaultStatement;
            }
        } else {
            kind = SyntaxKind::GotoStatement;
            arg = self.parse_identifier_name(ErrorCode::ERR_IdentifierExpected);
        }

        let semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
        self.arena().goto_statement(kind, attributes, goto, case_or_default, arg, semicolon)
    }

    /// `private ExpressionSyntax ParseExpressionForParenthesizedConstruct()` (LanguageParser.cs:10044).
    pub(crate) fn parse_expression_for_parenthesized_construct(&mut self) -> Green {
        let expression = self.parse_expression_core();
        self.parse_errant_expression_when_no_close_paren_token(expression)
    }

    /// `private ExpressionSyntax ParseErrantExpressionWhenNoCloseParenToken(ExpressionSyntax expression)` (LanguageParser.cs:10047).
    pub(crate) fn parse_errant_expression_when_no_close_paren_token(&mut self, expression: Green) -> Green {
        let mut expression = expression;
        if self.current_kind() != SyntaxKind::CloseParenToken {
            // Look for common case of `(a b)` where the user is in the middle of updating the expression to
            // something like `(a && b)`.  We want to recover well here as otherwise the subsequent close paren will
            // cause problems for higher level constructs.
            //
            // Note if we see `(a b) =>` we don't do this as it's much more likely this is some incomplete lambda
            // expression.
            let mut reset_point = self.get_disposable_reset_point(false);
            let next_expression = self.parse_expression_core();

            let last_token = self.arena_ref().last_token(next_expression);
            if self.current_kind() == SyntaxKind::CloseParenToken
                && self.peek_kind(1) != SyntaxKind::EqualsGreaterThanToken
                && !self.arena_ref().is_missing(last_token)
            {
                let first_token = self.arena_ref().first_token(next_expression);
                let skipped = self.add_error_to_first_token(
                    next_expression,
                    ErrorCode::ERR_UnexpectedToken,
                    &[DiagArg::TokenText(first_token)],
                );
                expression = self.add_trailing_skipped_syntax(expression, skipped);
            } else {
                self.reset_disposable(&mut reset_point);
            }
            self.dispose_reset_point(reset_point);
        }

        expression
    }

    /// `private IfStatementSyntax ParseIfStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10077).
    pub(crate) fn parse_if_statement(&mut self, attributes: Green) -> Green {
        debug_assert!(self.current_kind() == SyntaxKind::IfKeyword);

        // `ArrayBuilder<(SyntaxToken, SyntaxToken, ExpressionSyntax, SyntaxToken, StatementSyntax, SyntaxToken)>`:
        // six elements an entry, in a pooled builder.
        let mut stack = self.pool.allocate();

        let mut alternative;
        loop {
            let if_keyword = self.eat_token_kind(SyntaxKind::IfKeyword);
            let open_paren = self.eat_token_kind(SyntaxKind::OpenParenToken);
            let condition = self.parse_expression_for_parenthesized_construct();
            let close_paren = self.eat_token_kind(SyntaxKind::CloseParenToken);
            let consequence = self.parse_embedded_statement();

            let else_keyword = if self.current_kind() != SyntaxKind::ElseKeyword {
                Green::NULL
            } else {
                self.eat_token_kind(SyntaxKind::ElseKeyword)
            };
            stack.extend_from_slice(&[if_keyword, open_paren, condition, close_paren, consequence, else_keyword]);

            if else_keyword.is_null() {
                alternative = Green::NULL;
                break;
            }

            if self.current_kind() != SyntaxKind::IfKeyword {
                alternative = self.parse_embedded_statement();
                break;
            }

            alternative = self.try_reuse_statement(Green::NULL, false);
            if !alternative.is_null() {
                break;
            }
        }

        let mut if_statement;
        loop {
            let else_keyword = stack.pop().unwrap_or_default();
            let consequence = stack.pop().unwrap_or_default();
            let close_paren = stack.pop().unwrap_or_default();
            let condition = stack.pop().unwrap_or_default();
            let open_paren = stack.pop().unwrap_or_default();
            let if_keyword = stack.pop().unwrap_or_default();
            let a = self.base.arena();
            let else_clause =
                if alternative.is_null() { Green::NULL } else { a.else_clause(else_keyword, alternative) };
            if_statement = a.if_statement(
                if !stack.is_empty() { Green::NULL } else { attributes },
                if_keyword,
                open_paren,
                condition,
                close_paren,
                consequence,
                else_clause,
            );
            alternative = if_statement;
            if stack.is_empty() {
                break;
            }
        }

        self.pool.free(stack);

        if_statement
    }

    /// `private IfStatementSyntax ParseMisplacedElse(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10142).
    pub(crate) fn parse_misplaced_else(&mut self, attributes: Green) -> Green {
        debug_assert!(self.current_kind() == SyntaxKind::ElseKeyword);

        let if_keyword = self.eat_token_code(SyntaxKind::IfKeyword, ErrorCode::ERR_ElseCannotStartStatement, true);
        let open_paren = self.eat_token_kind(SyntaxKind::OpenParenToken);
        let condition = self.parse_expression_core();
        let close_paren = self.eat_token_kind(SyntaxKind::CloseParenToken);
        let statement = self.parse_expression_statement(Green::NULL);
        let else_ = self.parse_else_clause_opt();
        self.arena().if_statement(attributes, if_keyword, open_paren, condition, close_paren, statement, else_)
    }

    /// `private ElseClauseSyntax ParseElseClauseOpt()` (LanguageParser.cs:10156).
    pub(crate) fn parse_else_clause_opt(&mut self) -> Green {
        if self.current_kind() != SyntaxKind::ElseKeyword {
            return Green::NULL;
        }
        let else_keyword = self.eat_token_kind(SyntaxKind::ElseKeyword);
        let statement = self.parse_embedded_statement();
        self.arena().else_clause(else_keyword, statement)
    }

    /// `private LockStatementSyntax ParseLockStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10165).
    pub(crate) fn parse_lock_statement(&mut self, attributes: Green) -> Green {
        debug_assert!(self.current_kind() == SyntaxKind::LockKeyword);
        let lock_keyword = self.eat_token_kind(SyntaxKind::LockKeyword);
        let open_paren = self.eat_token_kind(SyntaxKind::OpenParenToken);
        let expression = self.parse_expression_for_parenthesized_construct();
        let close_paren = self.eat_token_kind(SyntaxKind::CloseParenToken);
        let statement = self.parse_embedded_statement();
        self.arena().lock_statement(attributes, lock_keyword, open_paren, expression, close_paren, statement)
    }

    /// `private ReturnStatementSyntax ParseReturnStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10177).
    pub(crate) fn parse_return_statement(&mut self, attributes: Green) -> Green {
        debug_assert!(self.current_kind() == SyntaxKind::ReturnKeyword);
        let return_keyword = self.eat_token_kind(SyntaxKind::ReturnKeyword);
        let expression = if self.current_kind() != SyntaxKind::SemicolonToken {
            self.parse_possible_ref_expression()
        } else {
            Green::NULL
        };
        let semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
        self.arena().return_statement(attributes, return_keyword, expression, semicolon)
    }

    /// `private YieldStatementSyntax ParseYieldStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10187).
    pub(crate) fn parse_yield_statement(&mut self, attributes: Green) -> Green {
        debug_assert!(self.current_contextual_kind() == SyntaxKind::YieldKeyword);

        let yield_token = self.eat_token();
        let yield_token = self.convert_to_keyword(yield_token);
        let return_or_break;
        let mut arg = Green::NULL;
        let kind;

        if self.current_kind() == SyntaxKind::BreakKeyword {
            kind = SyntaxKind::YieldBreakStatement;
            return_or_break = self.eat_token();
        } else {
            kind = SyntaxKind::YieldReturnStatement;
            let token = self.eat_token_kind(SyntaxKind::ReturnKeyword);
            if self.current_kind() == SyntaxKind::SemicolonToken {
                return_or_break = self.add_error(token, ErrorCode::ERR_EmptyYield);
            } else {
                return_or_break = token;
                arg = self.parse_expression_core();
            }
        }

        let semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
        self.arena().yield_statement(kind, attributes, yield_token, return_or_break, arg, semicolon)
    }

    /// `private SwitchStatementSyntax ParseSwitchStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10224).
    pub(crate) fn parse_switch_statement(&mut self, attributes: Green) -> Green {
        debug_assert!(matches!(self.current_kind(), SyntaxKind::SwitchKeyword | SyntaxKind::CaseKeyword));

        // `void parseSwitchHeader(out SyntaxToken switchKeyword, out SyntaxToken openParen, out ExpressionSyntax
        // expression, out SyntaxToken closeParen, out SyntaxToken openBrace)`
        let switch_keyword;
        let open_paren;
        let expression;
        let close_paren;
        let open_brace;
        if self.current_kind() == SyntaxKind::CaseKeyword {
            // try to eat a 'switch' so the user gets a good error message about what's wrong. then directly
            // creating missing tokens for the rest so they don't get cascading errors.
            switch_keyword = self.eat_token_kind(SyntaxKind::SwitchKeyword);
            open_paren = self.arena().missing_token(SyntaxKind::OpenParenToken);
            expression = self.create_missing_identifier_name();
            close_paren = self.arena().missing_token(SyntaxKind::CloseParenToken);
            open_brace = self.arena().missing_token(SyntaxKind::OpenBraceToken);
        } else {
            switch_keyword = self.eat_token_kind(SyntaxKind::SwitchKeyword);
            let parsed = self.parse_expression_core();
            let kind = self.arena_ref().kind(parsed);
            if kind == SyntaxKind::ParenthesizedExpression {
                let a = self.arena_ref();
                open_paren = a.slot(parsed, nt::ParenthesizedExpressionSyntax::OPEN_PAREN_TOKEN);
                expression = a.slot(parsed, nt::ParenthesizedExpressionSyntax::EXPRESSION);
                close_paren = a.slot(parsed, nt::ParenthesizedExpressionSyntax::CLOSE_PAREN_TOKEN);

                debug_assert!(a.diagnostics(parsed).len() == 0);
            } else if kind == SyntaxKind::TupleExpression {
                // As a special case, when a tuple literal is the governing expression of
                // a switch statement we permit the switch statement's own parentheses to be omitted.
                // LDM 2018-04-04.
                open_paren = Green::NULL;
                close_paren = Green::NULL;
                expression = parsed;
            } else {
                // Some other expression has appeared without parens. Give a syntax error.
                open_paren = self.arena().missing_token(SyntaxKind::OpenParenToken);
                expression = self.add_error(parsed, ErrorCode::ERR_SwitchGoverningExpressionRequiresParens);
                close_paren = self.arena().missing_token(SyntaxKind::CloseParenToken);
            }

            open_brace = self.eat_token_kind(SyntaxKind::OpenBraceToken);
        }

        let mut sections = self.pool.allocate();

        while self.is_possible_switch_section() {
            let section = self.parse_switch_section();
            sections.push(section);
        }

        let sections_list = self.pool.to_list_and_free(self.base.arena(), sections);
        let close_brace = self.eat_token_kind(SyntaxKind::CloseBraceToken);
        self.arena().switch_statement(
            attributes,
            switch_keyword,
            open_paren,
            expression,
            close_paren,
            open_brace,
            sections_list,
            close_brace,
        )
    }

    /// `private bool IsPossibleSwitchSection()` (LanguageParser.cs:10294).
    pub(crate) fn is_possible_switch_section(&mut self) -> bool {
        self.current_kind() == SyntaxKind::CaseKeyword
            || (self.current_kind() == SyntaxKind::DefaultKeyword && self.peek_kind(1) != SyntaxKind::OpenParenToken)
    }

    /// `private SwitchSectionSyntax ParseSwitchSection()` (LanguageParser.cs:10300).
    pub(crate) fn parse_switch_section(&mut self) -> Green {
        debug_assert!(self.is_possible_switch_section());

        // First, parse case label(s)
        let mut labels = self.pool.allocate();
        let mut statements = self.pool.allocate();

        loop {
            let label;
            if self.current_kind() == SyntaxKind::CaseKeyword {
                let case_keyword = self.eat_token();

                if self.current_kind() == SyntaxKind::ColonToken {
                    let value = self.parse_identifier_name(ErrorCode::ERR_ConstantExpected);
                    let colon = self.eat_token_kind(SyntaxKind::ColonToken);
                    label = self.arena().case_switch_label(case_keyword, value, colon);
                } else {
                    let mut node = self.parse_expression_or_pattern_for_switch_statement();

                    // if there is a 'when' token, we treat a case expression as a constant pattern.
                    if self.current_contextual_kind() == SyntaxKind::WhenKeyword
                        && is_expression(self.arena_ref(), node)
                    {
                        node = self.arena().constant_pattern(node);
                    }

                    if self.arena_ref().kind(node) == SyntaxKind::DiscardPattern {
                        node = self.add_error(node, ErrorCode::ERR_DiscardPatternInSwitchStatement);
                    }

                    if is_pattern(self.arena_ref(), node) {
                        let when_clause = self.parse_when_clause(Precedence::EXPRESSION);
                        let colon = self.eat_token_kind(SyntaxKind::ColonToken);
                        label = self.arena().case_pattern_switch_label(case_keyword, node, when_clause, colon);
                    } else {
                        let colon = self.eat_token_kind(SyntaxKind::ColonToken);
                        label = self.arena().case_switch_label(case_keyword, node, colon);
                    }
                }
            } else {
                debug_assert!(self.current_kind() == SyntaxKind::DefaultKeyword);
                let default_keyword = self.eat_token_kind(SyntaxKind::DefaultKeyword);
                let colon = self.eat_token_kind(SyntaxKind::ColonToken);
                label = self.arena().default_switch_label(default_keyword, colon);
            }

            labels.push(label);
            if !self.is_possible_switch_section() {
                break;
            }
        }

        // Next, parse statement list stopping for new sections
        let mut tmp = labels.last().copied().unwrap_or_default();
        self.parse_statements(&mut tmp, &mut statements, true);
        if let Some(last) = labels.last_mut() {
            *last = tmp;
        }

        let labels_list = self.pool.to_list_and_free(self.base.arena(), labels);
        let statements_list = self.pool.to_list_and_free(self.base.arena(), statements);
        self.arena().switch_section(labels_list, statements_list)
    }

    /// `private ThrowStatementSyntax ParseThrowStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10372).
    pub(crate) fn parse_throw_statement(&mut self, attributes: Green) -> Green {
        debug_assert!(self.current_kind() == SyntaxKind::ThrowKeyword);
        let throw_keyword = self.eat_token_kind(SyntaxKind::ThrowKeyword);
        let expression =
            if self.current_kind() != SyntaxKind::SemicolonToken { self.parse_expression_core() } else { Green::NULL };
        let semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
        self.arena().throw_statement(attributes, throw_keyword, expression, semicolon)
    }

    /// `private UnsafeStatementSyntax ParseUnsafeStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10382).
    pub(crate) fn parse_unsafe_statement(&mut self, attributes: Green) -> Green {
        debug_assert!(self.current_kind() == SyntaxKind::UnsafeKeyword);
        let unsafe_keyword = self.eat_token_kind(SyntaxKind::UnsafeKeyword);
        let block = self.parse_possibly_attributed_block();
        self.arena().unsafe_statement(attributes, unsafe_keyword, block)
    }

    /// `private UsingStatementSyntax ParseUsingStatement(SyntaxList<AttributeListSyntax> attributes, SyntaxToken awaitTokenOpt = null)` (LanguageParser.cs:10391).
    pub(crate) fn parse_using_statement(&mut self, attributes: Green, await_token_opt: Green) -> Green {
        let using = self.eat_token_kind(SyntaxKind::UsingKeyword);
        let open_paren = self.eat_token_kind(SyntaxKind::OpenParenToken);

        let mut declaration = Green::NULL;
        let mut expression = Green::NULL;

        let mut reset_point = self.get_reset_point();
        self.parse_using_expression(&mut declaration, &mut expression, &mut reset_point);
        self.release(&mut reset_point);

        let close_paren = self.eat_token_kind(SyntaxKind::CloseParenToken);
        let statement = self.parse_embedded_statement();
        self.arena().using_statement(
            attributes,
            await_token_opt,
            using,
            open_paren,
            declaration,
            expression,
            close_paren,
            statement,
        )
    }

    /// `private void ParseUsingExpression(ref VariableDeclarationSyntax declaration, ref ExpressionSyntax expression, ref ResetPoint resetPoint)` (LanguageParser.cs:10414).
    pub(crate) fn parse_using_expression(
        &mut self,
        declaration: &mut Green,
        expression: &mut Green,
        reset_point: &mut ResetPoint,
    ) {
        if self.is_await_expression() {
            *expression = self.parse_expression_core();
            return;
        }

        // Now, this can be either an expression or a decl list

        let st;

        if self.is_query_expression(true, false) {
            st = ScanTypeFlags::NOT_TYPE;
        } else {
            let scoped_keyword = self.parse_possible_scoped_keyword(false, false);

            if !scoped_keyword.is_null() {
                *declaration = self.parse_parenthesized_variable_declaration(VariableFlags::NONE, scoped_keyword);
                return;
            } else {
                st = self.scan_type(false);
            }
        }

        if st == ScanTypeFlags::NULLABLE_TYPE {
            // We need to handle:
            // * using (f ? x = a : x = b)
            // * using (f ? x = a)
            // * using (f ? x, y)

            if self.current_kind() != SyntaxKind::IdentifierToken {
                self.reset(reset_point);
                *expression = self.parse_expression_core();
            } else {
                match self.peek_kind(1) {
                    SyntaxKind::CommaToken | SyntaxKind::CloseParenToken => {
                        self.reset(reset_point);
                        *declaration = self.parse_parenthesized_variable_declaration(VariableFlags::NONE, Green::NULL);
                    }

                    SyntaxKind::EqualsToken => {
                        // Parse it as a decl. If the next token is a : and only one variable was parsed,
                        // convert the whole thing to ?: expression.
                        self.reset(reset_point);
                        *declaration = self.parse_parenthesized_variable_declaration(VariableFlags::NONE, Green::NULL);

                        // We may have non-nullable types in error scenarios.
                        let a = self.arena_ref();
                        let type_ = a.slot(*declaration, nt::VariableDeclarationSyntax::TYPE);
                        let variables = a.slot(*declaration, nt::VariableDeclarationSyntax::VARIABLES);
                        if self.current_kind() == SyntaxKind::ColonToken && {
                            let a = self.arena_ref();
                            a.kind(type_) == SyntaxKind::NullableType
                                && facts::is_name(a.kind(a.slot(type_, nt::NullableTypeSyntax::ELEMENT_TYPE)))
                                && separated_count(a, variables) == 1
                        } {
                            // We have "name? id = expr :" so need to convert to a ?: expression.
                            self.reset(reset_point);
                            *declaration = Green::NULL;
                            *expression = self.parse_expression_core();
                        }
                    }

                    _ => {
                        self.reset(reset_point);
                        *expression = self.parse_expression_core();
                    }
                }
            }
        } else if self.is_using_statement_variable_declaration(st) {
            self.reset(reset_point);
            *declaration = self.parse_parenthesized_variable_declaration(VariableFlags::NONE, Green::NULL);
        } else {
            // Must be an expression statement
            self.reset(reset_point);
            *expression = self.parse_expression_core();
        }
    }

    /// `private bool IsUsingStatementVariableDeclaration(ScanTypeFlags st)` (LanguageParser.cs:10507).
    pub(crate) fn is_using_statement_variable_declaration(&mut self, st: ScanTypeFlags) -> bool {
        debug_assert!(st != ScanTypeFlags::NULLABLE_TYPE);

        let condition1 = st == ScanTypeFlags::MUST_BE_TYPE && self.current_kind() != SyntaxKind::DotToken;
        let condition2 = st != ScanTypeFlags::NOT_TYPE && self.current_kind() == SyntaxKind::IdentifierToken;
        let condition3 =
            st == ScanTypeFlags::NON_GENERIC_TYPE_OR_EXPRESSION || self.peek_kind(1) == SyntaxKind::EqualsToken;

        condition1 || (condition2 && condition3)
    }

    /// `private WhileStatementSyntax ParseWhileStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10518).
    pub(crate) fn parse_while_statement(&mut self, attributes: Green) -> Green {
        debug_assert!(self.current_kind() == SyntaxKind::WhileKeyword);
        let while_keyword = self.eat_token_kind(SyntaxKind::WhileKeyword);
        let open_paren = self.eat_token_kind(SyntaxKind::OpenParenToken);
        let condition = self.parse_expression_for_parenthesized_construct();
        let close_paren = self.eat_token_kind(SyntaxKind::CloseParenToken);
        let statement = self.parse_embedded_statement();
        self.arena().while_statement(attributes, while_keyword, open_paren, condition, close_paren, statement)
    }

    /// `private LabeledStatementSyntax ParseLabeledStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10530).
    pub(crate) fn parse_labeled_statement(&mut self, attributes: Green) -> Green {
        // We have an identifier followed by a colon. But if the identifier is a contextual keyword in a query context,
        // ParseIdentifier will result in a missing name and Eat(Colon) will fail. We won't make forward progress.
        debug_assert!(self.is_true_identifier() && self.peek_kind(1) == SyntaxKind::ColonToken);

        let identifier = self.parse_identifier_token(ErrorCode::ERR_IdentifierExpected);
        let colon = self.eat_token_kind(SyntaxKind::ColonToken);
        let mut statement = self.parse_possibly_attributed_statement();
        if statement.is_null() {
            let semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
            statement = self.arena().empty_statement(Green::NULL, semicolon);
        }
        self.arena().labeled_statement(attributes, identifier, colon, statement)
    }

    /// `private StatementSyntax ParseLocalDeclarationStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:10546).
    ///
    /// Parses any kind of local declaration statement: local variable or local function.
    pub(crate) fn parse_local_declaration_statement(&mut self, attributes: Green) -> Green {
        let await_keyword;
        let using_keyword;
        let mut can_parse_as_local_function = false;
        if self.is_possible_await_using() {
            await_keyword = self.eat_contextual_token(SyntaxKind::AwaitKeyword);
            using_keyword = self.eat_token();
        } else if self.current_kind() == SyntaxKind::UsingKeyword {
            await_keyword = Green::NULL;
            using_keyword = self.eat_token();
        } else {
            await_keyword = Green::NULL;
            using_keyword = Green::NULL;
            can_parse_as_local_function = true;
        }

        let mut mods = self.pool.allocate();
        self.parse_local_declaration_statement_modifiers(&mut mods, !using_keyword.is_null());

        let mut variables = self.pool.allocate();
        // try
        let result = 'result: {
            let scoped_keyword = self.parse_possible_scoped_keyword(false, false);

            // For local functions, 'scoped' is a modifier in LocalFunctionStatementSyntax
            if !scoped_keyword.is_null() {
                mods.push(scoped_keyword);
            }

            let mut type_ = Green::NULL;
            let mut local_function = Green::NULL;
            let mods_list = self.base.arena().list(&mods);
            self.parse_local_declaration(
                &mut variables,
                can_parse_as_local_function,
                // A local declaration doesn't have a `(...)` construct.  So no need to stop if we hit a close paren
                // after a declarator.  Let normal error recovery kick in.
                false,
                attributes,
                mods_list,
                Green::NULL,
                VariableFlags::NONE,
                &mut type_,
                &mut local_function,
            );

            if !local_function.is_null() {
                debug_assert!(variables.is_empty());
                break 'result local_function;
            }

            if can_parse_as_local_function {
                // If we find an accessibility modifier but no local function it's likely
                // the user forgot a closing brace. Let's back out of statement parsing.
                // We check just for a leading accessibility modifier in the syntax because
                // SkipBadStatementListTokens will not skip attribute lists.
                if list_count(self.arena_ref(), attributes) == 0
                    && let Some(&first) = mods.first()
                    && Self::is_accessibility_modifier(self.arena_ref().contextual_kind(first))
                {
                    break 'result Green::NULL;
                }
            }

            // For locals, 'scoped' is part of ScopedTypeSyntax.
            if !scoped_keyword.is_null() {
                mods.pop();
                type_ = self.arena().scoped_type(scoped_keyword, type_);
            }

            // We've already reported all modifiers for local_using_declaration as errors
            if using_keyword.is_null() {
                for slot in mods.iter_mut() {
                    let modifier = *slot;

                    if Self::is_additional_local_function_modifier(self.arena_ref().contextual_kind(modifier)) {
                        *slot = self.add_error_args(
                            modifier,
                            ErrorCode::ERR_BadMemberFlag,
                            &[DiagArg::TokenText(modifier)],
                        );
                    }
                }
            }

            let a = self.base.arena();
            let mods_list = a.list(&mods);
            let variables_list = a.list(&variables);
            let declaration = a.variable_declaration(type_, variables_list);
            let semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
            self.arena().local_declaration_statement(
                attributes,
                await_keyword,
                using_keyword,
                mods_list,
                declaration,
                semicolon,
            )
        };
        // finally
        self.pool.free(variables);
        self.pool.free(mods);
        result
    }

    /// `private bool IsDefiniteScopedModifier( bool isFunctionPointerParameter, bool isLambdaParameter)` (LanguageParser.cs:10647).
    pub(crate) fn is_definite_scoped_modifier(
        &mut self,
        is_function_pointer_parameter: bool,
        is_lambda_parameter: bool,
    ) -> bool {
        if self.current_contextual_kind() != SyntaxKind::ScopedKeyword {
            return false;
        }

        // In C# 14 we decided that within a lambda 'scoped' would *always* be a modifier, not a type.
        // so `scoped scoped` is `modifier-scoped identifier-scoped` not `type-scoped identifier-scoped`.
        // Note: this only applies the modifier/type portion.  We still allow the identifier of a lambda
        // to be named 'scoped'.
        if is_lambda_parameter && self.is_feature_enabled(MessageID::IDS_FeatureSimpleLambdaParameterModifiers) {
            return true;
        }

        let before_scoped_reset_point = self.get_disposable_reset_point(true);

        let _scoped_keyword = self.eat_contextual_token(SyntaxKind::ScopedKeyword);

        // trivial case.  scoped ref/out/in/this  is definitely the scoped keyword. Note: the only actual legal
        // cases are `scoped ref`, `scoped out`, and `scoped in`.  But we detect and allow `scoped this`, `scoped
        // params` and `scoped readonly` as well.  These will be reported as errors later in binding.
        let current = self.current_token();
        let result = if self.is_parameter_modifier_excluding_scoped(current) {
            true
        } else {
            // More complex cases.  We have to check for `scoped Type ...` now.
            //
            // Note that `scoped scoped` can be valid here as a type called scoped and a variable called scoped.
            self.scan_type(false) != ScanTypeFlags::NOT_TYPE && {
                // `bool isValidScopedTypeCase()`: had `scoped Type ...`
                //
                // 1. This is a function pointer `delegate<T1, scoped T2>`
                // 2. this is a parameter `scoped T x`.
                if is_function_pointer_parameter {
                    matches!(self.current_kind(), SyntaxKind::CommaToken | SyntaxKind::GreaterThanToken)
                } else {
                    self.current_kind() == SyntaxKind::IdentifierToken
                }
            }
        };

        self.dispose_reset_point(before_scoped_reset_point);
        result
    }

    /// `private SyntaxToken ParsePossibleScopedKeyword( bool isFunctionPointerParameter, bool isLambdaParameter)` (LanguageParser.cs:10696).
    pub(crate) fn parse_possible_scoped_keyword(
        &mut self,
        is_function_pointer_parameter: bool,
        is_lambda_parameter: bool,
    ) -> Green {
        if self.is_definite_scoped_modifier(is_function_pointer_parameter, is_lambda_parameter) {
            self.eat_contextual_token(SyntaxKind::ScopedKeyword)
        } else {
            Green::NULL
        }
    }

    /// `private VariableDesignationSyntax ParseDesignation(bool forPattern)` (LanguageParser.cs:10705).
    ///
    /// Not in Roslyn: the nested parentheses count `_recursionDepth` (see the module's documentation).
    pub(crate) fn parse_designation(&mut self, for_pattern: bool) -> Green {
        // the two forms of designation are
        // (1) identifier
        // (2) ( designation ... )
        // for pattern-matching, we permit the designation list to be empty
        let result;
        if self.current_kind() == SyntaxKind::OpenParenToken {
            self.recursion_depth += 1;
            self.ensure_sufficient_execution_stack();

            let open_paren = self.eat_token_kind(SyntaxKind::OpenParenToken);
            let mut list_of_designations = self.pool.allocate();

            let mut done = false;
            if for_pattern {
                done = self.current_kind() == SyntaxKind::CloseParenToken;
            } else {
                let designation = self.parse_designation(for_pattern);
                list_of_designations.push(designation);
                let comma = self.eat_token_kind(SyntaxKind::CommaToken);
                list_of_designations.push(comma);
            }

            if !done {
                loop {
                    let designation = self.parse_designation(for_pattern);
                    list_of_designations.push(designation);
                    if self.current_kind() == SyntaxKind::CommaToken {
                        let comma = self.eat_token_kind(SyntaxKind::CommaToken);
                        list_of_designations.push(comma);
                    } else {
                        break;
                    }
                }
            }

            let designations = self.pool.to_list_and_free(self.base.arena(), list_of_designations);
            let close_paren = self.eat_token_kind(SyntaxKind::CloseParenToken);
            result = self.arena().parenthesized_variable_designation(open_paren, designations, close_paren);

            self.recursion_depth -= 1;
        } else {
            result = self.parse_simple_designation();
        }

        result
    }

    /// `private VariableDesignationSyntax ParseSimpleDesignation()` (LanguageParser.cs:10761).
    ///
    /// Parse a single variable designation (e.g. `x`) or a wildcard designation (e.g. `_`)
    pub(crate) fn parse_simple_designation(&mut self) -> Green {
        if self.current_contextual_kind() == SyntaxKind::UnderscoreToken {
            let underscore = self.eat_contextual_token(SyntaxKind::UnderscoreToken);
            self.arena().discard_designation(underscore)
        } else {
            let identifier = self.eat_token_kind(SyntaxKind::IdentifierToken);
            self.arena().single_variable_designation(identifier)
        }
    }

    /// `private WhenClauseSyntax ParseWhenClause(Precedence precedence)` (LanguageParser.cs:10768).
    pub(crate) fn parse_when_clause(&mut self, precedence: Precedence) -> Green {
        if self.current_contextual_kind() != SyntaxKind::WhenKeyword {
            return Green::NULL;
        }

        let when_keyword = self.eat_contextual_token(SyntaxKind::WhenKeyword);
        let condition = self.parse_sub_expression(precedence);
        self.arena().when_clause(when_keyword, condition)
    }

    /// `private VariableDeclarationSyntax ParseParenthesizedVariableDeclaration( VariableFlags initialFlags, SyntaxToken? scopedKeyword)` (LanguageParser.cs:10786).
    ///
    /// Parse a local variable declaration for constructs where the variable declaration is enclosed in parentheses.
    /// Specifically, only for the `fixed (...)` `for(...)` or `using (...)` statements.
    pub(crate) fn parse_parenthesized_variable_declaration(
        &mut self,
        initial_flags: VariableFlags,
        scoped_keyword: Green,
    ) -> Green {
        let mut variables = self.pool.allocate();
        let mut type_ = Green::NULL;
        let mut local_function = Green::NULL;
        self.parse_local_declaration(
            &mut variables,
            false,
            // Always stop on a close paren as the parent `fixed(...)/for(...)/using(...)` statement wants to
            // consume it.
            true,
            Green::NULL,
            Green::NULL,
            scoped_keyword,
            initial_flags,
            &mut type_,
            &mut local_function,
        );
        debug_assert!(local_function.is_null());
        let variables_list = self.pool.to_list_and_free(self.base.arena(), variables);
        self.arena().variable_declaration(type_, variables_list)
    }

    /// `private void ParseLocalDeclaration( SeparatedSyntaxListBuilder<VariableDeclaratorSyntax> variables, bool allowLocalFunctions, bool stopOnCloseParen, SyntaxList<AttributeListSyntax> attributes, SyntaxList<SyntaxToken> mods, SyntaxToken? scopedKeyword, VariableFlags initialFlags, out TypeSyntax type, out LocalFunctionStatementSyntax localFunction)` (LanguageParser.cs:10808).
    pub(crate) fn parse_local_declaration(
        &mut self,
        variables: &mut SyntaxListBuilder,
        allow_local_functions: bool,
        stop_on_close_paren: bool,
        attributes: Green,
        mods: Green,
        scoped_keyword: Green,
        initial_flags: VariableFlags,
        type_: &mut Green,
        local_function: &mut Green,
    ) {
        *type_ = if allow_local_functions { self.parse_return_type() } else { self.parse_type(ParseTypeMode::Normal) };

        if !scoped_keyword.is_null() {
            *type_ = self.arena().scoped_type(scoped_keyword, *type_);
        }

        let mut flags = initial_flags | VariableFlags::LOCAL_OR_FIELD;
        if list_any_kind(self.arena_ref(), mods, SyntaxKind::ConstKeyword) {
            flags = flags | VariableFlags::CONST_;
        }

        let save_term = self.term_state;
        self.term_state = self.term_state | TerminatorState::IS_END_OF_DECLARATION_CLAUSE;
        self.parse_variable_declarators(
            *type_,
            flags,
            variables,
            true,
            allow_local_functions,
            stop_on_close_paren,
            attributes,
            mods,
            local_function,
        );
        self.term_state = save_term;

        if allow_local_functions && local_function.is_null() && {
            let a = self.arena_ref();
            a.kind(*type_) == SyntaxKind::PredefinedType
                && a.kind(a.slot(*type_, nt::PredefinedTypeSyntax::KEYWORD)) == SyntaxKind::VoidKeyword
        } {
            *type_ = self.add_error(*type_, ErrorCode::ERR_NoVoidHere);
        }
    }

    /// `private bool IsEndOfDeclarationClause()` (LanguageParser.cs:10852).
    pub(crate) fn is_end_of_declaration_clause(&mut self) -> bool {
        matches!(self.current_kind(), SyntaxKind::SemicolonToken | SyntaxKind::ColonToken)
    }

    /// `private void ParseLocalDeclarationStatementModifiers(SyntaxListBuilder list, bool isUsingDeclaration)` (LanguageParser.cs:10864).
    pub(crate) fn parse_local_declaration_statement_modifiers(
        &mut self,
        list: &mut SyntaxListBuilder,
        is_using_declaration: bool,
    ) {
        // `bool shouldTreatAsModifier()`
        fn should_treat_as_modifier(this: &mut LanguageParser<'_, '_>) -> bool {
            let reset_point = this.get_disposable_reset_point(true);

            debug_assert!(this.current_kind() == SyntaxKind::IdentifierToken);

            let result = loop {
                this.eat_token();

                if LanguageParser::is_declaration_modifier(this.current_kind())
                    || LanguageParser::is_additional_local_function_modifier(this.current_kind())
                {
                    break true;
                }

                let reset_point2 = this.get_disposable_reset_point(true);
                let is_typed_identifier = this.scan_type(false) != ScanTypeFlags::NOT_TYPE
                    && this.current_kind() == SyntaxKind::IdentifierToken;
                this.dispose_reset_point(reset_point2);
                if is_typed_identifier {
                    break true;
                }

                // If current token might be a contextual modifier we need to check ahead the next token after it
                // If the next token appears to be a modifier, we treat current token as a modifier as well
                // This allows to correctly parse things like local functions with several `async` modifiers
                if !LanguageParser::is_additional_local_function_modifier(this.current_contextual_kind()) {
                    break false;
                }
            };

            this.dispose_reset_point(reset_point);
            result
        }

        loop {
            let k = self.current_contextual_kind();
            if !(Self::is_declaration_modifier(k) || Self::is_additional_local_function_modifier(k)) {
                break;
            }
            let mut modifier;
            if matches!(k, SyntaxKind::AsyncKeyword | SyntaxKind::SafeKeyword) {
                // check for things like "async async()" where async is the type and/or the function name
                if !should_treat_as_modifier(self) {
                    break;
                }

                modifier = self.eat_contextual_token(k);
            } else {
                modifier = self.eat_token();
            }

            if is_using_declaration {
                modifier = self.add_error(modifier, ErrorCode::ERR_NoModifiersOnUsing);
            } else if matches!(k, SyntaxKind::ReadOnlyKeyword | SyntaxKind::VolatileKeyword) {
                modifier = self.add_error_args(modifier, ErrorCode::ERR_BadMemberFlag, &[DiagArg::TokenText(modifier)]);
            }

            // Note: Duplicate modifiers are not reported here during parsing.
            // They will be reported during binding (see Binder_Statements.BindDeclarationStatementParts).

            list.push(modifier);
        }
    }

    /// `private static bool IsDeclarationModifier(SyntaxKind kind)` (LanguageParser.cs:10932).
    pub(crate) fn is_declaration_modifier(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::ConstKeyword
                | SyntaxKind::StaticKeyword
                | SyntaxKind::ReadOnlyKeyword
                | SyntaxKind::VolatileKeyword
        )
    }

    /// `private static bool IsAdditionalLocalFunctionModifier(SyntaxKind kind)` (LanguageParser.cs:10946).
    pub(crate) fn is_additional_local_function_modifier(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::StaticKeyword
                | SyntaxKind::AsyncKeyword
                | SyntaxKind::UnsafeKeyword
                | SyntaxKind::SafeKeyword
                | SyntaxKind::ExternKeyword
                // Not a valid modifier, but we should parse to give a good
                // error message
                | SyntaxKind::PublicKeyword
                | SyntaxKind::InternalKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::PrivateKeyword
        )
    }

    /// `private static bool IsAccessibilityModifier(SyntaxKind kind)` (LanguageParser.cs:10968).
    pub(crate) fn is_accessibility_modifier(kind: SyntaxKind) -> bool {
        // Accessibility modifiers aren't legal in a local function,
        // but a common mistake. Parse to give a better error message.
        matches!(
            kind,
            SyntaxKind::PublicKeyword
                | SyntaxKind::InternalKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::PrivateKeyword
        )
    }

    /// `private LocalFunctionStatementSyntax TryParseLocalFunctionStatementBody( SyntaxList<AttributeListSyntax> attributes, SyntaxList<SyntaxToken> modifiers, TypeSyntax type, SyntaxToken identifier)` (LanguageParser.cs:10985).
    pub(crate) fn try_parse_local_function_statement_body(
        &mut self,
        attributes: Green,
        modifiers: Green,
        type_: Green,
        identifier: Green,
    ) -> Green {
        // This may potentially be an ambiguous parse until very far into the token stream, so we may have to backtrack.
        // For example, "await x()" is ambiguous at the current point of parsing (right now we're right after the x).
        // The point at which it becomes unambiguous is after the argument list. A "=>" or "{" means its a local function
        // (with return type @await), a ";" or other expression-y token means its an await of a function call.

        // Note that we could just check if we're in an async context, but that breaks some analyzers, because
        // "await f();" would be parsed as a local function statement when really we want a parse error so we can say
        // "did you mean to make this method be an async method?" (it's invalid either way, so the spec doesn't care)
        let mut reset_point = self.get_disposable_reset_point(false);

        // Indicates this must be parsed as a local function, even if there's no body
        let mut force_local_func = true;
        if self.arena_ref().kind(type_) == SyntaxKind::IdentifierName {
            let id = self.arena_ref().slot(type_, nt::IdentifierNameSyntax::IDENTIFIER);
            force_local_func = self.arena_ref().contextual_kind(id) != SyntaxKind::AwaitKeyword;
        }

        let mut modifiers = modifiers;
        let mut bad_builder: Option<SyntaxListBuilder> = None;
        let count = list_count(self.arena_ref(), modifiers);
        for i in 0..count {
            let mut modifier = list_item(self.arena_ref(), modifiers, i);
            match self.arena_ref().contextual_kind(modifier) {
                SyntaxKind::AsyncKeyword | SyntaxKind::UnsafeKeyword | SyntaxKind::SafeKeyword => {
                    force_local_func = true;
                    continue;
                }
                // already reported earlier, no need to report again
                SyntaxKind::ReadOnlyKeyword | SyntaxKind::VolatileKeyword => continue,
                SyntaxKind::StaticKeyword => continue,
                SyntaxKind::ExternKeyword => continue,
                _ => {
                    modifier =
                        self.add_error_args(modifier, ErrorCode::ERR_BadMemberFlag, &[DiagArg::TokenText(modifier)]);
                }
            }

            if bad_builder.is_none() {
                let mut builder = self.pool.allocate();
                for j in 0..count {
                    builder.push(list_item(self.arena_ref(), modifiers, j));
                }
                bad_builder = Some(builder);
            }

            if let Some(slot) = bad_builder.as_mut().and_then(|builder| builder.get_mut(i)) {
                *slot = modifier;
            }
        }

        if let Some(builder) = bad_builder {
            modifiers = self.pool.to_list_and_free(self.base.arena(), builder);
        }

        // using var _ = new ParserSyntaxContextResetter(this, isInAsyncContext: modifiers.Any((int)SyntaxKind.AsyncKeyword));
        let saved_context = save_syntax_context(self);
        self.is_in_async = list_any_kind(self.arena_ref(), modifiers, SyntaxKind::AsyncKeyword);

        let type_parameter_list_opt = self.parse_type_parameter_list();
        // "await f<T>()" still makes sense, so don't force accept a local function if there's a type parameter list.
        let param_list = self.parse_parenthesized_parameter_list(false);
        // "await x()" is ambiguous (see note at start of this method), but we assume "await x(await y)" is meant to be a function if it's in a non-async context.
        if !force_local_func {
            let a = self.arena_ref();
            let param_list_syntax = a.slot(param_list, nt::ParameterListSyntax::PARAMETERS);
            for i in 0..separated_count(a, param_list_syntax) {
                // "await x(y)" still parses as a parameter list, so check to see if it's a valid parameter (like "x(t y)")
                force_local_func |= !a.contains_diagnostics(list_item(a, param_list_syntax, 2 * i));
                if force_local_func {
                    break;
                }
            }
        }

        let mut constraints: Option<SyntaxListBuilder> = None;
        if self.current_contextual_kind() == SyntaxKind::WhereKeyword {
            let mut clauses = self.pool.allocate();
            self.parse_type_parameter_constraint_clauses(&mut clauses);
            constraints = Some(clauses);
            force_local_func = true;
        }

        let mut block_body = Green::NULL;
        let mut expression_body = Green::NULL;
        let mut semicolon = Green::NULL;
        self.parse_block_and_expression_bodies_with_semicolon(
            &mut block_body,
            &mut expression_body,
            &mut semicolon,
            false,
        );

        let constraints_list = match constraints {
            Some(clauses) => self.pool.to_list_and_free(self.base.arena(), clauses),
            None => Green::NULL,
        };

        let result = if !force_local_func && block_body.is_null() && expression_body.is_null() {
            self.reset_disposable(&mut reset_point);
            Green::NULL
        } else {
            self.arena().local_function_statement(
                attributes,
                modifiers,
                type_,
                identifier,
                type_parameter_list_opt,
                param_list,
                constraints_list,
                block_body,
                expression_body,
                semicolon,
            )
        };

        restore_syntax_context(self, saved_context);
        self.dispose_reset_point(reset_point);
        result
    }

    /// `private ExpressionStatementSyntax ParseExpressionStatement(SyntaxList<AttributeListSyntax> attributes)` (LanguageParser.cs:11101).
    pub(crate) fn parse_expression_statement(&mut self, attributes: Green) -> Green {
        let expression = self.parse_expression_core();
        self.parse_expression_statement_with_expression(attributes, expression)
    }

    /// `private ExpressionStatementSyntax ParseExpressionStatement(SyntaxList<AttributeListSyntax> attributes, ExpressionSyntax expression)` (LanguageParser.cs:11106).
    pub(crate) fn parse_expression_statement_with_expression(&mut self, attributes: Green, expression: Green) -> Green {
        let semicolon = if self.is_script() && self.current_kind() == SyntaxKind::EndOfFileToken {
            self.arena().missing_token(SyntaxKind::SemicolonToken)
        } else {
            // Do not report an error if the expression is not a statement expression.
            // The error is reported in semantic analysis.
            self.eat_token_kind(SyntaxKind::SemicolonToken)
        };

        self.arena().expression_statement(attributes, expression, semicolon)
    }
}

// Helpers of this file, not Roslyn's methods: `ParserSyntaxContextResetter` and what Roslyn reads off a green node's
// typed wrapper.

/// `new ParserSyntaxContextResetter(parser, ...)`: the four context flags, for `restore_syntax_context` (its
/// `Dispose`).
fn save_syntax_context(p: &LanguageParser<'_, '_>) -> [bool; 4] {
    [p.is_in_async, p.is_in_query, p.is_in_field_keyword_context, p.force_conditional_access_expression]
}

/// `ParserSyntaxContextResetter.Dispose()`.
fn restore_syntax_context(p: &mut LanguageParser<'_, '_>, saved: [bool; 4]) {
    [p.is_in_async, p.is_in_query, p.is_in_field_keyword_context, p.force_conditional_access_expression] = saved;
}

/// `SyntaxList<T>.Count` (and the element count of `GetWithSeparators()`): null is empty, a list its slots, any other
/// element a list of one.
fn list_count(a: &GreenArena, list: Green) -> usize {
    if list.is_null() {
        0
    } else if list.is_list() {
        a.slot_count(list)
    } else {
        1
    }
}

/// `SyntaxList<T>[index]`.
fn list_item(a: &GreenArena, list: Green, index: usize) -> Green {
    if list.is_list() {
        a.slot(list, index)
    } else if index == 0 {
        list
    } else {
        Green::NULL
    }
}

/// `SeparatedSyntaxList<T>.Count`: the elements, not the separators.
fn separated_count(a: &GreenArena, list: Green) -> usize {
    list_count(a, list).div_ceil(2)
}

/// `SyntaxList<T>.Any(kind)`: an element of the kind (`RawKind`).
fn list_any_kind(a: &GreenArena, list: Green, kind: SyntaxKind) -> bool {
    (0..list_count(a, list)).any(|i| a.kind(list_item(a, list, i)) == kind)
}

/// `token.TrailingTrivia.Any((int)SyntaxKind.EndOfLineTrivia)`.
fn trailing_trivia_has_end_of_line(a: &GreenArena, token: Green) -> bool {
    a.trivia_items(a.trailing_trivia(token)).iter().any(|&t| a.trivia_kind(t) == SyntaxKind::EndOfLineTrivia)
}

/// `SyntaxFacts.IsIdentifierVar(token)`: `ContextualKind == VarKeyword`.
fn is_identifier_var(a: &GreenArena, token: Green) -> bool {
    a.contextual_kind(token) == SyntaxKind::VarKeyword
}

/// `node is PatternSyntax`: a node of a kind of `PatternSyntax`'s subtypes in Syntax.xml.
fn is_pattern(a: &GreenArena, node: Green) -> bool {
    node.is_node()
        && matches!(
            a.kind(node),
            SyntaxKind::DiscardPattern
                | SyntaxKind::DeclarationPattern
                | SyntaxKind::VarPattern
                | SyntaxKind::RecursivePattern
                | SyntaxKind::ConstantPattern
                | SyntaxKind::ParenthesizedPattern
                | SyntaxKind::RelationalPattern
                | SyntaxKind::TypePattern
                | SyntaxKind::OrPattern
                | SyntaxKind::AndPattern
                | SyntaxKind::NotPattern
                | SyntaxKind::ListPattern
                | SyntaxKind::SlicePattern
        )
}

/// `node is ExpressionSyntax` of what `ParseExpressionOrPatternForSwitchStatement` returns: an expression or a
/// pattern, so a node that is no pattern.
fn is_expression(a: &GreenArena, node: Green) -> bool {
    node.is_node() && !is_pattern(a, node)
}
