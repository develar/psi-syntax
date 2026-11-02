//! `LanguageParser.cs` 6163-8247: type parameters and constraints, type arguments and their scan, `ScanType`,
//! `ParseType`, tuples and function pointer types (lane A). Every method in C# order; see `parser/mod.rs` for the conventions.

use super::*;
use crate::syntax::types as T;

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private TypeParameterListSyntax ParseTypeParameterList()` (LanguageParser.cs:6163).
    pub(crate) fn parse_type_parameter_list(&mut self) -> Green {
        if self.current_kind() != SyntaxKind::LessThanToken {
            return Green::NULL;
        }

        let save_term = self.term_state;
        self.term_state = self.term_state | TerminatorState::IS_END_OF_TYPE_PARAMETER_LIST;

        let mut open = self.eat_token_kind(SyntaxKind::LessThanToken);
        let parameters = self.parse_comma_separated_syntax_list(
            &mut open,
            SyntaxKind::GreaterThanToken,
            |this| this.is_start_of_type_parameter(),
            |this| this.parse_type_parameter(),
            Self::parse_type_parameter_list_skip_bad_type_parameter_list_tokens,
            false,
            true,
            false,
        );

        self.term_state = save_term;

        let close = self.eat_token_kind(SyntaxKind::GreaterThanToken);
        self.base.arena().type_parameter_list(open, parameters, close)
    }

    /// `skipBadTypeParameterListTokens` (a local function of `ParseTypeParameterList`, LanguageParser.cs:6191).
    fn parse_type_parameter_list_skip_bad_type_parameter_list_tokens(
        this: &mut LanguageParser<'_, '_>,
        open: &mut Green,
        list: &mut SyntaxListBuilder,
        expected_kind: SyntaxKind,
        close_kind: SyntaxKind,
    ) -> PostSkipAction {
        this.skip_bad_separated_list_tokens_with_expected_kind(
            open,
            list,
            |p| p.current_kind() != SyntaxKind::CommaToken,
            |p, close_kind| p.current_kind() == close_kind,
            expected_kind,
            close_kind,
        )
    }

    /// `private bool IsStartOfTypeParameter()` (LanguageParser.cs:6201).
    pub(crate) fn is_start_of_type_parameter(&mut self) -> bool {
        if self.is_current_token_where_of_constraint_clause() {
            return false;
        }

        // possible attributes.
        // Variance.
        if matches!(self.current_kind(), SyntaxKind::OpenBracketToken | SyntaxKind::InKeyword | SyntaxKind::OutKeyword)
        {
            return true;
        }

        self.is_true_identifier()
    }

    /// `private TypeParameterSyntax ParseTypeParameter()` (LanguageParser.cs:6214).
    pub(crate) fn parse_type_parameter(&mut self) -> Green {
        let mut attrs = Green::NULL;
        if self.current_kind() == SyntaxKind::OpenBracketToken {
            let save_term = self.term_state;
            self.term_state = TerminatorState::IS_END_OF_TYPE_ARGUMENT_LIST;
            attrs = self.parse_attribute_declarations(false);
            self.term_state = save_term;
        }

        if self.is_current_token_where_of_constraint_clause()
            || self.is_current_token_partial_keyword_of_partial_member_or_type()
        {
            let missing = self.create_missing_identifier_token();
            let identifier = self.add_error(missing, ErrorCode::ERR_IdentifierExpected);
            return self.base.arena().type_parameter(attrs, Green::NULL, identifier);
        }

        let variance = if matches!(self.current_kind(), SyntaxKind::InKeyword | SyntaxKind::OutKeyword) {
            self.eat_token()
        } else {
            Green::NULL
        };
        let identifier = self.parse_identifier_token(ErrorCode::ERR_IdentifierExpected);
        self.base.arena().type_parameter(attrs, variance, identifier)
    }

    /// `private SimpleNameSyntax ParseSimpleName(NameOptions options = NameOptions.None)` (LanguageParser.cs:6241).
    pub(crate) fn parse_simple_name(&mut self, options: NameOptions) -> Green {
        let id = self.parse_identifier_name(ErrorCode::ERR_IdentifierExpected);
        let identifier = self.base.arena_ref().slot(id, T::IdentifierNameSyntax::IDENTIFIER);
        if self.base.arena_ref().is_missing(identifier) {
            return id;
        }

        // You can pass ignore generics if you don't even want the parser to consider generics at all.
        // The name parsing will then stop at the first "<". It doesn't make sense to pass both Generic and IgnoreGeneric.

        let mut name = id;
        if self.current_kind() == SyntaxKind::LessThanToken {
            let kind;
            {
                let reset_point = self.get_disposable_reset_point(true);
                kind = self.scan_type_argument_list(options);
                self.dispose_reset_point(reset_point);
            }

            if kind == ScanTypeArgumentListKind::DefiniteTypeArgumentList
                || (kind == ScanTypeArgumentListKind::PossibleTypeArgumentList
                    && options.contains(NameOptions::IN_TYPE_LIST))
            {
                debug_assert!(self.current_kind() == SyntaxKind::LessThanToken);

                let mut types = self.pool.allocate();
                let mut open = Green::NULL;
                let mut close = Green::NULL;
                self.parse_type_argument_list(&mut open, &mut types, &mut close);
                let arguments = self.pool.to_list_and_free(self.base.lexer.arena(), types);
                let a = self.base.arena();
                let type_argument_list = a.type_argument_list(open, arguments, close);
                name = a.generic_name(identifier, type_argument_list);
            }
        }

        name
    }
}

/// `ScanTypeArgumentListKind` (LanguageParser.cs:6279).
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub(crate) enum ScanTypeArgumentListKind {
    #[default]
    NotTypeArgumentList,
    PossibleTypeArgumentList,
    DefiniteTypeArgumentList,
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private ScanTypeArgumentListKind ScanTypeArgumentList(NameOptions options)` (LanguageParser.cs:6286).
    pub(crate) fn scan_type_argument_list(&mut self, options: NameOptions) -> ScanTypeArgumentListKind {
        if self.current_kind() != SyntaxKind::LessThanToken {
            return ScanTypeArgumentListKind::NotTypeArgumentList;
        }

        if !options.contains(NameOptions::IN_EXPRESSION) {
            return ScanTypeArgumentListKind::DefiniteTypeArgumentList;
        }

        // We're in an expression context, and we have a < token.  This could be a
        // type argument list, or it could just be a relational expression.
        //
        // Scan just the type argument list portion (i.e. the part from < to > ) to
        // see what we think it could be.  This will give us one of three possibilities:
        //
        //      result == ScanTypeFlags.NotType.
        //
        // This is absolutely not a type-argument-list.  Just return that result immediately.
        //
        //      result != ScanTypeFlags.NotType && isDefinitelyTypeArgumentList.
        //
        // This is absolutely a type-argument-list.  Just return that result immediately
        //
        //      result != ScanTypeFlags.NotType && !isDefinitelyTypeArgumentList.
        //
        // This could be a type-argument list, or it could be an expression.  Need to see
        // what came after the last '>' to find out which it is.

        // Scan for a type argument list. If we think it's a type argument list
        // then assume it is unless we see specific tokens following it.
        let mut greater_than_token = Green::NULL;
        let mut is_definitely_type_argument_list = false;
        let possible_type_argument_flags =
            self.scan_possible_type_argument_list(&mut greater_than_token, &mut is_definitely_type_argument_list);

        if possible_type_argument_flags == ScanTypeFlags::NOT_TYPE {
            return ScanTypeArgumentListKind::NotTypeArgumentList;
        }

        if is_definitely_type_argument_list {
            return ScanTypeArgumentListKind::DefiniteTypeArgumentList;
        }

        // If we did not definitively determine from immediate syntax that it was or
        // was not a type argument list, we must have scanned the entire thing up through
        // the closing greater-than token. In that case we will disambiguate based on the
        // token that follows it.
        debug_assert!(self.base.arena_ref().kind(greater_than_token) == SyntaxKind::GreaterThanToken);

        match self.current_kind() {
            SyntaxKind::OpenParenToken
            | SyntaxKind::CloseParenToken
            | SyntaxKind::CloseBracketToken
            | SyntaxKind::CloseBraceToken
            | SyntaxKind::ColonToken
            | SyntaxKind::SemicolonToken
            | SyntaxKind::CommaToken
            | SyntaxKind::DotToken
            | SyntaxKind::QuestionToken
            | SyntaxKind::EqualsEqualsToken
            | SyntaxKind::ExclamationEqualsToken
            | SyntaxKind::BarToken
            | SyntaxKind::CaretToken => {
                // These tokens are from 7.5.4.2 Grammar Ambiguities
                ScanTypeArgumentListKind::DefiniteTypeArgumentList
            }

            SyntaxKind::AmpersandAmpersandToken // e.g. `e is A<B> && e`
            | SyntaxKind::BarBarToken // e.g. `e is A<B> || e`
            | SyntaxKind::AmpersandToken // e.g. `e is A<B> & e`
            | SyntaxKind::OpenBracketToken // e.g. `e is A<B>[]`
            | SyntaxKind::LessThanToken // e.g. `e is A<B> < C`
            | SyntaxKind::LessThanEqualsToken // e.g. `e is A<B> <= C`
            | SyntaxKind::GreaterThanEqualsToken // e.g. `e is A<B> >= C`
            | SyntaxKind::IsKeyword // e.g. `e is A<B> is bool`
            | SyntaxKind::AsKeyword // e.g. `e is A<B> as bool`
            => {
                // These tokens were added to 7.5.4.2 Grammar Ambiguities in C# 7.0
                ScanTypeArgumentListKind::DefiniteTypeArgumentList
            }

            SyntaxKind::OpenBraceToken => {
                // e.g. `e is A<B> {}`
                // This token was added to 7.5.4.2 Grammar Ambiguities in C# 8.0
                ScanTypeArgumentListKind::DefiniteTypeArgumentList
            }

            SyntaxKind::GreaterThanToken
                if options.contains(NameOptions::AFTER_IS) && self.peek_kind(1) != SyntaxKind::GreaterThanToken =>
            {
                // This token is added to 7.5.4.2 Grammar Ambiguities in C#7 for the special case in which
                // the possible generic is following an `is` keyword, e.g. `e is A<B> > C`.
                // We test one further token ahead because a right-shift operator `>>` looks like a pair of greater-than
                // tokens at this stage, but we don't intend to be handling the right-shift operator.
                // The upshot is that we retain compatibility with the two previous behaviors:
                // `(x is A<B>>C)` is parsed as `(x is A<B>) > C`
                // `A<B>>C` elsewhere is parsed as `A < (B >> C)`
                ScanTypeArgumentListKind::DefiniteTypeArgumentList
            }

            SyntaxKind::IdentifierToken => {
                // C#7: In certain contexts, we treat *identifier* as a disambiguating token. Those
                // contexts are where the sequence of tokens being disambiguated is immediately preceded by one
                // of the keywords is, case, or out, or arises while parsing the first element of a tuple literal
                // (in which case the tokens are preceded by `(` and the identifier is followed by a `,`) or a
                // subsequent element of a tuple literal (in which case the tokens are preceded by `,` and the
                // identifier is followed by a `,` or `)`).
                // In C#8 (or whenever recursive patterns are introduced) we also treat an identifier as a
                // disambiguating token if we're parsing the type of a pattern.
                // Note that we treat query contextual keywords (which appear here as identifiers) as disambiguating tokens as well.
                if options.contains(NameOptions::AFTER_IS | NameOptions::DEFINITE_PATTERN | NameOptions::AFTER_OUT)
                    || (options.contains(NameOptions::AFTER_TUPLE_COMMA)
                        && matches!(self.peek_kind(1), SyntaxKind::CommaToken | SyntaxKind::CloseParenToken))
                    || (options.contains(NameOptions::FIRST_ELEMENT_OF_POSSIBLE_TUPLE_LITERAL)
                        && self.peek_kind(1) == SyntaxKind::CommaToken)
                {
                    // we allow 'G<T,U> x' as a pattern-matching operation and a declaration expression in a tuple.
                    return ScanTypeArgumentListKind::DefiniteTypeArgumentList;
                }

                ScanTypeArgumentListKind::PossibleTypeArgumentList
            }

            SyntaxKind::EndOfFileToken => {
                // e.g. `e is A<B>`
                // This is useful for parsing expressions in isolation
                ScanTypeArgumentListKind::DefiniteTypeArgumentList
            }

            SyntaxKind::EqualsGreaterThanToken => {
                // e.g. `e switch { A<B> => 1 }`
                // This token was added to 7.5.4.2 Grammar Ambiguities in C# 9.0
                ScanTypeArgumentListKind::DefiniteTypeArgumentList
            }

            _ => ScanTypeArgumentListKind::PossibleTypeArgumentList,
        }
    }

    /// `private ScanTypeFlags ScanPossibleTypeArgumentList( out SyntaxToken greaterThanToken, out bool isDefinitelyTypeArgumentList)` (LanguageParser.cs:6415).
    pub(crate) fn scan_possible_type_argument_list(
        &mut self,
        greater_than_token: &mut Green,
        is_definitely_type_argument_list: &mut bool,
    ) -> ScanTypeFlags {
        *is_definitely_type_argument_list = false;
        debug_assert!(self.current_kind() == SyntaxKind::LessThanToken);

        // If we have `X<>`, or `X<,>` or `X<,,,,,,>` then none of these are legal expression or types. However,
        // it seems likelier that they are invalid open-types in an expression context, versus expressions
        // missing values (note that we only support this when the open name does have the final `>` token).
        if self.is_open_name() {
            *is_definitely_type_argument_list = true;

            let start = self.eat_token();
            while self.current_kind() == SyntaxKind::CommaToken {
                self.eat_token();
            }
            *greater_than_token = self.eat_token();
            debug_assert!(self.base.arena_ref().kind(start) == SyntaxKind::LessThanToken);
            debug_assert!(self.base.arena_ref().kind(*greater_than_token) == SyntaxKind::GreaterThanToken);

            return ScanTypeFlags::GENERIC_TYPE_OR_METHOD;
        }

        let mut result = ScanTypeFlags::GENERIC_TYPE_OR_EXPRESSION;
        let mut last_scanned_type;

        loop {
            self.eat_token();

            // Type arguments cannot contain attributes, so if this is an open square, we early out and assume it is not a type argument
            if self.current_kind() == SyntaxKind::OpenBracketToken {
                *greater_than_token = Green::NULL;
                return ScanTypeFlags::NOT_TYPE;
            }

            if self.current_kind() == SyntaxKind::GreaterThanToken {
                *greater_than_token = self.eat_token();
                return result;
            }

            // Allow for any chain of errant commas in the generic name.  like `Dictionary<,int>` or
            // `Dictionary<int,,>` We still want to think of these as generics, just with missing type-arguments, vs
            // some invalid tree-expression that we would otherwise form.
            if self.current_kind() == SyntaxKind::CommaToken {
                // (`lastScannedType = default`: never read, the next round sets it before the loop can end.)
                continue;
            }

            let mut unused = Green::NULL;
            last_scanned_type = self.scan_type_with_for_pattern(&mut unused, false);
            match last_scanned_type {
                ScanTypeFlags::NOT_TYPE => {
                    *greater_than_token = Green::NULL;
                    return ScanTypeFlags::NOT_TYPE;
                }

                ScanTypeFlags::MUST_BE_TYPE => {
                    // We're currently scanning a possible type-argument list.  But we're
                    // not sure if this is actually a type argument list, or is maybe some
                    // complex relational expression with <'s and >'s.  One thing we can
                    // tell though is that if we have a predefined type (like 'int' or 'string')
                    // before a comma or > then this is definitely a type argument list. i.e.
                    // if you have:
                    //
                    //      var v = ImmutableDictionary<int,
                    //
                    // then there's no legal interpretation of this as an expression (since a
                    // standalone predefined type is not a valid simple term.  Contrast that
                    // with :
                    //
                    //  var v = ImmutableDictionary<Int32,
                    //
                    // Here this might actually be a relational expression and the comma is meant
                    // to separate out the variable declarator 'v' from the next variable.
                    //
                    // Note: we check if we got 'MustBeType' which triggers for predefined types,
                    // (int, string, etc.), or array types (Goo[], A<T>[][] etc.), or pointer types
                    // of things that must be types (int*, void**, etc.).
                    *is_definitely_type_argument_list = *is_definitely_type_argument_list
                        || matches!(self.current_kind(), SyntaxKind::CommaToken | SyntaxKind::GreaterThanToken);
                    result = ScanTypeFlags::GENERIC_TYPE_OR_METHOD;
                }

                // case ScanTypeFlags.TupleType:
                // It would be nice if we saw a tuple to state that we definitely had a
                // type argument list.  However, there are cases where this would not be
                // true.  For example:
                //
                // public class C
                // {
                //     public static void Main()
                //     {
                //         XX X = default;
                //         int a = 1, b = 2;
                //         bool z = X < (a, b), w = false;
                //     }
                // }
                //
                // struct XX
                // {
                //     public static bool operator <(XX x, (int a, int b) arg) => true;
                //     public static bool operator >(XX x, (int a, int b) arg) => false;
                // }
                ScanTypeFlags::NULLABLE_TYPE => {
                    // See above.  If we have `X<Y?,` or `X<Y?>` then this is definitely a type argument list.
                    *is_definitely_type_argument_list = *is_definitely_type_argument_list
                        || matches!(self.current_kind(), SyntaxKind::CommaToken | SyntaxKind::GreaterThanToken);
                    if *is_definitely_type_argument_list {
                        result = ScanTypeFlags::GENERIC_TYPE_OR_METHOD;
                    }

                    // Note: we intentionally fall out without setting 'result'.
                    // Seeing a nullable type (not followed by a , or > ) is not enough
                    // information for us to determine what this is yet.  i.e. the user may have:
                    //
                    //      X < Y ? Z : W
                    //
                    // We'd see a nullable type here, but this is definitely not a type arg list.
                }

                ScanTypeFlags::GENERIC_TYPE_OR_EXPRESSION => {
                    // See above.  If we have  X<Y<Z>,  then this would definitely be a type argument list.
                    // However, if we have  X<Y<Z>> then this might not be type argument list.  This could just
                    // be some sort of expression where we're comparing, and then shifting values.
                    if !*is_definitely_type_argument_list {
                        *is_definitely_type_argument_list = self.current_kind() == SyntaxKind::CommaToken;
                        result = ScanTypeFlags::GENERIC_TYPE_OR_METHOD;
                    }
                }

                ScanTypeFlags::GENERIC_TYPE_OR_METHOD => {
                    result = ScanTypeFlags::GENERIC_TYPE_OR_METHOD;
                }

                // NonGenericTypeOrExpression:
                // Explicitly keeping this case in the switch for clarity.  We parsed out another portion of the
                // type argument list that looks like it's a non-generic-type-or-expr (the simplest case just
                // being "X").  That changes nothing here wrt determining what type of entity we have here, so
                // just fall through and see if we're followed by a "," (in which case keep going), or a ">", in
                // which case we're done.
                _ => {}
            }

            if self.current_kind() != SyntaxKind::CommaToken {
                break;
            }
        }

        if self.current_kind() != SyntaxKind::GreaterThanToken {
            // Error recovery after missing > token:

            // In the case of an identifier, we assume that there could be a missing > token
            // For example, we have reached C in X<A, B C
            if self.current_kind() == SyntaxKind::IdentifierToken {
                *greater_than_token = self.eat_token_kind(SyntaxKind::GreaterThanToken);
                return result;
            }

            // As for tuples, we do not expect direct invocation right after the parenthesis
            // EXAMPLE: X<(string, string)(), where we imply a missing > token between )(
            // as the user probably wants to invoke X by X<(string, string)>()
            if last_scanned_type == ScanTypeFlags::TUPLE_TYPE && self.current_kind() == SyntaxKind::OpenParenToken {
                *greater_than_token = self.eat_token_kind(SyntaxKind::GreaterThanToken);
                return result;
            }

            *greater_than_token = Green::NULL;
            return ScanTypeFlags::NOT_TYPE;
        }

        *greater_than_token = self.eat_token();

        // If we have `X<Y>)` then this would definitely be a type argument list.
        *is_definitely_type_argument_list =
            *is_definitely_type_argument_list || self.current_kind() == SyntaxKind::CloseParenToken;
        if *is_definitely_type_argument_list {
            result = ScanTypeFlags::GENERIC_TYPE_OR_METHOD;
        }

        result
    }

    /// `private void ParseTypeArgumentList(out SyntaxToken open, SeparatedSyntaxListBuilder<TypeSyntax> types, out SyntaxToken close)` (LanguageParser.cs:6604).
    pub(crate) fn parse_type_argument_list(
        &mut self,
        open: &mut Green,
        types: &mut SyntaxListBuilder,
        close: &mut Green,
    ) {
        debug_assert!(self.current_kind() == SyntaxKind::LessThanToken);
        let is_open_name = self.is_open_name();
        *open = self.eat_token_kind(SyntaxKind::LessThanToken);
        *open = self.check_feature_availability(*open, MessageID::IDS_FeatureGenerics, false);

        if is_open_name {
            // NOTE: trivia will be attached to comma, not omitted type argument
            let a = self.base.arena();
            let token = a.token(SyntaxKind::OmittedTypeArgumentToken);
            let omitted_type_argument_instance = a.omitted_type_argument(token);
            types.push(omitted_type_argument_instance);
            while self.current_kind() == SyntaxKind::CommaToken {
                let comma = self.eat_token_kind(SyntaxKind::CommaToken);
                types.push(comma);
                types.push(omitted_type_argument_instance);
            }

            *close = self.eat_token_kind(SyntaxKind::GreaterThanToken);

            return;
        }

        // first type
        let first = self.parse_type_argument();
        types.push(first);

        // remaining types & commas
        loop {
            if self.current_kind() == SyntaxKind::GreaterThanToken {
                break;
            }

            // We prefer early terminating the argument list over parsing until exhaustion
            // for better error recovery
            let current = self.current_token();
            if self.parse_type_argument_list_token_breaks_type_argument_list(current) {
                break;
            }

            // We are currently past parsing a type and we encounter an unexpected identifier token
            // followed by tokens that are not part of a type argument list
            // Example: List<(string a, string b) Method() { }
            //                 current token:     ^^^^^^
            if self.current_kind() == SyntaxKind::IdentifierToken {
                let next = self.peek_token(1);
                if self.parse_type_argument_list_token_breaks_type_argument_list(next) {
                    break;
                }
            }

            // This is for the case where we are in a this[] accessor, and the last one of the parameters in the parameter list
            // is missing a > on its type
            // Example: X this[IEnumerable<string parameter] =>
            //                 current token:     ^^^^^^^^^
            if self.current_kind() == SyntaxKind::IdentifierToken && self.peek_kind(1) == SyntaxKind::CloseBracketToken
            {
                break;
            }

            if self.current_kind() == SyntaxKind::CommaToken || self.is_possible_type() {
                let comma = self.eat_token_kind(SyntaxKind::CommaToken);
                types.push(comma);
                let argument = self.parse_type_argument();
                types.push(argument);
            } else if self.skip_bad_type_argument_list_tokens(types, SyntaxKind::CommaToken) == PostSkipAction::Abort {
                break;
            }
        }

        *close = self.eat_token_kind(SyntaxKind::GreaterThanToken);
    }

    /// `tokenBreaksTypeArgumentList` (a local function of `ParseTypeArgumentList`, LanguageParser.cs:6677).
    fn parse_type_argument_list_token_breaks_type_argument_list(&self, token: Green) -> bool {
        let a = self.base.arena_ref();
        let contextual_kind = facts::get_contextual_keyword_kind(a.token_value_text(token, self.source()));
        match contextual_kind {
            // Example: x is IEnumerable<string or IList<int>
            SyntaxKind::OrKeyword
            // Example: x is IEnumerable<string and IDisposable
            | SyntaxKind::AndKeyword => return true,
            _ => {}
        }

        matches!(
            a.kind(token),
            // Example: Method<string(argument)
            // Note: We would do a bad job handling a tuple argument with a missing comma,
            //       like: Method<string (int x, int y)>
            //       but since we do not look as far as possible to determine whether it is
            //       a tuple type or an argument list, we resort to considering it as an
            //       argument list
            SyntaxKind::OpenParenToken
                // Example: IEnumerable<string Method<T>() --- (< in <T>)
                | SyntaxKind::LessThanToken
                // Example: Method(IEnumerable<string parameter)
                | SyntaxKind::CloseParenToken
                // Example: IEnumerable<string field;
                | SyntaxKind::SemicolonToken
                // Example: IEnumerable<string Property { get; set; }
                | SyntaxKind::OpenBraceToken
                // Example:
                // {
                //     IEnumerable<string field
                // }
                | SyntaxKind::CloseBraceToken
                // Examples:
                // - IEnumerable<string field = null;
                // - Method(IEnumerable<string parameter = null)
                | SyntaxKind::EqualsToken
                // Example: IEnumerable<string Property => null;
                | SyntaxKind::EqualsGreaterThanToken
                // Example: IEnumerable<string this[string key] { get; set; }
                | SyntaxKind::ThisKeyword
                // Example: static IEnumerable<string operator +(A left, A right);
                | SyntaxKind::OperatorKeyword
        )
    }

    /// `private PostSkipAction SkipBadTypeArgumentListTokens(SeparatedSyntaxListBuilder<TypeSyntax> list, SyntaxKind expected)` (LanguageParser.cs:6729).
    pub(crate) fn skip_bad_type_argument_list_tokens(
        &mut self,
        list: &mut SyntaxListBuilder,
        expected: SyntaxKind,
    ) -> PostSkipAction {
        let mut tmp = Green::NULL;
        debug_assert!(!list.is_empty());
        self.skip_bad_separated_list_tokens_with_expected_kind(
            &mut tmp,
            list,
            |p| p.current_kind() != SyntaxKind::CommaToken && !p.is_possible_type(),
            |p, _| p.current_kind() == SyntaxKind::GreaterThanToken,
            expected,
            SyntaxKind::None,
        )
    }

    /// `private TypeSyntax ParseTypeArgument()` (LanguageParser.cs:6740).
    pub(crate) fn parse_type_argument(&mut self) -> Green {
        let mut attrs = Green::NULL;
        if self.current_kind() == SyntaxKind::OpenBracketToken && self.peek_kind(1) != SyntaxKind::CloseBracketToken {
            // Here, if we see a "[" that looks like it has something in it, we parse
            // it as an attribute and then later put an error on the whole type if
            // it turns out that attributes are not allowed.
            // TODO: should there be another flag that controls this behavior? we have
            // "allowAttrs" but should there also be a "recognizeAttrs" that we can
            // set to false in an expression context?

            let save_term = self.term_state;
            self.term_state = TerminatorState::IS_END_OF_TYPE_ARGUMENT_LIST;
            attrs = self.parse_attribute_declarations(false);
            self.term_state = save_term;
        }

        // Recognize the variance syntax, but give an error as it's only appropriate in a type parameter list.
        let variance_token = if matches!(self.current_kind(), SyntaxKind::InKeyword | SyntaxKind::OutKeyword) {
            let token = self.eat_token();
            self.add_error(token, ErrorCode::ERR_IllegalVarianceSyntax)
        } else {
            Green::NULL
        };

        let mut result = self.parse_type(ParseTypeMode::Normal);

        // Consider the case where someone supplies an invalid type argument
        // Such as Action<0> or Action<static>.  In this case we generate a missing
        // identifier in ParseType, but if we continue as is we'll immediately start to
        // interpret 0 as the start of a new expression when we can tell it's most likely
        // meant to be part of the type list.
        //
        // To solve this we check if the current token is not comma or greater than and
        // the next token is a comma or greater than. If so we assume that the found
        // token is part of this expression and we attempt to recover. This does open
        // the door for cases where we have an  incomplete line to be interpretted as
        // a single expression.  For example:
        //
        // Action< // Incomplete line
        // a>b;
        //
        // However, this only happens when the following expression is of the form a>...
        // or a,... which  means this case should happen less frequently than what we're
        // trying to solve here so we err on the side of better error messages
        // for the majority of cases.

        if self.base.arena_ref().is_missing(result)
            && !matches!(self.current_kind(), SyntaxKind::CommaToken | SyntaxKind::GreaterThanToken)
            && matches!(self.peek_kind(1), SyntaxKind::CommaToken | SyntaxKind::GreaterThanToken)
        {
            // Eat the current token and add it as skipped so we recover
            let token = self.eat_token();
            result = self.add_trailing_skipped_syntax(result, token);
        }

        if !variance_token.is_null() {
            result = self.add_leading_skipped_syntax(result, variance_token);
        }

        if !attrs.is_null() {
            result = self.add_leading_skipped_syntax(result, attrs);
            result = self.add_error(result, ErrorCode::ERR_TypeExpected);
        }

        result
    }

    /// `private bool IsEndOfTypeArgumentList()` (LanguageParser.cs:6807).
    pub(crate) fn is_end_of_type_argument_list(&mut self) -> bool {
        self.current_kind() == SyntaxKind::GreaterThanToken
    }

    /// `private bool IsOpenName()` (LanguageParser.cs:6810).
    pub(crate) fn is_open_name(&mut self) -> bool {
        debug_assert!(self.current_kind() == SyntaxKind::LessThanToken);
        let mut n = 1;
        while self.peek_kind(n) == SyntaxKind::CommaToken {
            n += 1;
        }

        self.peek_kind(n) == SyntaxKind::GreaterThanToken
    }

    /// `private void ParseMemberName( out ExplicitInterfaceSpecifierSyntax explicitInterfaceOpt, out SyntaxToken identifierOrThisOpt, out TypeParameterListSyntax typeParameterListOpt, bool isEvent)` (LanguageParser.cs:6820).
    pub(crate) fn parse_member_name(
        &mut self,
        explicit_interface_opt: &mut Green,
        identifier_or_this_opt: &mut Green,
        type_parameter_list_opt: &mut Green,
        is_event: bool,
    ) {
        *identifier_or_this_opt = Green::NULL;
        *explicit_interface_opt = Green::NULL;
        *type_parameter_list_opt = Green::NULL;

        if !self.is_possible_member_name() {
            // No clue what this is.  Just bail.  Our caller will have to
            // move forward and try again.
            return;
        }

        let mut explicit_interface_name = Green::NULL;
        let mut separator = Green::NULL;

        let mut before_identifier_point: Option<ResetPoint>;

        loop {
            // Check if we got 'this'.  If so, then we have an indexer.
            // Note: we parse out type parameters here as well so that
            // we can give a useful error about illegal generic indexers.
            if self.current_kind() == SyntaxKind::ThisKeyword {
                before_identifier_point = Some(self.get_reset_point());
                *identifier_or_this_opt = self.eat_token();
                *type_parameter_list_opt = self.parse_type_parameter_list();
                break;
            }

            // now, scan past the next name.  if it's followed by a dot then
            // it's part of the explicit name we're building up.  Otherwise,
            // it's the name of the member.
            let is_member_name;
            {
                let reset_point = self.get_disposable_reset_point(true);
                self.scan_named_type_part();
                is_member_name = !self.is_dot_or_colon_colon();
                self.dispose_reset_point(reset_point);
            }

            if is_member_name {
                // We're past any explicit interface portion and We've
                // gotten to the member name.
                before_identifier_point = Some(self.get_reset_point());

                if !separator.is_null() && self.base.arena_ref().kind(separator) == SyntaxKind::ColonColonToken {
                    separator = self.add_error(separator, ErrorCode::ERR_AliasQualAsExpression);
                    separator = self.convert_to_missing_with_trailing_trivia(separator, SyntaxKind::DotToken);
                }

                *identifier_or_this_opt = self.parse_identifier_token(ErrorCode::ERR_IdentifierExpected);
                *type_parameter_list_opt = self.parse_type_parameter_list();
                break;
            } else {
                // If we saw a . or :: then we must have something explicit.
                self.accumulate_explicit_interface_name(&mut explicit_interface_name, &mut separator);
            }
        }

        if !explicit_interface_name.is_null() {
            let separator_kind = self.base.arena_ref().kind(separator);
            if separator_kind != SyntaxKind::DotToken {
                let a = self.base.arena_ref();
                let (offset, width) = (a.leading_trivia_width(separator) as i32, a.width(separator) as i32);
                let error = self.get_expected_token_error(SyntaxKind::DotToken, separator_kind, offset, width);
                separator = self.with_additional_diagnostics(separator, &[error]);
                separator = self.convert_to_missing_with_trailing_trivia(separator, SyntaxKind::DotToken);
            }

            if is_event && !matches!(self.current_kind(), SyntaxKind::OpenBraceToken | SyntaxKind::SemicolonToken) {
                // CS0071: If you're explicitly implementing an event field, you have to use the accessor form
                //
                // Good:
                //   event EventDelegate Parent.E
                //   {
                //      add { ... }
                //      remove { ... }
                //   }
                //
                // Bad:
                //   event EventDelegate Parent.
                //   E( //(or anything that is not the semicolon
                //
                // To recover: rollback to before the name of the field was parsed (just the part after the last
                // dot), insert a missing identifier for the field name, insert missing accessors, and then treat
                // the event name that's actually there as the beginning of a new member. e.g.
                //
                //   event EventDelegate Parent./*Missing nodes here*/
                //
                //   E(
                //
                // Rationale: The identifier could be the name of a type at the beginning of an existing member
                // declaration (above which someone has started to type an explicit event implementation).
                //
                // In case the dot doesn't follow with an end line or E ends with a semicolon, the error recovery
                // is skipped. In that case the rationale above does not fit very well.

                let dot = self.add_error(separator, ErrorCode::ERR_ExplicitEventFieldImpl);
                *explicit_interface_opt = self.base.arena().explicit_interface_specifier(explicit_interface_name, dot);

                if self.trailing_trivia_has_end_of_line(separator) {
                    debug_assert!(before_identifier_point.is_some());
                    if let Some(point) = before_identifier_point.as_mut() {
                        self.reset(point);
                    }
                    //clear fields that were populated after the reset point
                    *identifier_or_this_opt = Green::NULL;
                    *type_parameter_list_opt = Green::NULL;
                }
            } else {
                *explicit_interface_opt =
                    self.base.arena().explicit_interface_specifier(explicit_interface_name, separator);
            }
        }

        if let Some(mut point) = before_identifier_point {
            self.release(&mut point);
        }
    }

    /// `private void AccumulateExplicitInterfaceName(ref NameSyntax explicitInterfaceName, ref SyntaxToken separator)` (LanguageParser.cs:6958).
    pub(crate) fn accumulate_explicit_interface_name(
        &mut self,
        explicit_interface_name: &mut Green,
        separator: &mut Green,
    ) {
        // first parse the upcoming name portion.

        let save_term = self.term_state;
        self.term_state = self.term_state | TerminatorState::IS_END_OF_NAME_IN_EXPLICIT_INTERFACE;

        if explicit_interface_name.is_null() {
            // If this is the first time, then just get the next simple
            // name and store it as the explicit interface name.
            *explicit_interface_name = self.parse_simple_name(NameOptions::IN_TYPE_LIST);

            // Now, get the next separator.
            *separator = if self.current_kind() == SyntaxKind::ColonColonToken {
                self.eat_token() // fine after the first identifier
            } else {
                self.eat_token_kind(SyntaxKind::DotToken)
            };
        } else {
            // Parse out the next part and combine it with the
            // current explicit name to form the new explicit name.
            let tmp = self.parse_qualified_name_right(NameOptions::IN_TYPE_LIST, *explicit_interface_name, *separator);
            debug_assert!(
                tmp != *explicit_interface_name,
                "We should have consumed something and updated explicitInterfaceName"
            );
            *explicit_interface_name = tmp;

            // Now, get the next separator.
            if self.current_kind() == SyntaxKind::ColonColonToken {
                *separator = self.eat_token();
                *separator = self.add_error(*separator, ErrorCode::ERR_UnexpectedAliasedName);
                *separator = self.convert_to_missing_with_trailing_trivia(*separator, SyntaxKind::DotToken);
            } else {
                *separator = self.eat_token_kind(SyntaxKind::DotToken);
            }
        }

        self.term_state = save_term;
    }

    /// `private bool IsOperatorStart(out ExplicitInterfaceSpecifierSyntax explicitInterfaceOpt, bool advanceParser = true)` (LanguageParser.cs:7005).
    pub(crate) fn is_operator_start(&mut self, explicit_interface_opt: &mut Green, advance_parser: bool) -> bool {
        *explicit_interface_opt = Green::NULL;

        if self.is_operator_keyword() {
            return true;
        }

        if self.current_kind() != SyntaxKind::IdentifierToken {
            return false;
        }

        let mut explicit_interface_name = Green::NULL;
        let mut separator = Green::NULL;

        let mut before_identifier_point = self.get_disposable_reset_point(false);

        loop {
            // now, scan past the next name.  if it's followed by a dot then
            // it's part of the explicit name we're building up.  Otherwise,
            // it should be an operator token
            let is_part_of_interface_name;
            {
                let reset_point = self.get_disposable_reset_point(true);
                if self.is_operator_keyword() {
                    is_part_of_interface_name = false;
                } else {
                    self.scan_named_type_part();

                    // If we have part of the interface name, but no dot before the operator token, then
                    // for the purpose of error recovery, treat this as an operator start with a
                    // missing dot token.
                    is_part_of_interface_name = self.is_dot_or_colon_colon() || self.is_operator_keyword();
                }
                self.dispose_reset_point(reset_point);
            }

            if !is_part_of_interface_name {
                // We're past any explicit interface portion
                if !separator.is_null() && self.base.arena_ref().kind(separator) == SyntaxKind::ColonColonToken {
                    separator = self.add_error(separator, ErrorCode::ERR_AliasQualAsExpression);
                    separator = self.convert_to_missing_with_trailing_trivia(separator, SyntaxKind::DotToken);
                }

                break;
            } else {
                // If we saw a . or :: then we must have something explicit.
                self.accumulate_explicit_interface_name(&mut explicit_interface_name, &mut separator);
            }
        }

        if !self.is_operator_keyword() || explicit_interface_name.is_null() {
            self.reset_disposable(&mut before_identifier_point);
            self.dispose_reset_point(before_identifier_point);
            return false;
        }

        if !advance_parser {
            self.reset_disposable(&mut before_identifier_point);
            self.dispose_reset_point(before_identifier_point);
            return true;
        }

        let separator_kind = self.base.arena_ref().kind(separator);
        if separator_kind != SyntaxKind::DotToken {
            let a = self.base.arena_ref();
            let (offset, width) = (a.leading_trivia_width(separator) as i32, a.width(separator) as i32);
            let error = self.get_expected_token_error(SyntaxKind::DotToken, separator_kind, offset, width);
            separator = self.with_additional_diagnostics(separator, &[error]);
            separator = self.convert_to_missing_with_trailing_trivia(separator, SyntaxKind::DotToken);
        }

        *explicit_interface_opt = self.base.arena().explicit_interface_specifier(explicit_interface_name, separator);
        self.dispose_reset_point(before_identifier_point);
        true
    }

    /// `private NameSyntax ParseAliasQualifiedName(NameOptions allowedParts = NameOptions.None)` (LanguageParser.cs:7087).
    pub(crate) fn parse_alias_qualified_name(&mut self, allowed_parts: NameOptions) -> Green {
        let name = self.parse_simple_name(allowed_parts);
        if self.current_kind() == SyntaxKind::ColonColonToken {
            let separator = self.eat_token();
            self.parse_qualified_name_right(allowed_parts, name, separator)
        } else {
            name
        }
    }

    /// `private NameSyntax ParseQualifiedName(NameOptions options = NameOptions.None)` (LanguageParser.cs:7095).
    pub(crate) fn parse_qualified_name(&mut self, options: NameOptions) -> Green {
        let mut name = self.parse_alias_qualified_name(options);

        // Handle .. tokens for error recovery purposes.
        while self.is_dot_or_colon_colon() {
            if self.peek_kind(1) == SyntaxKind::ThisKeyword {
                break;
            }

            let separator = self.eat_token();
            name = self.parse_qualified_name_right(options, name, separator);
        }

        name
    }

    /// `private NameSyntax ParseQualifiedNameRight( NameOptions options, NameSyntax left, SyntaxToken separator)` (LanguageParser.cs:7114).
    pub(crate) fn parse_qualified_name_right(&mut self, options: NameOptions, left: Green, separator: Green) -> Green {
        debug_assert!(matches!(
            self.base.arena_ref().kind(separator),
            SyntaxKind::DotToken | SyntaxKind::ColonColonToken
        ));
        let mut separator = separator;
        let right = self.parse_simple_name(options);

        match self.base.arena_ref().kind(separator) {
            SyntaxKind::ColonColonToken => {
                if self.base.arena_ref().kind(left) != SyntaxKind::IdentifierName {
                    separator = self.add_error(separator, ErrorCode::ERR_UnexpectedAliasedName);
                }

                // If the left hand side is not an identifier name then the user has done
                // something like Goo.Bar::Blah. We've already made an error node for the
                // ::, so just pretend that they typed Goo.Bar.Blah and continue on.

                if self.base.arena_ref().kind(left) != SyntaxKind::IdentifierName {
                    separator = self.convert_to_missing_with_trailing_trivia(separator, SyntaxKind::DotToken);
                    self.base.arena().qualified_name(left, separator, right)
                } else {
                    let mut identifier_left = left;
                    let identifier = self.base.arena_ref().slot(left, T::IdentifierNameSyntax::IDENTIFIER);
                    if self.base.arena_ref().contextual_kind(identifier) == SyntaxKind::GlobalKeyword {
                        let keyword = self.convert_to_keyword(identifier);
                        identifier_left = self.base.arena().identifier_name(keyword);
                    }

                    // If the name on the right had errors or warnings then we need to preserve
                    // them in the tree.
                    let alias = self.base.arena().alias_qualified_name(identifier_left, separator, right);
                    let diagnostics: Vec<DiagnosticId> = self.base.arena_ref().diagnostics(left).collect();
                    self.with_additional_diagnostics(alias, &diagnostics)
                }
            }

            // DotToken (anything else is `throw ExceptionUtilities.Unreachable()`).
            _ => self.base.arena().qualified_name(left, separator, right),
        }
    }

    /// `private SyntaxToken ConvertToMissingWithTrailingTrivia(SyntaxToken token, SyntaxKind expectedKind)` (LanguageParser.cs:7159).
    pub(crate) fn convert_to_missing_with_trailing_trivia(&mut self, token: Green, expected_kind: SyntaxKind) -> Green {
        let new_token = self.base.arena().missing_token(expected_kind);
        self.add_trailing_skipped_syntax(new_token, token)
    }
}

/// `ScanTypeFlags` (LanguageParser.cs:7166): an enum in C#, its values in declaration order.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub(crate) struct ScanTypeFlags(pub(crate) u32);

impl ScanTypeFlags {
    pub(crate) const NOT_TYPE: ScanTypeFlags = ScanTypeFlags(0);
    pub(crate) const MUST_BE_TYPE: ScanTypeFlags = ScanTypeFlags(1);
    pub(crate) const GENERIC_TYPE_OR_METHOD: ScanTypeFlags = ScanTypeFlags(2);
    pub(crate) const GENERIC_TYPE_OR_EXPRESSION: ScanTypeFlags = ScanTypeFlags(3);
    pub(crate) const NON_GENERIC_TYPE_OR_EXPRESSION: ScanTypeFlags = ScanTypeFlags(4);
    pub(crate) const ALIAS_QUALIFIED_NAME: ScanTypeFlags = ScanTypeFlags(5);
    pub(crate) const NULLABLE_TYPE: ScanTypeFlags = ScanTypeFlags(6);
    pub(crate) const POINTER_OR_MULTIPLICATION: ScanTypeFlags = ScanTypeFlags(7);
    pub(crate) const TUPLE_TYPE: ScanTypeFlags = ScanTypeFlags(8);
}

impl ScanTypeFlags {
    /// `(value & flag) != 0`.
    #[inline]
    pub(crate) fn contains(self, flag: ScanTypeFlags) -> bool {
        self.0 & flag.0 != 0
    }

    /// `value & ~flag`.
    #[inline]
    pub(crate) fn without(self, flag: ScanTypeFlags) -> ScanTypeFlags {
        ScanTypeFlags(self.0 & !flag.0)
    }
}

impl std::ops::BitOr for ScanTypeFlags {
    type Output = ScanTypeFlags;

    #[inline]
    fn bitor(self, other: ScanTypeFlags) -> ScanTypeFlags {
        ScanTypeFlags(self.0 | other.0)
    }
}

impl std::ops::BitAnd for ScanTypeFlags {
    type Output = ScanTypeFlags;

    #[inline]
    fn bitand(self, other: ScanTypeFlags) -> ScanTypeFlags {
        ScanTypeFlags(self.0 & other.0)
    }
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private bool IsPossibleType()` (LanguageParser.cs:7218).
    pub(crate) fn is_possible_type(&mut self) -> bool {
        let tk = self.current_kind();
        Self::is_predefined_type(tk) || self.is_true_identifier()
    }

    /// `private ScanTypeFlags ScanType(bool forPattern = false)` (LanguageParser.cs:7224).
    pub(crate) fn scan_type(&mut self, for_pattern: bool) -> ScanTypeFlags {
        let mut unused = Green::NULL;
        self.scan_type_with_for_pattern(&mut unused, for_pattern)
    }

    /// `private ScanTypeFlags ScanType(out SyntaxToken lastTokenOfType, bool forPattern = false)` (LanguageParser.cs:7229).
    pub(crate) fn scan_type_with_for_pattern(
        &mut self,
        last_token_of_type: &mut Green,
        for_pattern: bool,
    ) -> ScanTypeFlags {
        let mode = if for_pattern { ParseTypeMode::DefinitePattern } else { ParseTypeMode::Normal };
        self.scan_type_with_last_token_of_type(mode, last_token_of_type)
    }

    /// `private void ScanNamedTypePart()` (LanguageParser.cs:7234).
    pub(crate) fn scan_named_type_part(&mut self) {
        let mut unused = Green::NULL;
        self.scan_named_type_part_with_last_token_of_type(&mut unused);
    }

    /// `private ScanTypeFlags ScanNamedTypePart(out SyntaxToken lastTokenOfType)` (LanguageParser.cs:7239).
    pub(crate) fn scan_named_type_part_with_last_token_of_type(
        &mut self,
        last_token_of_type: &mut Green,
    ) -> ScanTypeFlags {
        if self.current_kind() != SyntaxKind::IdentifierToken || !self.is_true_identifier() {
            *last_token_of_type = Green::NULL;
            return ScanTypeFlags::NOT_TYPE;
        }

        *last_token_of_type = self.eat_token();
        if self.current_kind() == SyntaxKind::LessThanToken {
            let mut unused = false;
            self.scan_possible_type_argument_list(last_token_of_type, &mut unused)
        } else {
            ScanTypeFlags::NON_GENERIC_TYPE_OR_EXPRESSION
        }
    }

    /// `private ScanTypeFlags ScanType(ParseTypeMode mode, out SyntaxToken lastTokenOfType)` (LanguageParser.cs:7258).
    ///
    /// Not in Roslyn: the recursion through the scans of type argument lists, tuples and function pointers counts
    /// `_recursionDepth` (Roslyn leaves it unguarded), so a scan fits a 1 MB stack.
    pub(crate) fn scan_type_with_last_token_of_type(
        &mut self,
        mode: ParseTypeMode,
        last_token_of_type: &mut Green,
    ) -> ScanTypeFlags {
        self.recursion_depth += 1;
        self.ensure_sufficient_execution_stack();
        let result = self.scan_type_with_last_token_of_type_core(mode, last_token_of_type);
        self.recursion_depth -= 1;
        result
    }

    /// The body of `ScanType(ParseTypeMode mode, out SyntaxToken lastTokenOfType)` (LanguageParser.cs:7258).
    fn scan_type_with_last_token_of_type_core(
        &mut self,
        mode: ParseTypeMode,
        last_token_of_type: &mut Green,
    ) -> ScanTypeFlags {
        debug_assert!(mode != ParseTypeMode::NewExpression);
        let mut result;

        if self.current_kind() == SyntaxKind::RefKeyword {
            // in a ref local or ref return, we treat "ref" and "ref readonly" as part of the type
            self.eat_token();

            if self.current_kind() == SyntaxKind::ReadOnlyKeyword {
                self.eat_token();
            }
        }

        // Handle :: as well for error case of an alias used without a preceding identifier.
        if matches!(self.current_kind(), SyntaxKind::IdentifierToken | SyntaxKind::ColonColonToken) {
            let mut is_alias;
            if self.current_kind() == SyntaxKind::ColonColonToken {
                result = ScanTypeFlags::NON_GENERIC_TYPE_OR_EXPRESSION;

                // Definitely seems like an alias if we're starting with a ::
                is_alias = true;

                // We set this to null to appease the flow checker.  It will always be the case that this will be
                // set to an appropriate value inside the `for` loop below.  We'll consume the :: there and then
                // call ScanNamedTypePart which will always set this to a valid value.
                *last_token_of_type = Green::NULL;
            } else {
                debug_assert!(self.current_kind() == SyntaxKind::IdentifierToken);

                // We're an alias if we start with an: id::
                is_alias = self.peek_kind(1) == SyntaxKind::ColonColonToken;

                result = self.scan_named_type_part_with_last_token_of_type(last_token_of_type);
                if result == ScanTypeFlags::NOT_TYPE {
                    return ScanTypeFlags::NOT_TYPE;
                }

                debug_assert!(matches!(
                    result,
                    ScanTypeFlags::GENERIC_TYPE_OR_EXPRESSION
                        | ScanTypeFlags::GENERIC_TYPE_OR_METHOD
                        | ScanTypeFlags::NON_GENERIC_TYPE_OR_EXPRESSION
                ));
            }

            // Scan a name
            let mut first_loop = true;
            while self.is_dot_or_colon_colon() {
                // If we consume any more dots or colons, don't consider us an alias anymore.  For dots, we now have
                // x::y.z (which is now back to a normal expr/type, not an alias), and for colons that means we have
                // x::y::z or x.y::z both of which are effectively gibberish.
                if !first_loop {
                    is_alias = false;
                }

                self.eat_token();
                result = self.scan_named_type_part_with_last_token_of_type(last_token_of_type);
                if result == ScanTypeFlags::NOT_TYPE {
                    return ScanTypeFlags::NOT_TYPE;
                }

                debug_assert!(matches!(
                    result,
                    ScanTypeFlags::GENERIC_TYPE_OR_EXPRESSION
                        | ScanTypeFlags::GENERIC_TYPE_OR_METHOD
                        | ScanTypeFlags::NON_GENERIC_TYPE_OR_EXPRESSION
                ));
                first_loop = false;
            }

            if is_alias {
                result = ScanTypeFlags::ALIAS_QUALIFIED_NAME;
            }
        } else if Self::is_predefined_type(self.current_kind()) {
            // Simple type...
            *last_token_of_type = self.eat_token();
            result = ScanTypeFlags::MUST_BE_TYPE;
        } else if self.current_kind() == SyntaxKind::OpenParenToken {
            *last_token_of_type = self.eat_token();

            result = self.scan_tuple_type(last_token_of_type);
            if result == ScanTypeFlags::NOT_TYPE
                || (mode == ParseTypeMode::DefinitePattern && self.current_kind() != SyntaxKind::OpenBracketToken)
            {
                // A tuple type can appear in a pattern only if it is the element type of an array type.
                return ScanTypeFlags::NOT_TYPE;
            }
        } else if self.is_function_pointer_start() {
            result = self.scan_function_pointer_type(last_token_of_type);
        } else {
            // Can't be a type!
            *last_token_of_type = Green::NULL;
            return ScanTypeFlags::NOT_TYPE;
        }

        let mut last_token_position = -1;
        while self.is_making_progress(&mut last_token_position, true) {
            match self.current_kind() {
                SyntaxKind::QuestionToken
                    if !matches!(
                        self.base.arena_ref().kind(*last_token_of_type),
                        SyntaxKind::QuestionToken // don't allow `Type??`
                            | SyntaxKind::AsteriskToken // don't allow `Type*?`
                    ) =>
                {
                    *last_token_of_type = self.eat_token();
                    result = ScanTypeFlags::NULLABLE_TYPE;
                }
                SyntaxKind::AsteriskToken => {
                    // Check for pointer type(s)
                    let pointer = match mode {
                        ParseTypeMode::FirstElementOfPossibleTupleLiteral | ParseTypeMode::AfterTupleComma => {
                            // We are parsing the type for a declaration expression in a tuple, which does
                            // not permit pointer types except as an element type of an array type.
                            // In that context a `*` is parsed as a multiplication.
                            self.pointer_type_mods_followed_by_rank_and_dimension_specifier()
                        }
                        // pointer type syntax is not supported in patterns.
                        ParseTypeMode::DefinitePattern => false,
                        _ => true,
                    };
                    if !pointer {
                        break; // goto done
                    }
                    *last_token_of_type = self.eat_token();
                    if matches!(
                        result,
                        ScanTypeFlags::GENERIC_TYPE_OR_EXPRESSION | ScanTypeFlags::NON_GENERIC_TYPE_OR_EXPRESSION
                    ) {
                        result = ScanTypeFlags::POINTER_OR_MULTIPLICATION;
                    } else if result == ScanTypeFlags::GENERIC_TYPE_OR_METHOD {
                        result = ScanTypeFlags::MUST_BE_TYPE;
                    }
                }
                SyntaxKind::OpenBracketToken => {
                    // Check for array types.
                    self.eat_token();
                    while self.current_kind() == SyntaxKind::CommaToken {
                        self.eat_token();
                    }

                    if self.current_kind() != SyntaxKind::CloseBracketToken {
                        *last_token_of_type = Green::NULL;
                        return ScanTypeFlags::NOT_TYPE;
                    }

                    *last_token_of_type = self.eat_token();
                    result = ScanTypeFlags::MUST_BE_TYPE;
                }
                _ => break, // goto done
            }
        }

        // done:
        result
    }

    /// `private ScanTypeFlags ScanTupleType(out SyntaxToken lastTokenOfType)` (LanguageParser.cs:7432).
    pub(crate) fn scan_tuple_type(&mut self, last_token_of_type: &mut Green) -> ScanTypeFlags {
        let mut tuple_element_type = self.scan_type_with_for_pattern(last_token_of_type, false);
        if tuple_element_type != ScanTypeFlags::NOT_TYPE {
            if self.is_true_identifier() {
                *last_token_of_type = self.eat_token();
            }

            if self.current_kind() == SyntaxKind::CommaToken {
                loop {
                    *last_token_of_type = self.eat_token();
                    tuple_element_type = self.scan_type_with_for_pattern(last_token_of_type, false);

                    if tuple_element_type == ScanTypeFlags::NOT_TYPE {
                        *last_token_of_type = self.eat_token();
                        return ScanTypeFlags::NOT_TYPE;
                    }

                    if self.is_true_identifier() {
                        *last_token_of_type = self.eat_token();
                    }

                    if self.current_kind() != SyntaxKind::CommaToken {
                        break;
                    }
                }

                if self.current_kind() == SyntaxKind::CloseParenToken {
                    *last_token_of_type = self.eat_token();
                    return ScanTypeFlags::TUPLE_TYPE;
                }
            }
        }

        // Can't be a type!
        *last_token_of_type = Green::NULL;
        ScanTypeFlags::NOT_TYPE
    }

    /// `private ScanTypeFlags ScanFunctionPointerType(out SyntaxToken lastTokenOfType)` (LanguageParser.cs:7476).
    pub(crate) fn scan_function_pointer_type(&mut self, last_token_of_type: &mut Green) -> ScanTypeFlags {
        debug_assert!(self.is_function_pointer_start());
        let _ = self.eat_token_kind(SyntaxKind::DelegateKeyword);
        *last_token_of_type = self.eat_token_kind(SyntaxKind::AsteriskToken);

        if self.current_kind() == SyntaxKind::IdentifierToken {
            let peek1 = self.peek_token(1);
            let contextual = self.current_contextual_kind();
            if matches!(contextual, SyntaxKind::ManagedKeyword | SyntaxKind::UnmanagedKeyword)
                || self.is_possible_function_pointer_parameter_list_start(peek1)
                || self.base.arena_ref().kind(peek1) == SyntaxKind::OpenBracketToken
            {
                *last_token_of_type = self.eat_token();
            } else {
                // Whatever is next, it's probably not part of the type. We know that delegate* must be
                // a function pointer start, however, so say the asterisk is the last element and bail
                return ScanTypeFlags::MUST_BE_TYPE;
            }

            if self.current_kind() == SyntaxKind::OpenBracketToken {
                *last_token_of_type = self.eat_token_kind(SyntaxKind::OpenBracketToken);
                let save_term = self.term_state;
                self.term_state = self.term_state | TerminatorState::IS_END_OF_FUNCTION_POINTER_CALLING_CONVENTION;

                loop {
                    *last_token_of_type = self.try_eat_token(SyntaxKind::IdentifierToken).or(*last_token_of_type);

                    if self.scan_function_pointer_type_skip_bad_function_pointer_tokens() == PostSkipAction::Abort {
                        break;
                    }

                    debug_assert!(self.current_kind() == SyntaxKind::CommaToken);
                    *last_token_of_type = self.eat_token();
                }

                *last_token_of_type = self.try_eat_token(SyntaxKind::CloseBracketToken).or(*last_token_of_type);
                self.term_state = save_term;
            }
        }

        let current = self.current_token();
        if !self.is_possible_function_pointer_parameter_list_start(current) {
            // Even though this function pointer type is incomplete, we know that it
            // must be the start of a type, as there is no other possible interpretation
            // of delegate*. By always treating it as a type, we ensure that any disambiguation
            // done in later parsing treats this as a type, which will produce better
            // errors at later stages.
            return ScanTypeFlags::MUST_BE_TYPE;
        }

        let start = self.eat_token();
        let valid_starting_token = self.base.arena_ref().kind(start) == SyntaxKind::LessThanToken;

        let save_term = self.term_state;
        self.term_state = self.term_state
            | if valid_starting_token {
                TerminatorState::IS_END_OF_FUNCTION_POINTER_PARAMETER_LIST
            } else {
                TerminatorState::IS_END_OF_FUNCTION_POINTER_PARAMETER_LIST_ERRORED
            };
        let mut ignored_modifiers = self.pool.allocate();

        loop {
            self.parse_parameter_modifiers(&mut ignored_modifiers, true, false);
            ignored_modifiers.clear();

            let mut unused = Green::NULL;
            let _ = self.scan_type_with_for_pattern(&mut unused, false);

            if self.scan_function_pointer_type_skip_bad_function_pointer_tokens() == PostSkipAction::Abort {
                break;
            }

            let _ = self.eat_token_kind(SyntaxKind::CommaToken);
        }

        self.term_state = save_term;
        self.pool.free(ignored_modifiers);

        if !valid_starting_token && self.current_kind() == SyntaxKind::CloseParenToken {
            *last_token_of_type = self.eat_token_as_kind(SyntaxKind::GreaterThanToken);
        } else {
            *last_token_of_type = self.eat_token_kind(SyntaxKind::GreaterThanToken);
        }

        ScanTypeFlags::MUST_BE_TYPE
    }

    /// `skipBadFunctionPointerTokens` (a local function of `ScanFunctionPointerType`, LanguageParser.cs:7583).
    fn scan_function_pointer_type_skip_bad_function_pointer_tokens(&mut self) -> PostSkipAction {
        let mut unused = Green::NULL;
        self.skip_bad_tokens_with_expected_kind(
            |p| p.current_kind() != SyntaxKind::CommaToken,
            |p, _| p.is_terminator(),
            SyntaxKind::CommaToken,
            SyntaxKind::None,
            &mut unused,
        )
    }

    /// `private static bool IsPredefinedType(SyntaxKind keyword)` (LanguageParser.cs:7595).
    pub(crate) fn is_predefined_type(keyword: SyntaxKind) -> bool {
        facts::is_predefined_type(keyword)
    }

    /// `public TypeSyntax ParseTypeName()` (LanguageParser.cs:7600).
    pub(crate) fn parse_type_name(&mut self) -> Green {
        self.parse_type(ParseTypeMode::Normal)
    }

    /// `private TypeSyntax ParseTypeOrVoid()` (LanguageParser.cs:7605).
    pub(crate) fn parse_type_or_void(&mut self) -> Green {
        if self.current_kind() == SyntaxKind::VoidKeyword && self.peek_kind(1) != SyntaxKind::AsteriskToken {
            // Must be 'void' type, so create such a type node and return it.
            let token = self.eat_token();
            return self.base.arena().predefined_type(token);
        }

        self.parse_type(ParseTypeMode::Normal)
    }
}

/// `ParseTypeMode` (LanguageParser.cs:7616).
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub(crate) enum ParseTypeMode {
    #[default]
    Normal,
    Parameter,
    AfterIs,
    DefinitePattern,
    AfterOut,
    AfterRef,
    AfterTupleComma,
    AsExpression,
    NewExpression,
    FirstElementOfPossibleTupleLiteral,
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private TypeSyntax ParseType(ParseTypeMode mode = ParseTypeMode.Normal)` (LanguageParser.cs:7630).
    ///
    /// Not in Roslyn: counts `_recursionDepth` (Roslyn leaves the recursion through type argument lists, tuple and
    /// function pointer types unguarded), so a parse fits a 1 MB stack.
    pub(crate) fn parse_type(&mut self, mode: ParseTypeMode) -> Green {
        self.recursion_depth += 1;
        self.ensure_sufficient_execution_stack();
        let result = if self.current_kind() == SyntaxKind::RefKeyword {
            let ref_keyword = self.eat_token();
            let read_only_keyword =
                if self.current_kind() == SyntaxKind::ReadOnlyKeyword { self.eat_token() } else { Green::NULL };
            let type_ = self.parse_type_core(ParseTypeMode::AfterRef);
            self.base.arena().ref_type(ref_keyword, read_only_keyword, type_)
        } else {
            self.parse_type_core(mode)
        };
        self.recursion_depth -= 1;
        result
    }

    /// `private TypeSyntax ParseTypeCore(ParseTypeMode mode)` (LanguageParser.cs:7643).
    pub(crate) fn parse_type_core(&mut self, mode: ParseTypeMode) -> Green {
        let name_options = match mode {
            ParseTypeMode::AfterIs => {
                NameOptions::IN_EXPRESSION | NameOptions::AFTER_IS | NameOptions::POSSIBLE_PATTERN
            }
            ParseTypeMode::DefinitePattern => {
                NameOptions::IN_EXPRESSION | NameOptions::DEFINITE_PATTERN | NameOptions::POSSIBLE_PATTERN
            }
            ParseTypeMode::AfterOut => NameOptions::IN_EXPRESSION | NameOptions::AFTER_OUT,
            ParseTypeMode::AfterTupleComma => NameOptions::IN_EXPRESSION | NameOptions::AFTER_TUPLE_COMMA,
            ParseTypeMode::FirstElementOfPossibleTupleLiteral => {
                NameOptions::IN_EXPRESSION | NameOptions::FIRST_ELEMENT_OF_POSSIBLE_TUPLE_LITERAL
            }
            ParseTypeMode::NewExpression
            | ParseTypeMode::AsExpression
            | ParseTypeMode::Normal
            | ParseTypeMode::Parameter
            | ParseTypeMode::AfterRef => NameOptions::NONE,
        };

        let mut type_ = self.parse_underlying_type(mode, name_options);
        debug_assert!(!type_.is_null());

        let mut last_token_position = -1;
        while self.is_making_progress(&mut last_token_position, true) {
            match self.current_kind() {
                SyntaxKind::QuestionToken => {
                    let question = self.try_eat_nullable_qualifier_if_applicable(type_, mode);
                    if !question.is_null() {
                        type_ = self.base.arena().nullable_type(type_, question);
                        continue;
                    }

                    // token not consumed
                }
                SyntaxKind::AsteriskToken => match mode {
                    ParseTypeMode::AfterIs
                    | ParseTypeMode::DefinitePattern
                    | ParseTypeMode::AfterTupleComma
                    | ParseTypeMode::FirstElementOfPossibleTupleLiteral => {
                        // these contexts do not permit a pointer type except as an element type of an array.
                        if self.pointer_type_mods_followed_by_rank_and_dimension_specifier() {
                            type_ = self.parse_pointer_type_mods(type_);
                            continue;
                        }
                    }
                    ParseTypeMode::Normal
                    | ParseTypeMode::Parameter
                    | ParseTypeMode::AfterOut
                    | ParseTypeMode::AfterRef
                    | ParseTypeMode::AsExpression
                    | ParseTypeMode::NewExpression => {
                        type_ = self.parse_pointer_type_mods(type_);
                        continue;
                    }
                },
                SyntaxKind::OpenBracketToken => {
                    // Now check for arrays.
                    let mut ranks = self.pool.allocate();
                    loop {
                        let mut unused = false;
                        let rank = self.parse_array_rank_specifier(&mut unused);
                        ranks.push(rank);
                        if self.current_kind() != SyntaxKind::OpenBracketToken {
                            break;
                        }
                    }

                    let ranks = self.pool.to_list_and_free(self.base.lexer.arena(), ranks);
                    type_ = self.base.arena().array_type(type_, ranks);
                    continue;
                }
                _ => {
                    // token not consumed
                }
            }

            // token not consumed
            break;
        }

        debug_assert!(!type_.is_null());
        type_
    }

    /// `private SyntaxToken TryEatNullableQualifierIfApplicable( TypeSyntax typeParsedSoFar, ParseTypeMode mode)` (LanguageParser.cs:7746).
    pub(crate) fn try_eat_nullable_qualifier_if_applicable(
        &mut self,
        type_parsed_so_far: Green,
        mode: ParseTypeMode,
    ) -> Green {
        debug_assert!(self.current_kind() == SyntaxKind::QuestionToken);

        // These are the fast tests for (in)applicability. More expensive tests are follow.
        //
        // If we already have `x?` or `x*` then do not parse out a nullable type if we see `x??` or `x*?`.  These
        // are never legal as types in the language, so we can fast bail out.
        if matches!(self.base.arena_ref().kind(type_parsed_so_far), SyntaxKind::NullableType | SyntaxKind::PointerType)
        {
            return Green::NULL;
        }

        let mut outer_reset_point = self.get_disposable_reset_point(false);

        let question_token = self.eat_token();
        if !self.try_eat_nullable_qualifier_if_applicable_can_follow_nullable_type(mode) {
            // Restore current token index
            self.reset_disposable(&mut outer_reset_point);
            self.dispose_reset_point(outer_reset_point);
            return Green::NULL;
        }

        self.dispose_reset_point(outer_reset_point);
        question_token
    }

    /// `canFollowNullableType` (a local function of `TryEatNullableQualifierIfApplicable`, LanguageParser.cs:7770).
    fn try_eat_nullable_qualifier_if_applicable_can_follow_nullable_type(&mut self, mode: ParseTypeMode) -> bool {
        if mode == ParseTypeMode::AfterIs && self.current_kind() == SyntaxKind::OpenBracketToken {
            // T?[
            //
            // This could be a array of nullable types (e.g. `is T?[]` or `is T?[,]`) or it's a
            // conditional with a collection expression or lambda (e.g. `is T ? [...] :` or `is T ? [Attr]() => ...`)
            //
            // Note: `is T?[]` could be the start of either.  So we have to look to see if we have a
            // `:` to know which case we're in.

            return match self.peek_kind(1) {
                // `is T?[,]`.  Definitely an array of nullable type.
                SyntaxKind::CommaToken => true,

                // `is T?[]`.  Could be an array of a nullable type, or a conditional.  Have to
                // see if it is followed by `:` to find out.  If there is a colon, it's a
                // conditional.
                SyntaxKind::CloseBracketToken => {
                    let reset_point = self.get_disposable_reset_point(true);

                    // Consume the expression after the `?`.
                    let _when_true = self.parse_possible_ref_expression();

                    // Now see if we have a ':' following.  If so, this is a conditional.  If not, it's a nullable type.
                    let result = self.current_kind() != SyntaxKind::ColonToken;
                    self.dispose_reset_point(reset_point);
                    result
                }

                // `is T ? [...`.  Not an array.  This is a conditional with a collection expr
                // or attributed lambda.
                _ => false,
            };
        }

        match mode {
            ParseTypeMode::AfterIs | ParseTypeMode::DefinitePattern | ParseTypeMode::AsExpression => {
                // We are currently after `?` token after a nullable type pattern and need to decide how to
                // parse what we see next.  In the case of an identifier (e.g. `x ? a` there are two ways we can
                // see things
                //
                // 1. As a start of conditional expression, e.g. `var a = obj is string ? a : b`
                // 2. As a designation of a nullable-typed pattern, e.g. `if (obj is string? str)`
                //
                // Since nullable types (no matter reference or value types) are not valid in patterns by
                // default we are biased towards the first option and consider case 2 only for error recovery
                // purposes (if we parse here as nullable type pattern an error will be reported during
                // binding). This condition checks for simple cases, where we better use option 2 and parse a
                // nullable-typed pattern
                let current = self.current_token();
                if self.is_true_identifier_with_token(current) {
                    // 1. `async` can start a simple lambda in a conditional expression
                    // (e.g. `x is Y ? async a => ...`). The correct behavior is to treat `async` as a keyword
                    // 2. In a non-async method, `await` is a simple identifier.  However, if we see `x ? await`
                    // it's almost certainly the start of an `await expression` in a conditional expression
                    // (e.g. `x is Y ? await ...`), not a nullable type pattern (since users would not use
                    // 'await' as the name of a variable).  So just treat this as a conditional expression.
                    // 3. `from` most likely starts a linq query: `x is Y ? from item in collection select item : ...`
                    if matches!(
                        self.current_contextual_kind(),
                        SyntaxKind::AsyncKeyword | SyntaxKind::AwaitKeyword | SyntaxKind::FromKeyword
                    ) {
                        return false;
                    }

                    let next_token = self.peek_token(1);

                    // Cases like `x is Y ? someRecord with { } : ...`
                    if self.base.arena_ref().contextual_kind(next_token) == SyntaxKind::WithKeyword {
                        return false;
                    }

                    let next_token_kind = self.base.arena_ref().kind(next_token);

                    // These token either 100% end a pattern or start a new one:

                    // A literal token starts a new pattern. Can occur in list pattern with missing separation
                    // `,`.  For example, in `x is [int[]? arr 5]` we'd prefer to parse this as a missing `,`
                    // after `arr`
                    if facts::is_literal(next_token_kind) {
                        return true;
                    }

                    // A predefined type is basically the same case: `x is [string[]? slice char ch]`. We'd
                    // prefer to parse this as a missing `,` after `slice`.
                    if facts::is_predefined_type(next_token_kind) {
                        return true;
                    }

                    // `)`, `]` and `}` obviously end a pattern.  For example:
                    // `if (x is int? i)`, `indexable[x is string? s]`, `x is { Prop: Type? typeVar }`
                    if matches!(
                        next_token_kind,
                        SyntaxKind::CloseParenToken | SyntaxKind::CloseBracketToken | SyntaxKind::CloseBraceToken
                    ) {
                        return true;
                    }

                    // `{` starts a new pattern.  For example: `x is A? { ...`. Note, that `[` and `(` are not
                    // in the list because they can start an invocation/indexer
                    if next_token_kind == SyntaxKind::OpenBraceToken {
                        return true;
                    }

                    // `,` ends a pattern in list/property pattern.  For example `x is { Prop1: Type1? type, Prop2: Type2 }` or
                    // `x is [Type1? type, ...]`
                    if next_token_kind == SyntaxKind::CommaToken {
                        return true;
                    }

                    // `;` ends a pattern if it finishes an expression statement: var y = x is bool? b;
                    if next_token_kind == SyntaxKind::SemicolonToken {
                        return true;
                    }

                    // EndOfFileToken is obviously the end of parsing. We are better parsing a pattern rather
                    // than an unfinished conditional expression
                    if next_token_kind == SyntaxKind::EndOfFileToken {
                        return true;
                    }

                    return false;
                }

                // If nothing from above worked permit the nullable qualifier if it is followed by a token that
                // could not start an expression. If we have `T?[]` we do want to treat that as an array of
                // nullables (following existing parsing), not a conditional that returns a list.
                if self.current_kind() == SyntaxKind::OpenBracketToken {
                    return true;
                }

                !self.can_start_expression()
            }
            ParseTypeMode::NewExpression => {
                // A nullable qualifier is permitted as part of the type in a `new` expression. e.g. `new
                // int?()` is allowed.  It creates a null value of type `Nullable<int>`. Similarly `new int? {}`
                // is allowed.
                matches!(
                    self.current_kind(),
                    SyntaxKind::OpenParenToken // ctor parameters
                        | SyntaxKind::OpenBracketToken // array type
                        | SyntaxKind::OpenBraceToken // object initializer
                )
            }
            _ => true,
        }
    }

    /// `private bool PointerTypeModsFollowedByRankAndDimensionSpecifier()` (LanguageParser.cs:7907).
    pub(crate) fn pointer_type_mods_followed_by_rank_and_dimension_specifier(&mut self) -> bool {
        // Are pointer specifiers (if any) followed by an array specifier?
        let mut i = 0;
        loop {
            match self.peek_kind(i) {
                SyntaxKind::AsteriskToken => {}
                SyntaxKind::OpenBracketToken => return true,
                _ => return false,
            }
            i += 1;
        }
    }

    /// `private ArrayRankSpecifierSyntax ParseArrayRankSpecifier(out bool sawNonOmittedSize)` (LanguageParser.cs:7924).
    pub(crate) fn parse_array_rank_specifier(&mut self, saw_non_omitted_size: &mut bool) -> Green {
        *saw_non_omitted_size = false;
        let mut saw_omitted_size = false;
        let mut open = self.eat_token_kind(SyntaxKind::OpenBracketToken);
        let mut list = self.pool.allocate();

        let a = self.base.arena();
        let token = a.token(SyntaxKind::OmittedArraySizeExpressionToken);
        let omitted_array_size_expression_instance = a.omitted_array_size_expression(token);
        let mut last_token_position = -1;
        while self.is_making_progress(&mut last_token_position, true)
            && self.current_kind() != SyntaxKind::CloseBracketToken
        {
            if self.current_kind() == SyntaxKind::CommaToken {
                // NOTE: trivia will be attached to comma, not omitted array size
                saw_omitted_size = true;
                list.push(omitted_array_size_expression_instance);
                let comma = self.eat_token();
                list.push(comma);
            } else if self.is_possible_expression() {
                let size = self.parse_expression_core();
                *saw_non_omitted_size = true;
                list.push(size);

                if self.current_kind() != SyntaxKind::CloseBracketToken {
                    let comma = self.eat_token_kind(SyntaxKind::CommaToken);
                    list.push(comma);
                }
            } else if self.skip_bad_array_rank_specifier_tokens(&mut open, &mut list, SyntaxKind::CommaToken)
                == PostSkipAction::Abort
            {
                break;
            }
        }

        // Don't end on a comma.
        // If the omitted size would be the only element, then skip it unless sizes were expected.
        if list.len() & 1 == 0 {
            saw_omitted_size = true;
            list.push(omitted_array_size_expression_instance);
        }

        // Never mix omitted and non-omitted array sizes.  If there were non-omitted array sizes,
        // then convert all of the omitted array sizes to missing identifiers.
        if saw_omitted_size && *saw_non_omitted_size {
            for item in list.iter_mut() {
                if self.base.arena_ref().kind(*item) == SyntaxKind::OmittedArraySizeExpression {
                    let width = self.base.arena_ref().width(*item) as i32;
                    let missing = self.create_missing_identifier_name();
                    *item = self.add_error_at(missing, 0, width, ErrorCode::ERR_ValueExpected, &[]);
                }
            }
        }

        let sizes = self.pool.to_list_and_free(self.base.lexer.arena(), list);
        let close = self.eat_token_kind(SyntaxKind::CloseBracketToken);
        self.base.arena().array_rank_specifier(open, sizes, close)
    }

    /// `private TupleTypeSyntax ParseTupleType()` (LanguageParser.cs:7984).
    pub(crate) fn parse_tuple_type(&mut self) -> Green {
        let open = self.eat_token_kind(SyntaxKind::OpenParenToken);
        let mut list = self.pool.allocate();

        if self.current_kind() != SyntaxKind::CloseParenToken {
            let element = self.parse_tuple_element();
            list.push(element);

            while self.current_kind() == SyntaxKind::CommaToken {
                let comma = self.eat_token_kind(SyntaxKind::CommaToken);
                list.push(comma);
                let element = self.parse_tuple_element();
                list.push(element);
            }
        }

        // `list.Count`: the elements, not the separators.
        if list.len().div_ceil(2) < 2 {
            if list.is_empty() {
                let missing = self.create_missing_identifier_name();
                let element = self.base.arena().tuple_element(missing, Green::NULL);
                list.push(element);
            }

            let comma = self.base.arena().missing_token(SyntaxKind::CommaToken);
            list.push(comma);
            let missing = self.create_missing_identifier_name();
            let missing = self.add_error(missing, ErrorCode::ERR_TupleTooFewElements);
            let element = self.base.arena().tuple_element(missing, Green::NULL);
            list.push(element);
        }

        let elements = self.pool.to_list_and_free(self.base.lexer.arena(), list);
        let close = self.eat_token_kind(SyntaxKind::CloseParenToken);
        self.base.arena().tuple_type(open, elements, close)
    }

    /// `private TupleElementSyntax ParseTupleElement()` (LanguageParser.cs:8018).
    pub(crate) fn parse_tuple_element(&mut self) -> Green {
        let type_ = self.parse_type(ParseTypeMode::Normal);
        let identifier = if self.is_true_identifier() {
            self.parse_identifier_token(ErrorCode::ERR_IdentifierExpected)
        } else {
            Green::NULL
        };
        self.base.arena().tuple_element(type_, identifier)
    }

    /// `private PostSkipAction SkipBadArrayRankSpecifierTokens(ref SyntaxToken openBracket, SeparatedSyntaxListBuilder<ExpressionSyntax> list, SyntaxKind expected)` (LanguageParser.cs:8025).
    pub(crate) fn skip_bad_array_rank_specifier_tokens(
        &mut self,
        open_bracket: &mut Green,
        list: &mut SyntaxListBuilder,
        expected: SyntaxKind,
    ) -> PostSkipAction {
        self.skip_bad_separated_list_tokens_with_expected_kind(
            open_bracket,
            list,
            |p| p.current_kind() != SyntaxKind::CommaToken && !p.is_possible_expression(),
            |p, _| p.current_kind() == SyntaxKind::CloseBracketToken,
            expected,
            SyntaxKind::None,
        )
    }

    /// `private TypeSyntax ParseUnderlyingType(ParseTypeMode mode, NameOptions options = NameOptions.None)` (LanguageParser.cs:8033).
    pub(crate) fn parse_underlying_type(&mut self, mode: ParseTypeMode, options: NameOptions) -> Green {
        if Self::is_predefined_type(self.current_kind()) {
            // This is a predefined type
            let mut token = self.eat_token();
            if self.base.arena_ref().kind(token) == SyntaxKind::VoidKeyword
                && self.current_kind() != SyntaxKind::AsteriskToken
            {
                token = self.add_error(
                    token,
                    if mode == ParseTypeMode::Parameter {
                        ErrorCode::ERR_NoVoidParameter
                    } else {
                        ErrorCode::ERR_NoVoidHere
                    },
                );
            }

            return self.base.arena().predefined_type(token);
        }

        // The :: case is for error recovery.
        if self.is_true_identifier() || self.current_kind() == SyntaxKind::ColonColonToken {
            return self.parse_qualified_name(options);
        }

        if self.current_kind() == SyntaxKind::OpenParenToken {
            return self.parse_tuple_type();
        } else if self.is_function_pointer_start() {
            return self.parse_function_pointer_type_syntax();
        }

        let missing = self.create_missing_identifier_name();
        self.add_error(
            missing,
            if mode == ParseTypeMode::NewExpression { ErrorCode::ERR_BadNewExpr } else { ErrorCode::ERR_TypeExpected },
        )
    }

    /// `private FunctionPointerTypeSyntax ParseFunctionPointerTypeSyntax()` (LanguageParser.cs:8068).
    pub(crate) fn parse_function_pointer_type_syntax(&mut self) -> Green {
        debug_assert!(self.is_function_pointer_start());
        let delegate = self.eat_token_kind(SyntaxKind::DelegateKeyword);
        let asterisk = self.eat_token_kind(SyntaxKind::AsteriskToken);

        let calling_convention = self.parse_function_pointer_type_syntax_parse_calling_convention();

        let current = self.current_token();
        if !self.is_possible_function_pointer_parameter_list_start(current) {
            let less_than_token_error = self.create_missing_token(SyntaxKind::LessThanToken, SyntaxKind::None);

            let mut missing_types = self.pool.allocate();
            let missing = self.create_missing_identifier_name();
            let missing_type = self.base.arena().function_pointer_parameter(Green::NULL, Green::NULL, missing);
            missing_types.push(missing_type);

            // Handle the simple case of delegate*>. We don't try to deal with any variation of delegate*invalid>, as
            // we don't know for sure that the expression isn't a relational with something else.
            let parameters = self.pool.to_list_and_free(self.base.lexer.arena(), missing_types);
            let mut greater = self.try_eat_token(SyntaxKind::GreaterThanToken);
            if greater.is_null() {
                greater = self.base.arena().missing_token(SyntaxKind::GreaterThanToken);
            }
            let a = self.base.arena();
            let parameter_list = a.function_pointer_parameter_list(less_than_token_error, parameters, greater);
            return a.function_pointer_type(delegate, asterisk, calling_convention, parameter_list);
        }

        let less_than_token = self.eat_token_as_kind(SyntaxKind::LessThanToken);
        let less_than_missing = self.base.arena_ref().is_missing(less_than_token);
        let save_term = self.term_state;
        self.term_state = self.term_state
            | if less_than_missing {
                TerminatorState::IS_END_OF_FUNCTION_POINTER_PARAMETER_LIST_ERRORED
            } else {
                TerminatorState::IS_END_OF_FUNCTION_POINTER_PARAMETER_LIST
            };
        let mut types = self.pool.allocate();

        loop {
            let mut modifiers = self.pool.allocate();

            self.parse_parameter_modifiers(&mut modifiers, true, false);

            let modifiers = self.pool.to_list_and_free(self.base.lexer.arena(), modifiers);
            let type_ = self.parse_type_or_void();
            let parameter = self.base.arena().function_pointer_parameter(Green::NULL, modifiers, type_);
            types.push(parameter);

            if self.parse_function_pointer_type_syntax_skip_bad_function_pointer_tokens(&mut types)
                == PostSkipAction::Abort
            {
                break;
            }

            debug_assert!(self.current_kind() == SyntaxKind::CommaToken);
            let comma = self.eat_token_kind(SyntaxKind::CommaToken);
            types.push(comma);
        }

        let parameters = self.pool.to_list_and_free(self.base.lexer.arena(), types);
        let greater = if less_than_missing && self.current_kind() == SyntaxKind::CloseParenToken {
            self.eat_token_as_kind(SyntaxKind::GreaterThanToken)
        } else {
            self.eat_token_kind(SyntaxKind::GreaterThanToken)
        };
        let a = self.base.arena();
        let parameter_list = a.function_pointer_parameter_list(less_than_token, parameters, greater);
        let result = a.function_pointer_type(delegate, asterisk, calling_convention, parameter_list);
        self.term_state = save_term;
        result
    }

    /// `skipBadFunctionPointerTokens<T>` (a local function of `ParseFunctionPointerTypeSyntax`,
    /// LanguageParser.cs:8139).
    fn parse_function_pointer_type_syntax_skip_bad_function_pointer_tokens(
        &mut self,
        list: &mut SyntaxListBuilder,
    ) -> PostSkipAction {
        let mut tmp = Green::NULL;
        debug_assert!(!list.is_empty());
        self.skip_bad_separated_list_tokens_with_expected_kind(
            &mut tmp,
            list,
            |p| p.current_kind() != SyntaxKind::CommaToken,
            // this.IsTerminator() (called by our caller) is the only thing that aborts parsing.
            |_, _| false,
            SyntaxKind::CommaToken,
            SyntaxKind::None,
        )
    }

    /// `parseCallingConvention` (a local function of `ParseFunctionPointerTypeSyntax`, LanguageParser.cs:8151).
    fn parse_function_pointer_type_syntax_parse_calling_convention(&mut self) -> Green {
        if self.current_kind() == SyntaxKind::IdentifierToken {
            #[allow(clippy::needless_late_init, reason = "the C# declares it, then assigns it per case")]
            let managed_specifier;
            let peek1 = self.peek_token(1);
            let contextual = self.current_contextual_kind();
            if matches!(contextual, SyntaxKind::ManagedKeyword | SyntaxKind::UnmanagedKeyword) {
                managed_specifier = self.eat_contextual_token(contextual);
            } else if self.is_possible_function_pointer_parameter_list_start(peek1) {
                // If there's a possible parameter list next, treat this as a bad identifier that should have been managed or unmanaged
                managed_specifier = self.eat_token_as_kind(SyntaxKind::ManagedKeyword);
            } else if self.base.arena_ref().kind(peek1) == SyntaxKind::OpenBracketToken {
                // If there's an open brace next, treat this as a bad identifier that should have been unmanaged
                managed_specifier = self.eat_token_as_kind(SyntaxKind::UnmanagedKeyword);
            } else {
                // Whatever is next, it's probably not a calling convention or a function pointer type.
                // Bail out
                return Green::NULL;
            }

            let mut unmanaged_calling_conventions = Green::NULL;
            if self.current_kind() == SyntaxKind::OpenBracketToken {
                let open_bracket = self.eat_token_kind(SyntaxKind::OpenBracketToken);
                let mut calling_convention_modifiers = self.pool.allocate();
                let save_term = self.term_state;
                self.term_state = self.term_state | TerminatorState::IS_END_OF_FUNCTION_POINTER_CALLING_CONVENTION;

                loop {
                    let name = self.eat_token_kind(SyntaxKind::IdentifierToken);
                    let convention = self.base.arena().function_pointer_unmanaged_calling_convention(name);
                    calling_convention_modifiers.push(convention);

                    if self.parse_function_pointer_type_syntax_skip_bad_function_pointer_tokens(
                        &mut calling_convention_modifiers,
                    ) == PostSkipAction::Abort
                    {
                        break;
                    }

                    debug_assert!(self.current_kind() == SyntaxKind::CommaToken);
                    let comma = self.eat_token_kind(SyntaxKind::CommaToken);
                    calling_convention_modifiers.push(comma);
                }

                let close_bracket = self.eat_token_kind(SyntaxKind::CloseBracketToken);

                let conventions = self.pool.to_list_and_free(self.base.lexer.arena(), calling_convention_modifiers);
                unmanaged_calling_conventions = self.base.arena().function_pointer_unmanaged_calling_convention_list(
                    open_bracket,
                    conventions,
                    close_bracket,
                );
                self.term_state = save_term;
            }

            if self.base.arena_ref().kind(managed_specifier) == SyntaxKind::ManagedKeyword
                && !unmanaged_calling_conventions.is_null()
            {
                // 'managed' calling convention cannot be combined with unmanaged calling convention specifiers.
                unmanaged_calling_conventions = self.add_error(
                    unmanaged_calling_conventions,
                    ErrorCode::ERR_CannotSpecifyManagedWithUnmanagedSpecifiers,
                );
            }

            return self
                .base
                .arena()
                .function_pointer_calling_convention(managed_specifier, unmanaged_calling_conventions);
        }

        Green::NULL
    }

    /// `private bool IsFunctionPointerStart()` (LanguageParser.cs:8228).
    pub(crate) fn is_function_pointer_start(&mut self) -> bool {
        self.current_kind() == SyntaxKind::DelegateKeyword && self.peek_kind(1) == SyntaxKind::AsteriskToken
    }

    /// `private static bool IsPossibleFunctionPointerParameterListStart(SyntaxToken token)` (LanguageParser.cs:8231).
    pub(crate) fn is_possible_function_pointer_parameter_list_start(&self, token: Green) -> bool {
        // We consider both ( and < to be possible starts, in order to make error recovery more graceful
        // in the scenario where a user accidentally surrounds their function pointer type list with parens.
        matches!(self.base.arena_ref().kind(token), SyntaxKind::LessThanToken | SyntaxKind::OpenParenToken)
    }

    /// `private TypeSyntax ParsePointerTypeMods(TypeSyntax type)` (LanguageParser.cs:8237).
    pub(crate) fn parse_pointer_type_mods(&mut self, type_: Green) -> Green {
        let mut type_ = type_;
        // Check for pointer types
        while self.current_kind() == SyntaxKind::AsteriskToken {
            let asterisk = self.eat_token();
            type_ = self.base.arena().pointer_type(type_, asterisk);
        }

        type_
    }
}
