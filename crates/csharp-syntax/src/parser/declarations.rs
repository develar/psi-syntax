//! `LanguageParser.cs` 236-6162: namespaces, extern aliases and usings, attributes, type and member declarations,
//! modifiers, accessors, parameter lists, fields, events, variable declarators, enums and names (lane A). Every method in C# order; see `parser/mod.rs` for the conventions.

use super::*;
use crate::syntax::types as T;

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private BaseNamespaceDeclarationSyntax ParseNamespaceDeclaration( SyntaxList<AttributeListSyntax> attributeLists, SyntaxListBuilder modifiers)` (LanguageParser.cs:236).
    pub(crate) fn parse_namespace_declaration(
        &mut self,
        attribute_lists: Green,
        modifiers: &mut SyntaxListBuilder,
    ) -> Green {
        self.recursion_depth += 1;
        self.ensure_sufficient_execution_stack();
        let result = self.parse_namespace_declaration_core(attribute_lists, modifiers);
        self.recursion_depth -= 1;
        result
    }

    /// `private BaseNamespaceDeclarationSyntax ParseNamespaceDeclarationCore( SyntaxList<AttributeListSyntax> attributeLists, SyntaxListBuilder modifiers)` (LanguageParser.cs:247).
    pub(crate) fn parse_namespace_declaration_core(
        &mut self,
        attribute_lists: Green,
        modifiers: &mut SyntaxListBuilder,
    ) -> Green {
        debug_assert!(self.current_kind() == SyntaxKind::NamespaceKeyword);
        let namespace_token = self.eat_token_kind(SyntaxKind::NamespaceKeyword);

        // IsScript: never (ERR_NamespaceNotAllowedInScript).

        let name = self.parse_qualified_name(NameOptions::NONE);

        let mut open_brace = Green::NULL;
        let mut semicolon = Green::NULL;

        if self.current_kind() == SyntaxKind::SemicolonToken {
            semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
        } else if self.current_kind() == SyntaxKind::OpenBraceToken || self.is_possible_namespace_member_declaration() {
            //either we see the brace we expect here or we see something that could come after a brace
            //so we insert a missing one
            open_brace = self.eat_token_kind(SyntaxKind::OpenBraceToken);
        } else {
            //the next character is neither the brace we expect, nor a token that could follow the expected
            //brace so we assume it's a mistake and replace it with a missing brace
            let token = self.eat_token_even_with_incorrect_kind(SyntaxKind::OpenBraceToken);
            open_brace = self.convert_to_missing_with_trailing_trivia(token, SyntaxKind::OpenBraceToken);
        }

        debug_assert!(!semicolon.is_null() || !open_brace.is_null());

        let mut body = NamespaceBodyBuilder::new(&mut self.pool);
        let result = if open_brace.is_null() {
            debug_assert!(!semicolon.is_null());

            let mut initial_bad_nodes: Option<SyntaxListBuilder> = None;
            self.parse_namespace_body(
                &mut semicolon,
                &mut body,
                &mut initial_bad_nodes,
                SyntaxKind::FileScopedNamespaceDeclaration,
            );
            debug_assert!(initial_bad_nodes.is_none()); // init bad nodes should have been attached to semicolon...
            if let Some(bad) = initial_bad_nodes {
                self.pool.free(bad);
            }

            let a = self.base.arena();
            let modifiers = a.list(modifiers);
            let externs = a.list(&body.externs);
            let usings = a.list(&body.usings);
            let members = a.list(&body.members);
            a.file_scoped_namespace_declaration(
                attribute_lists,
                modifiers,
                namespace_token,
                name,
                semicolon,
                externs,
                usings,
                members,
            )
        } else {
            let mut initial_bad_nodes: Option<SyntaxListBuilder> = None;
            self.parse_namespace_body(
                &mut open_brace,
                &mut body,
                &mut initial_bad_nodes,
                SyntaxKind::NamespaceDeclaration,
            );
            debug_assert!(initial_bad_nodes.is_none()); // init bad nodes should have been attached to open brace...
            if let Some(bad) = initial_bad_nodes {
                self.pool.free(bad);
            }

            let close_brace = self.eat_token_kind(SyntaxKind::CloseBraceToken);
            let semicolon = self.try_eat_token(SyntaxKind::SemicolonToken);
            let a = self.base.arena();
            let modifiers = a.list(modifiers);
            let externs = a.list(&body.externs);
            let usings = a.list(&body.usings);
            let members = a.list(&body.members);
            a.namespace_declaration(
                attribute_lists,
                modifiers,
                namespace_token,
                name,
                open_brace,
                externs,
                usings,
                members,
                close_brace,
                semicolon,
            )
        };
        body.free(&mut self.pool);
        result
    }

    /// `private static bool IsPossibleStartOfTypeDeclaration(SyntaxKind kind)` (LanguageParser.cs:331).
    pub(crate) fn is_possible_start_of_type_declaration(kind: SyntaxKind) -> bool {
        Self::is_type_modifier_or_type_keyword(kind) || kind == SyntaxKind::OpenBracketToken
    }

    /// `private static bool IsTypeModifierOrTypeKeyword(SyntaxKind kind)` (LanguageParser.cs:337).
    pub(crate) fn is_type_modifier_or_type_keyword(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::EnumKeyword
                | SyntaxKind::DelegateKeyword
                | SyntaxKind::ClassKeyword
                | SyntaxKind::InterfaceKeyword
                | SyntaxKind::StructKeyword
                | SyntaxKind::AbstractKeyword
                | SyntaxKind::InternalKeyword
                | SyntaxKind::NewKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::PublicKeyword
                | SyntaxKind::SealedKeyword
                | SyntaxKind::StaticKeyword
                | SyntaxKind::UnsafeKeyword
        )
    }

    /// `private void AddSkippedNamespaceText( ref SyntaxToken? openBraceOrSemicolon, ref NamespaceBodyBuilder body, ref SyntaxListBuilder? initialBadNodes, CSharpSyntaxNode skippedSyntax)` (LanguageParser.cs:361).
    pub(crate) fn add_skipped_namespace_text(
        &mut self,
        open_brace_or_semicolon: &mut Green,
        body: &mut NamespaceBodyBuilder,
        initial_bad_nodes: &mut Option<SyntaxListBuilder>,
        skipped_syntax: Green,
    ) {
        if !body.members.is_empty() {
            self.add_trailing_skipped_syntax_to_list(&mut body.members, skipped_syntax);
        } else if !body.attributes.is_empty() {
            self.add_trailing_skipped_syntax_to_list(&mut body.attributes, skipped_syntax);
        } else if !body.usings.is_empty() {
            self.add_trailing_skipped_syntax_to_list(&mut body.usings, skipped_syntax);
        } else if !body.externs.is_empty() {
            self.add_trailing_skipped_syntax_to_list(&mut body.externs, skipped_syntax);
        } else if !open_brace_or_semicolon.is_null() {
            *open_brace_or_semicolon = self.add_trailing_skipped_syntax(*open_brace_or_semicolon, skipped_syntax);
        } else {
            let pool = &mut self.pool;
            let bad = initial_bad_nodes.get_or_insert_with(|| pool.allocate());
            add_range(self.base.arena_ref(), bad, skipped_syntax);
        }
    }
}

/// `NamespaceParts` (LanguageParser.cs:399).
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub(crate) enum NamespaceParts {
    #[default]
    None = 0,
    ExternAliases = 1,
    Usings = 2,
    GlobalAttributes = 3,
    MembersAndStatements = 4,
    TypesAndNamespaces = 5,
    TopLevelStatementsAfterTypesAndNamespaces = 6,
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private void ParseNamespaceBody( [NotNullIfNotNull(nameof(openBraceOrSemicolon))] ref SyntaxToken? openBraceOrSemicolon, ref NamespaceBodyBuilder body, ref SyntaxListBuilder? initialBadNodes, SyntaxKind parentKind)` (LanguageParser.cs:410).
    pub(crate) fn parse_namespace_body(
        &mut self,
        open_brace_or_semicolon: &mut Green,
        body: &mut NamespaceBodyBuilder,
        initial_bad_nodes: &mut Option<SyntaxListBuilder>,
        parent_kind: SyntaxKind,
    ) {
        let mut saw_member_declaration_only_valid_within_type_declaration = false;
        self.parse_namespace_body_worker(
            open_brace_or_semicolon,
            body,
            initial_bad_nodes,
            parent_kind,
            &mut saw_member_declaration_only_valid_within_type_declaration,
        );

        // In the common case, we will not see errant type-only member declarations in the namespace itself.  In
        // that case, we have no extra work to do and can return immediately.
        //
        // If we do see errant type-only members (like a method/property/constructor/etc.), then see if they follow
        // some normal type declaration.  If so, it's likely there was a misplaced close curly that preemptively
        // ended the type declaration, and the member declaration was supposed to go in it instead.
        if !saw_member_declaration_only_valid_within_type_declaration {
            return;
        }

        // In a script file, it can be ok to have these members at the top level (IsScript: never).

        let mut final_members = self.pool.allocate();

        // Do a single linear sweep, examining each type declaration we run into within the namespace.
        let mut current_body_member_index = 0usize;
        while current_body_member_index < body.members.len() {
            let current_member = body.members[current_body_member_index];

            // If we have a suitable type declaration that ended without problem (has a real close curly and no
            // trailing semicolon),  then see if there are any type-only members following it that should be moved
            // into it.
            if let Some(slots) = type_declaration_slots(self.base.arena_ref().kind(current_member)) {
                let a = self.base.arena_ref();
                let close = a.slot(current_member, slots.close_brace_token);
                if a.slot(current_member, slots.semicolon_token).is_null()
                    && !close.is_null()
                    && !a.is_missing(close)
                    && !a.contains_diagnostics(close)
                {
                    let siblings_to_move_into_type =
                        self.determine_siblings_to_move_into_type(current_body_member_index, body);
                    if let Some((first_sibling_to_move_inclusive, last_sibling_to_move_exclusive)) =
                        siblings_to_move_into_type
                    {
                        // We found sibling type-only members.  Move them into the preceding type declaration.
                        let final_type_declaration = self.move_sibling_members_into_preceding_type(
                            current_member,
                            slots,
                            body,
                            first_sibling_to_move_inclusive,
                            last_sibling_to_move_exclusive,
                        );
                        final_members.push(final_type_declaration);

                        // We moved a sequence of type-only-members into the preceding type declaration.  We need to
                        // continue processing from the end of that sequence.
                        current_body_member_index = last_sibling_to_move_exclusive;
                        continue;
                    }
                }
            }

            // Simple case.  A normal namespace member we don't need to do anything with.
            final_members.push(current_member);
            current_body_member_index += 1;
        }

        let old = std::mem::replace(&mut body.members, final_members);
        self.pool.free(old);
    }

    /// `determineSiblingsToMoveIntoType` (a local function of `ParseNamespaceBody`, LanguageParser.cs:474).
    fn determine_siblings_to_move_into_type(
        &mut self,
        type_declaration_index: usize,
        body: &NamespaceBodyBuilder,
    ) -> Option<(usize, usize)> {
        let a = self.base.arena_ref();
        let start_inclusive = type_declaration_index + 1;
        if start_inclusive < body.members.len()
            && Self::is_member_declaration_only_valid_within_type_declaration_kind(
                a.kind(body.members[start_inclusive]),
            )
        {
            let mut end_exclusive = start_inclusive + 1;

            while end_exclusive < body.members.len()
                && Self::is_member_declaration_only_valid_within_type_declaration_kind(
                    a.kind(body.members[end_exclusive]),
                )
            {
                end_exclusive += 1;
            }

            return Some((start_inclusive, end_exclusive));
        }

        None
    }

    /// `moveSiblingMembersIntoPrecedingType` (a local function of `ParseNamespaceBody`, LanguageParser.cs:496).
    fn move_sibling_members_into_preceding_type(
        &mut self,
        type_declaration: Green,
        slots: TypeDeclarationSlots,
        body: &NamespaceBodyBuilder,
        first_sibling_to_move_inclusive: usize,
        last_sibling_to_move_exclusive: usize,
    ) -> Green {
        let mut final_type_declaration_members = self.pool.allocate();
        let members = self.base.arena_ref().slot(type_declaration, slots.members);
        add_range(self.base.arena_ref(), &mut final_type_declaration_members, members);
        let close_brace_token = self.base.arena_ref().slot(type_declaration, slots.close_brace_token);

        for member_to_move_index in first_sibling_to_move_inclusive..last_sibling_to_move_exclusive {
            let mut current_sibling = body.members[member_to_move_index];
            if member_to_move_index == first_sibling_to_move_inclusive {
                // Move the existing close brace token to the first member as a skipped token, with a
                // diagnostic saying that it was unexpected.
                let skipped =
                    self.add_error_args(close_brace_token, ErrorCode::ERR_InvalidMemberDecl, &[DiagArg::Str("}")]);
                current_sibling = self.add_leading_skipped_syntax(current_sibling, skipped);
            }

            final_type_declaration_members.push(current_sibling);
        }

        let is_last = last_sibling_to_move_exclusive == body.members.len();

        // The existing close brace token is moved to the first member as a skipped token, with a diagnostic saying
        // it was unexpected.  The type decl will then get a missing close brace token if there are still members
        // following.  If not, we'll try to eat an actual close brace token.
        let final_close_brace_token = if is_last {
            self.eat_token_kind(SyntaxKind::CloseBraceToken)
        } else {
            let missing = self.base.arena().missing_token(SyntaxKind::CloseBraceToken);
            self.add_error(missing, ErrorCode::ERR_RbraceExpected)
        };
        let new_members = self.pool.to_list_and_free(self.base.lexer.arena(), final_type_declaration_members);

        let mut new_slots = self.base.arena_ref().slots(type_declaration).to_vec();
        new_slots[slots.members] = new_members;
        new_slots[slots.close_brace_token] = final_close_brace_token;
        self.base.arena().update_node(type_declaration, &new_slots)
    }

    /// `private static bool IsMemberDeclarationOnlyValidWithinTypeDeclaration(MemberDeclarationSyntax? memberDeclaration)` (LanguageParser.cs:547).
    pub(crate) fn is_member_declaration_only_valid_within_type_declaration(&self, member_declaration: Green) -> bool {
        Self::is_member_declaration_only_valid_within_type_declaration_kind(
            self.base.arena_ref().kind(member_declaration),
        )
    }

    /// `IsMemberDeclarationOnlyValidWithinTypeDeclaration` of a declaration's kind (`SyntaxKind.None` for null).
    fn is_member_declaration_only_valid_within_type_declaration_kind(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::ConstructorDeclaration
                | SyntaxKind::ConversionOperatorDeclaration
                | SyntaxKind::DestructorDeclaration
                | SyntaxKind::EventDeclaration
                | SyntaxKind::EventFieldDeclaration
                | SyntaxKind::FieldDeclaration
                | SyntaxKind::IndexerDeclaration
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::OperatorDeclaration
                | SyntaxKind::PropertyDeclaration
        )
    }

    /// `private void ParseNamespaceBodyWorker( [NotNullIfNotNull(nameof(openBraceOrSemicolon))] ref SyntaxToken? openBraceOrSemicolon, ref NamespaceBodyBuilder body, ref SyntaxListBuilder? initialBadNodes, SyntaxKind parentKind, out bool sawMemberDeclarationOnlyValidWithinTypeDeclaration)` (LanguageParser.cs:562).
    pub(crate) fn parse_namespace_body_worker(
        &mut self,
        open_brace_or_semicolon: &mut Green,
        body: &mut NamespaceBodyBuilder,
        initial_bad_nodes: &mut Option<SyntaxListBuilder>,
        parent_kind: SyntaxKind,
        saw_member_declaration_only_valid_within_type_declaration: &mut bool,
    ) {
        // "top-level" expressions and statements should never occur inside an asynchronous context
        debug_assert!(!self.is_in_async);

        let is_global = open_brace_or_semicolon.is_null();

        let save_term = self.term_state;
        self.term_state = self.term_state | TerminatorState::IS_NAMESPACE_MEMBER_START_OR_STOP;
        let mut seen = NamespaceParts::None;
        let mut pending_incomplete_members = self.pool.allocate();
        let mut report_unexpected_token = true;

        *saw_member_declaration_only_valid_within_type_declaration = false;

        loop {
            let mut go_to_default = false;
            match self.current_kind() {
                SyntaxKind::NamespaceKeyword => {
                    // incomplete members must be processed before we add any nodes to the body:
                    Self::add_incomplete_members(&mut pending_incomplete_members, body);

                    let mut modifiers = self.pool.allocate();

                    let declaration = self.parse_namespace_declaration(Green::NULL, &mut modifiers);
                    let member = self.adjust_state_and_report_statement_out_of_order(&mut seen, declaration);
                    body.members.push(member);

                    self.pool.free(modifiers);

                    report_unexpected_token = true;
                }

                SyntaxKind::CloseBraceToken => {
                    // A very common user error is to type an additional }
                    // somewhere in the file.  This will cause us to stop parsing
                    // the root (global) namespace too early and will make the
                    // rest of the file unparseable and unusable by intellisense.
                    // We detect that case here and we skip the close curly and
                    // continue parsing as if we did not see the }
                    if is_global {
                        // incomplete members must be processed before we add any nodes to the body:
                        self.reduce_incomplete_members(
                            &mut pending_incomplete_members,
                            open_brace_or_semicolon,
                            body,
                            initial_bad_nodes,
                        );

                        let token = self.eat_token();
                        let token = self.add_error(token, ErrorCode::ERR_EOFExpected);

                        self.add_skipped_namespace_text(open_brace_or_semicolon, body, initial_bad_nodes, token);
                        report_unexpected_token = true;
                    } else {
                        // This token marks the end of a namespace body
                        break;
                    }
                }

                SyntaxKind::EndOfFileToken => {
                    // This token marks the end of a namespace body
                    break;
                }

                SyntaxKind::ExternKeyword => {
                    if is_global && !self.scan_extern_alias_directive() {
                        // extern member or a local function
                        go_to_default = true;
                    } else {
                        // incomplete members must be processed before we add any nodes to the body:
                        self.reduce_incomplete_members(
                            &mut pending_incomplete_members,
                            open_brace_or_semicolon,
                            body,
                            initial_bad_nodes,
                        );

                        let mut extern_ = self.parse_extern_alias_directive();
                        if seen > NamespaceParts::ExternAliases {
                            extern_ = self.add_error_to_first_token(extern_, ErrorCode::ERR_ExternAfterElements, &[]);
                            self.add_skipped_namespace_text(open_brace_or_semicolon, body, initial_bad_nodes, extern_);
                        } else {
                            body.externs.push(extern_);
                            seen = NamespaceParts::ExternAliases;
                        }

                        report_unexpected_token = true;
                    }
                }

                SyntaxKind::UsingKeyword => {
                    if is_global
                        && (self.peek_kind(1) == SyntaxKind::OpenParenToken
                            || self.is_possible_top_level_using_local_declaration_statement())
                    {
                        // Top-level using statement or using local declaration
                        go_to_default = true;
                    } else {
                        self.parse_namespace_body_worker_parse_using_directive(
                            open_brace_or_semicolon,
                            body,
                            initial_bad_nodes,
                            &mut seen,
                            &mut pending_incomplete_members,
                        );
                        report_unexpected_token = true;
                    }
                }

                SyntaxKind::IdentifierToken => {
                    if self.current_contextual_kind() != SyntaxKind::GlobalKeyword
                        || self.peek_kind(1) != SyntaxKind::UsingKeyword
                    {
                        go_to_default = true;
                    } else {
                        self.parse_namespace_body_worker_parse_using_directive(
                            open_brace_or_semicolon,
                            body,
                            initial_bad_nodes,
                            &mut seen,
                            &mut pending_incomplete_members,
                        );
                        report_unexpected_token = true;
                    }
                }

                SyntaxKind::OpenBracketToken => {
                    go_to_default = true;
                    if self.is_possible_global_attribute_declaration() {
                        // Could be an attribute, or it could be a collection expression at the top level.  e.g.
                        // `[assembly: 1].XYZ();`. While this is definitely odd code, it is totally legal (as
                        // `assembly` is just an identifier).
                        let mut attribute =
                            self.try_parse_attribute_declaration(parent_kind == SyntaxKind::CompilationUnit);
                        if !attribute.is_null() {
                            // incomplete members must be processed before we add any nodes to the body:
                            self.reduce_incomplete_members(
                                &mut pending_incomplete_members,
                                open_brace_or_semicolon,
                                body,
                                initial_bad_nodes,
                            );

                            if !is_global || seen > NamespaceParts::GlobalAttributes {
                                let a = self.base.arena_ref();
                                let target = a.slot(attribute, T::AttributeListSyntax::TARGET);
                                debug_assert!(
                                    !target.is_null(),
                                    "Must have a target as IsPossibleGlobalAttributeDeclaration checks for that"
                                );
                                let identifier = a.slot(target, T::AttributeTargetSpecifierSyntax::IDENTIFIER);
                                let colon = a.slot(target, T::AttributeTargetSpecifierSyntax::COLON_TOKEN);
                                let open = a.slot(attribute, T::AttributeListSyntax::OPEN_BRACKET_TOKEN);
                                let attributes = a.slot(attribute, T::AttributeListSyntax::ATTRIBUTES);
                                let close = a.slot(attribute, T::AttributeListSyntax::CLOSE_BRACKET_TOKEN);
                                let identifier = self.add_error(identifier, ErrorCode::ERR_GlobalAttributesNotFirst);
                                let a = self.base.arena();
                                let target = a.update_node(target, &[identifier, colon]);
                                attribute = a.update_node(attribute, &[open, target, attributes, close]);

                                self.add_skipped_namespace_text(
                                    open_brace_or_semicolon,
                                    body,
                                    initial_bad_nodes,
                                    attribute,
                                );
                            } else {
                                body.attributes.push(attribute);
                                seen = NamespaceParts::GlobalAttributes;
                            }

                            report_unexpected_token = true;
                            go_to_default = false;
                        }
                    }
                }

                _ => go_to_default = true,
            }

            if !go_to_default {
                continue;
            }

            // default:
            let member_or_statement = if is_global {
                self.parse_member_declaration_or_statement(parent_kind)
            } else {
                self.parse_member_declaration_with_parent_kind(parent_kind)
            };

            *saw_member_declaration_only_valid_within_type_declaration |=
                self.is_member_declaration_only_valid_within_type_declaration(member_or_statement);
            if member_or_statement.is_null() {
                // incomplete members must be processed before we add any nodes to the body:
                self.reduce_incomplete_members(
                    &mut pending_incomplete_members,
                    open_brace_or_semicolon,
                    body,
                    initial_bad_nodes,
                );

                // eat one token and try to parse declaration or statement again:
                let mut skipped_token = self.eat_token();
                if report_unexpected_token && !self.base.arena_ref().contains_diagnostics(skipped_token) {
                    skipped_token = self.add_error(skipped_token, ErrorCode::ERR_EOFExpected);

                    // do not report the error multiple times for subsequent tokens:
                    report_unexpected_token = false;
                }

                self.add_skipped_namespace_text(open_brace_or_semicolon, body, initial_bad_nodes, skipped_token);
            } else if self.base.arena_ref().kind(member_or_statement) == SyntaxKind::IncompleteMember
                && seen < NamespaceParts::MembersAndStatements
            {
                pending_incomplete_members.push(member_or_statement);
                report_unexpected_token = true;
            } else {
                // incomplete members must be processed before we add any nodes to the body:
                Self::add_incomplete_members(&mut pending_incomplete_members, body);

                let member = self.adjust_state_and_report_statement_out_of_order(&mut seen, member_or_statement);
                body.members.push(member);
                report_unexpected_token = true;
            }
        }

        // finally
        self.term_state = save_term;

        // adds pending incomplete nodes:
        Self::add_incomplete_members(&mut pending_incomplete_members, body);
        self.pool.free(pending_incomplete_members);
    }

    /// `adjustStateAndReportStatementOutOfOrder` (a local function of `ParseNamespaceBodyWorker`,
    /// LanguageParser.cs:776).
    fn adjust_state_and_report_statement_out_of_order(
        &mut self,
        seen: &mut NamespaceParts,
        member_or_statement: Green,
    ) -> Green {
        let mut member_or_statement = member_or_statement;
        match self.base.arena_ref().kind(member_or_statement) {
            SyntaxKind::GlobalStatement => {
                if *seen < NamespaceParts::MembersAndStatements {
                    *seen = NamespaceParts::MembersAndStatements;
                } else if *seen == NamespaceParts::TypesAndNamespaces {
                    *seen = NamespaceParts::TopLevelStatementsAfterTypesAndNamespaces;

                    // !IsScript
                    member_or_statement =
                        self.add_error(member_or_statement, ErrorCode::ERR_TopLevelStatementAfterNamespaceOrType);
                }
            }

            SyntaxKind::NamespaceDeclaration
            | SyntaxKind::FileScopedNamespaceDeclaration
            | SyntaxKind::EnumDeclaration
            | SyntaxKind::StructDeclaration
            | SyntaxKind::UnionDeclaration
            | SyntaxKind::ClassDeclaration
            | SyntaxKind::InterfaceDeclaration
            | SyntaxKind::DelegateDeclaration
            | SyntaxKind::RecordDeclaration
            | SyntaxKind::RecordStructDeclaration => {
                if *seen < NamespaceParts::TypesAndNamespaces {
                    *seen = NamespaceParts::TypesAndNamespaces;
                }
            }

            _ => {
                if *seen < NamespaceParts::MembersAndStatements {
                    *seen = NamespaceParts::MembersAndStatements;
                }
            }
        }

        member_or_statement
    }

    /// `parseUsingDirective` (a local function of `ParseNamespaceBodyWorker`, LanguageParser.cs:824).
    fn parse_namespace_body_worker_parse_using_directive(
        &mut self,
        open_brace: &mut Green,
        body: &mut NamespaceBodyBuilder,
        initial_bad_nodes: &mut Option<SyntaxListBuilder>,
        seen: &mut NamespaceParts,
        pending_incomplete_members: &mut SyntaxListBuilder,
    ) {
        // incomplete members must be processed before we add any nodes to the body:
        self.reduce_incomplete_members(pending_incomplete_members, open_brace, body, initial_bad_nodes);

        let mut using_ = self.parse_using_directive();
        if *seen > NamespaceParts::Usings {
            using_ = self.add_error(using_, ErrorCode::ERR_UsingAfterElements);
            self.add_skipped_namespace_text(open_brace, body, initial_bad_nodes, using_);
        } else {
            body.usings.push(using_);
            *seen = NamespaceParts::Usings;
        }
    }

    /// `private static void AddIncompleteMembers(ref SyntaxListBuilder<MemberDeclarationSyntax> incompleteMembers, ref NamespaceBodyBuilder body)` (LanguageParser.cs:848).
    pub(crate) fn add_incomplete_members(incomplete_members: &mut SyntaxListBuilder, body: &mut NamespaceBodyBuilder) {
        if !incomplete_members.is_empty() {
            body.members.extend_from_slice(incomplete_members);
            incomplete_members.clear();
        }
    }

    /// `private void ReduceIncompleteMembers( ref SyntaxListBuilder<MemberDeclarationSyntax> incompleteMembers, ref SyntaxToken? openBraceOrSemicolon, ref NamespaceBodyBuilder body, ref SyntaxListBuilder? initialBadNodes)` (LanguageParser.cs:857).
    pub(crate) fn reduce_incomplete_members(
        &mut self,
        incomplete_members: &mut SyntaxListBuilder,
        open_brace_or_semicolon: &mut Green,
        body: &mut NamespaceBodyBuilder,
        initial_bad_nodes: &mut Option<SyntaxListBuilder>,
    ) {
        for &member in incomplete_members.iter() {
            self.add_skipped_namespace_text(open_brace_or_semicolon, body, initial_bad_nodes, member);
        }

        incomplete_members.clear();
    }

    /// `private bool IsPossibleNamespaceMemberDeclaration()` (LanguageParser.cs:869).
    pub(crate) fn is_possible_namespace_member_declaration(&mut self) -> bool {
        match self.current_kind() {
            SyntaxKind::ExternKeyword | SyntaxKind::UsingKeyword | SyntaxKind::NamespaceKeyword => true,
            SyntaxKind::IdentifierToken => self.is_partial_in_namespace_member_declaration(),
            kind => Self::is_possible_start_of_type_declaration(kind),
        }
    }

    /// `private bool IsPartialInNamespaceMemberDeclaration()` (LanguageParser.cs:884).
    pub(crate) fn is_partial_in_namespace_member_declaration(&mut self) -> bool {
        if self.current_contextual_kind() == SyntaxKind::PartialKeyword {
            #[allow(clippy::if_same_then_else, reason = "the C# returns true from each branch")]
            if self.is_partial_type() {
                return true;
            } else if self.peek_kind(1) == SyntaxKind::NamespaceKeyword {
                return true;
            }
        }

        false
    }

    /// `public bool IsEndOfNamespace()` (LanguageParser.cs:901).
    pub(crate) fn is_end_of_namespace(&mut self) -> bool {
        self.current_kind() == SyntaxKind::CloseBraceToken
    }

    /// `public bool IsGobalAttributesTerminator()` (LanguageParser.cs:906).
    #[allow(dead_code)] // Roslyn calls it nowhere either
    pub(crate) fn is_gobal_attributes_terminator(&mut self) -> bool {
        self.is_end_of_namespace() || self.is_possible_namespace_member_declaration()
    }

    /// `private bool IsNamespaceMemberStartOrStop()` (LanguageParser.cs:912).
    pub(crate) fn is_namespace_member_start_or_stop(&mut self) -> bool {
        self.is_end_of_namespace() || self.is_possible_namespace_member_declaration()
    }

    /// `private bool ScanExternAliasDirective()` (LanguageParser.cs:921).
    pub(crate) fn scan_extern_alias_directive(&mut self) -> bool {
        // The check also includes the ending semicolon so that we can disambiguate among:
        //   extern alias goo;
        //   extern alias goo();
        //   extern alias goo { get; }

        self.current_kind() == SyntaxKind::ExternKeyword
            && self.peek_kind(1) == SyntaxKind::IdentifierToken
            && self.peek_contextual_kind(1) == SyntaxKind::AliasKeyword
            && self.peek_kind(2) == SyntaxKind::IdentifierToken
            && self.peek_kind(3) == SyntaxKind::SemicolonToken
    }

    /// `private ExternAliasDirectiveSyntax ParseExternAliasDirective()` (LanguageParser.cs:934).
    pub(crate) fn parse_extern_alias_directive(&mut self) -> Green {
        debug_assert!(self.current_kind() == SyntaxKind::ExternKeyword);

        let extern_keyword = self.eat_token_kind(SyntaxKind::ExternKeyword);
        let alias_keyword = self.eat_contextual_token(SyntaxKind::AliasKeyword);
        let identifier = self.parse_identifier_token(ErrorCode::ERR_IdentifierExpected);
        let semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
        self.base.arena().extern_alias_directive(extern_keyword, alias_keyword, identifier, semicolon)
    }

    /// `private NameEqualsSyntax ParseNameEquals()` (LanguageParser.cs:950).
    pub(crate) fn parse_name_equals(&mut self) -> Green {
        debug_assert!(self.is_named_assignment());
        let identifier = self.parse_identifier_token(ErrorCode::ERR_IdentifierExpected);
        let name = self.base.arena().identifier_name(identifier);
        let equals = self.eat_token_kind(SyntaxKind::EqualsToken);
        self.base.arena().name_equals(name, equals)
    }

    /// `private UsingDirectiveSyntax ParseUsingDirective()` (LanguageParser.cs:958).
    pub(crate) fn parse_using_directive(&mut self) -> Green {
        let global_token = if self.current_contextual_kind() == SyntaxKind::GlobalKeyword {
            let token = self.eat_token();
            self.convert_to_keyword(token)
        } else {
            Green::NULL
        };

        debug_assert!(self.current_kind() == SyntaxKind::UsingKeyword);

        let using_token = self.eat_token_kind(SyntaxKind::UsingKeyword);
        let mut static_token = self.try_eat_token(SyntaxKind::StaticKeyword);
        let mut unsafe_token = self.try_eat_token(SyntaxKind::UnsafeKeyword);

        // if the user wrote `using unsafe static` skip the `static` and tell them it needs to be `using static unsafe`.
        if static_token.is_null() && !unsafe_token.is_null() && self.current_kind() == SyntaxKind::StaticKeyword {
            // create a missing 'static' token so that later binding does recognize what the user wanted.
            static_token = self.base.arena().missing_token(SyntaxKind::StaticKeyword);
            let eaten = self.eat_token();
            let eaten = self.add_error(eaten, ErrorCode::ERR_BadStaticAfterUnsafe);
            unsafe_token = self.add_trailing_skipped_syntax(unsafe_token, eaten);
        }

        let alias = if self.is_named_assignment() { self.parse_name_equals() } else { Green::NULL };

        let mut type_;
        let semicolon;

        let is_alias_to_function_pointer = !alias.is_null() && self.current_kind() == SyntaxKind::DelegateKeyword;
        if !is_alias_to_function_pointer && self.is_possible_namespace_member_declaration() {
            //We're worried about the case where someone already has a correct program
            //and they've gone back to add a using directive, but have not finished the
            //new directive.  e.g.
            //
            //    using
            //    namespace Goo {
            //        //...
            //    }
            //
            //If the token we see after "using" could be its own top-level construct, then
            //we just want to insert a missing identifier and semicolon and then return to
            //parsing at the top-level.
            //
            //NB: there's no way this could be true for a set of tokens that form a valid
            //using directive, so there's no danger in checking the error case first.

            let actual = self.current_kind();
            let identifier = self.create_missing_token(SyntaxKind::IdentifierToken, actual);
            type_ = self.base.arena().identifier_name(identifier);
            semicolon = self.base.arena().missing_token(SyntaxKind::SemicolonToken);
        } else {
            // In the case where we don't have an alias, only parse out a name for this using-directive.  This is
            // worse for error recovery, but it means all code that consumes a using-directive can keep on assuming
            // it has a name when there is no alias.  Only code that specifically has to process aliases then has to
            // deal with getting arbitrary types back.
            type_ = if alias.is_null() {
                self.parse_qualified_name(NameOptions::NONE)
            } else {
                self.parse_type(ParseTypeMode::Normal)
            };

            // If we can see a semicolon ahead, then the current token was probably supposed to be an identifier
            if self.base.arena_ref().is_missing(type_) && self.peek_kind(1) == SyntaxKind::SemicolonToken {
                let eaten = self.eat_token();
                type_ = self.add_trailing_skipped_syntax(type_, eaten);
            }

            semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
        }

        self.base.arena().using_directive(
            global_token,
            using_token,
            static_token,
            unsafe_token,
            alias,
            type_,
            semicolon,
        )
    }

    /// `private bool IsPossibleGlobalAttributeDeclaration()` (LanguageParser.cs:1028).
    pub(crate) fn is_possible_global_attribute_declaration(&mut self) -> bool {
        if self.current_kind() != SyntaxKind::OpenBracketToken {
            return false;
        }
        let next = self.peek_token(1);
        self.is_global_attribute_target(next) && self.peek_kind(2) == SyntaxKind::ColonToken
    }

    /// `private static bool IsGlobalAttributeTarget(SyntaxToken token)` (LanguageParser.cs:1035): `ToAttributeLocation`
    /// reads the token's value text.
    pub(crate) fn is_global_attribute_target(&self, token: Green) -> bool {
        matches!(self.base.arena_ref().token_value_text(token, self.source()), b"assembly" | b"module")
    }

    /// `private bool IsPossibleAttributeDeclaration()` (LanguageParser.cs:1047).
    pub(crate) fn is_possible_attribute_declaration(&mut self) -> bool {
        // Have to at least start with `[` to be an attribute
        if self.current_kind() != SyntaxKind::OpenBracketToken {
            return false;
        }

        let reset_point = self.get_disposable_reset_point(true);
        let result = 'scope: {
            // Eat the `[`
            self.eat_token();

            // `[ id` could definitely begin an attribute.
            if self.is_true_identifier() {
                break 'scope true;
            }

            // `[ word: ...` could definitely start an attribute.
            if self.is_attribute_target() {
                break 'scope true;
            }

            // If we see `[lit` (like `[0`) then this is def not an attribute, and should be parsed as a collection
            // expr.  Note: this heuristic can be added to in the future.
            if facts::is_literal_expression(self.current_kind()) {
                break 'scope false;
            }

            true
        };
        self.dispose_reset_point(reset_point);
        result
    }

    /// `private SyntaxList<AttributeListSyntax> ParseAttributeDeclarations(bool inExpressionContext)` (LanguageParser.cs:1075).
    pub(crate) fn parse_attribute_declarations(&mut self, in_expression_context: bool) -> Green {
        let save_term = self.term_state;
        self.term_state = self.term_state | TerminatorState::IS_ATTRIBUTE_DECLARATION_TERMINATOR;

        // An attribute can never appear *inside* an attribute argument (since a lambda expression cannot be used as
        // a constant argument value).  However, during parsing we can end up in a state where we're trying to
        // exactly that, through a path of Attribute->Argument->Expression->Attribute (since attributes can not be
        // on lambda expressions).
        //
        // Worse, when we are in a deeply ambiguous (or gibberish) scenario, where we see lots of code with `... [
        // ... [ ... ] ... ] ...`, we can get into exponential speculative parsing where we try `[ ... ]` both as an
        // attribute *and* a collection expression.
        //
        // Since we cannot ever legally have an attribute within an attribute, we bail out here immediately
        // syntactically.  This does mean we won't parse something like: `[X([Y]() => {})]` without errors, but that
        // is not semantically legal anyway.
        if save_term == self.term_state {
            return Green::NULL;
        }

        let mut attributes = self.pool.allocate();
        while self.is_possible_attribute_declaration() {
            let attribute_declaration = self.try_parse_attribute_declaration(in_expression_context);
            if attribute_declaration.is_null() {
                break;
            }

            attributes.push(attribute_declaration);
        }

        self.term_state = save_term;

        self.pool.to_list_and_free(self.base.lexer.arena(), attributes)
    }

    /// `private bool IsAttributeDeclarationTerminator()` (LanguageParser.cs:1110).
    pub(crate) fn is_attribute_declaration_terminator(&mut self) -> bool {
        self.current_kind() == SyntaxKind::CloseBracketToken || self.is_possible_attribute_declaration() // start of a new one...
    }

    /// `private bool IsAttributeTarget()` (LanguageParser.cs:1116).
    pub(crate) fn is_attribute_target(&mut self) -> bool {
        Self::is_some_word(self.current_kind()) && self.peek_kind(1) == SyntaxKind::ColonToken
    }

    /// `private AttributeListSyntax? TryParseAttributeDeclaration(bool inExpressionContext)` (LanguageParser.cs:1119).
    pub(crate) fn try_parse_attribute_declaration(&mut self, in_expression_context: bool) -> Green {
        // May have to reset if we discover this is not an attribute but is instead a collection expression.
        let mut reset_point = self.get_disposable_reset_point(false);

        let mut open_bracket = self.eat_token_kind(SyntaxKind::OpenBracketToken);

        // Check for optional location :
        let location = if self.is_attribute_target() {
            let token = self.eat_token();
            let identifier = self.convert_to_keyword(token);
            let colon = self.eat_token_kind(SyntaxKind::ColonToken);
            self.base.arena().attribute_target_specifier(identifier, colon)
        } else {
            Green::NULL
        };

        let attributes = self.parse_comma_separated_syntax_list(
            &mut open_bracket,
            SyntaxKind::CloseBracketToken,
            |this| this.is_possible_attribute(),
            |this| this.parse_attribute(),
            Self::try_parse_attribute_declaration_skip_bad_attribute_list_tokens,
            true,
            true,
            false,
        );

        let close_bracket = self.eat_token_kind(SyntaxKind::CloseBracketToken);
        if in_expression_context && self.try_parse_attribute_declaration_should_parse_as_collection_expression() {
            // we're in an expression and we've seen `[A, B].`  This is actually the start of a collection expression
            // that someone is explicitly accessing a member off of.
            self.reset_disposable(&mut reset_point);
            self.dispose_reset_point(reset_point);
            return Green::NULL;
        }

        self.dispose_reset_point(reset_point);
        self.base.arena().attribute_list(open_bracket, location, attributes, close_bracket)
    }

    /// `shouldParseAsCollectionExpression` (a local function of `TryParseAttributeDeclaration`,
    /// LanguageParser.cs:1159).
    fn try_parse_attribute_declaration_should_parse_as_collection_expression(&mut self) -> bool {
        // `[A, B].` is a member access off of a collection expression.
        if self.current_kind() == SyntaxKind::DotToken {
            return true;
        }

        // `[A, B]->` is a member access off of a collection expression. Note: this will always be illegal
        // semantically (as a collection expression has the natural type List<> which is not a pointer type).  But
        // we leave that check to binding.
        if self.current_kind() == SyntaxKind::MinusGreaterThanToken {
            return true;
        }

        // `[A, B]?.`  The `?` is unnecessary (as a collection expression is always non-null), but is still
        // syntactically legal.
        if self.current_kind() == SyntaxKind::QuestionToken && self.peek_kind(1) == SyntaxKind::DotToken {
            return true;
        }

        false
    }

    /// `skipBadAttributeListTokens` (a local function of `TryParseAttributeDeclaration`, LanguageParser.cs:1182).
    fn try_parse_attribute_declaration_skip_bad_attribute_list_tokens(
        this: &mut LanguageParser<'_, '_>,
        open_bracket: &mut Green,
        list: &mut SyntaxListBuilder,
        expected_kind: SyntaxKind,
        close_kind: SyntaxKind,
    ) -> PostSkipAction {
        this.skip_bad_separated_list_tokens_with_expected_kind(
            open_bracket,
            list,
            |p| p.current_kind() != SyntaxKind::CommaToken && !p.is_possible_attribute(),
            |p, close_kind| p.current_kind() == close_kind,
            expected_kind,
            close_kind,
        )
    }

    /// `private bool IsPossibleAttribute()` (LanguageParser.cs:1192).
    pub(crate) fn is_possible_attribute(&mut self) -> bool {
        self.is_true_identifier()
    }

    /// `private AttributeSyntax ParseAttribute()` (LanguageParser.cs:1197).
    pub(crate) fn parse_attribute(&mut self) -> Green {
        let name = self.parse_qualified_name(NameOptions::NONE);
        let argument_list = self.parse_attribute_argument_list();
        self.base.arena().attribute(name, argument_list)
    }

    /// `internal AttributeArgumentListSyntax? ParseAttributeArgumentList()` (LanguageParser.cs:1209).
    pub(crate) fn parse_attribute_argument_list(&mut self) -> Green {
        if self.current_kind() != SyntaxKind::OpenParenToken {
            return Green::NULL;
        }

        let mut open_paren = self.eat_token_kind(SyntaxKind::OpenParenToken);

        let arg_nodes = self.parse_comma_separated_syntax_list_with_allow_semicolon_as_separator(
            &mut open_paren,
            SyntaxKind::CloseParenToken,
            |this| this.is_possible_attribute_argument(),
            |this| this.parse_attribute_argument(),
            Some(Self::parse_attribute_argument_list_immediately_abort),
            Self::parse_attribute_argument_list_skip_bad_attribute_argument_tokens,
            false,
            false,
            false,
        );

        let close_paren = self.eat_token_kind(SyntaxKind::CloseParenToken);
        self.base.arena().attribute_argument_list(open_paren, arg_nodes, close_paren)
    }

    /// `skipBadAttributeArgumentTokens` (a local function of `ParseAttributeArgumentList`, LanguageParser.cs:1237).
    fn parse_attribute_argument_list_skip_bad_attribute_argument_tokens(
        this: &mut LanguageParser<'_, '_>,
        open_paren: &mut Green,
        list: &mut SyntaxListBuilder,
        expected_kind: SyntaxKind,
        close_kind: SyntaxKind,
    ) -> PostSkipAction {
        this.skip_bad_separated_list_tokens_with_expected_kind(
            open_paren,
            list,
            |p| p.current_kind() != SyntaxKind::CommaToken && !p.is_possible_attribute_argument(),
            |p, close_kind| p.current_kind() == close_kind,
            expected_kind,
            close_kind,
        )
    }

    /// `immediatelyAbort` (a local function of `ParseAttributeArgumentList`, LanguageParser.cs:1246).
    fn parse_attribute_argument_list_immediately_abort(this: &mut LanguageParser<'_, '_>, argument: Green) -> bool {
        // We can be very thrown off by incomplete strings in an attribute argument (especially as the lexer
        // will restart on the next line with the contents of the string then being interpreted as more
        // arguments).  Bail out in this case to prevent going off the rails.
        let a = this.base.arena_ref();
        let expression = a.slot(argument, T::AttributeArgumentSyntax::EXPRESSION);
        if a.kind(expression) == SyntaxKind::StringLiteralExpression {
            let literal_token = a.slot(expression, T::LiteralExpressionSyntax::TOKEN);
            if a.diagnostics(literal_token).any(|d| a.diagnostic(d).code == ErrorCode::ERR_NewlineInConst) {
                return true;
            }
        }

        if a.kind(expression) == SyntaxKind::InterpolatedStringExpression {
            let start = a.slot(expression, T::InterpolatedStringExpressionSyntax::STRING_START_TOKEN);
            let end = a.slot(expression, T::InterpolatedStringExpressionSyntax::STRING_END_TOKEN);
            if a.kind(start) == SyntaxKind::InterpolatedStringStartToken && a.is_missing(end) {
                return true;
            }
        }

        false
    }

    /// `private bool IsPossibleAttributeArgument()` (LanguageParser.cs:1264).
    pub(crate) fn is_possible_attribute_argument(&mut self) -> bool {
        self.is_possible_expression()
    }

    /// `private AttributeArgumentSyntax ParseAttributeArgument()` (LanguageParser.cs:1269).
    pub(crate) fn parse_attribute_argument(&mut self) -> Green {
        // Need to parse both "real" named arguments and attribute-style named arguments.
        // We track attribute-style named arguments only with fShouldHaveName.

        let mut name_equals = Green::NULL;
        let mut name_colon = Green::NULL;
        if self.current_kind() == SyntaxKind::IdentifierToken {
            match self.peek_kind(1) {
                SyntaxKind::EqualsToken => {
                    let identifier = self.parse_identifier_token(ErrorCode::ERR_IdentifierExpected);
                    let name = self.base.arena().identifier_name(identifier);
                    let equals = self.eat_token_kind(SyntaxKind::EqualsToken);
                    name_equals = self.base.arena().name_equals(name, equals);
                }
                SyntaxKind::ColonToken => {
                    let name = self.parse_identifier_name(ErrorCode::ERR_IdentifierExpected);
                    let colon = self.eat_token_kind(SyntaxKind::ColonToken);
                    name_colon = self.base.arena().name_colon(name, colon);
                }
                _ => {}
            }
        }

        let expression = self.parse_expression_core();
        self.base.arena().attribute_argument(name_equals, name_colon, expression)
    }

    /// `private static DeclarationModifiers GetModifierExcludingScoped(SyntaxToken token)` (LanguageParser.cs:1299).
    pub(crate) fn get_modifier_excluding_scoped(&self, token: Green) -> DeclarationModifiers {
        let a = self.base.arena_ref();
        Self::get_modifier_excluding_scoped_with_contextual_kind(a.kind(token), a.contextual_kind(token))
    }

    /// `internal static DeclarationModifiers GetModifierExcludingScoped(SyntaxKind kind, SyntaxKind contextualKind)` (LanguageParser.cs:1302).
    pub(crate) fn get_modifier_excluding_scoped_with_contextual_kind(
        kind: SyntaxKind,
        contextual_kind: SyntaxKind,
    ) -> DeclarationModifiers {
        match kind {
            SyntaxKind::PublicKeyword => DeclarationModifiers::PUBLIC,
            SyntaxKind::InternalKeyword => DeclarationModifiers::INTERNAL,
            SyntaxKind::ProtectedKeyword => DeclarationModifiers::PROTECTED,
            SyntaxKind::PrivateKeyword => DeclarationModifiers::PRIVATE,
            SyntaxKind::SealedKeyword => DeclarationModifiers::SEALED,
            SyntaxKind::AbstractKeyword => DeclarationModifiers::ABSTRACT,
            SyntaxKind::StaticKeyword => DeclarationModifiers::STATIC,
            SyntaxKind::VirtualKeyword => DeclarationModifiers::VIRTUAL,
            SyntaxKind::ExternKeyword => DeclarationModifiers::EXTERN,
            SyntaxKind::NewKeyword => DeclarationModifiers::NEW,
            SyntaxKind::OverrideKeyword => DeclarationModifiers::OVERRIDE,
            SyntaxKind::ReadOnlyKeyword => DeclarationModifiers::READ_ONLY,
            SyntaxKind::VolatileKeyword => DeclarationModifiers::VOLATILE,
            SyntaxKind::UnsafeKeyword => DeclarationModifiers::UNSAFE,
            SyntaxKind::PartialKeyword => DeclarationModifiers::PARTIAL,
            SyntaxKind::AsyncKeyword => DeclarationModifiers::ASYNC,
            SyntaxKind::RefKeyword => DeclarationModifiers::REF,
            SyntaxKind::IdentifierToken => match contextual_kind {
                SyntaxKind::PartialKeyword => DeclarationModifiers::PARTIAL,
                SyntaxKind::AsyncKeyword => DeclarationModifiers::ASYNC,
                SyntaxKind::RequiredKeyword => DeclarationModifiers::REQUIRED,
                SyntaxKind::FileKeyword => DeclarationModifiers::FILE,
                SyntaxKind::ClosedKeyword => DeclarationModifiers::CLOSED,
                SyntaxKind::SafeKeyword => DeclarationModifiers::SAFE,
                _ => DeclarationModifiers::NONE,
            },
            _ => DeclarationModifiers::NONE,
        }
    }

    /// `private void ParseModifiers(SyntaxListBuilder tokens, bool forAccessors, bool forTopLevelStatements, out bool isPossibleTypeDeclaration)` (LanguageParser.cs:1363).
    pub(crate) fn parse_modifiers(
        &mut self,
        tokens: &mut SyntaxListBuilder,
        for_accessors: bool,
        for_top_level_statements: bool,
        is_possible_type_declaration: &mut bool,
    ) {
        debug_assert!(!(for_accessors && for_top_level_statements));

        *is_possible_type_declaration = true;

        loop {
            let current = self.current_token();
            let new_mod = self.get_modifier_excluding_scoped(current);

            debug_assert!(new_mod != DeclarationModifiers::SCOPED);
            if new_mod == DeclarationModifiers::NONE {
                if !for_accessors {
                    let scoped_keyword = self.parse_possible_scoped_keyword(false, false);

                    if !scoped_keyword.is_null() {
                        *is_possible_type_declaration = false;
                        tokens.push(scoped_keyword);
                    }
                }

                break;
            }

            let mod_tok;
            match new_mod {
                DeclarationModifiers::PARTIAL => {
                    let next_token = self.peek_token(1);
                    let next_kind = self.base.arena_ref().kind(next_token);
                    if self.is_partial_type() || self.is_partial_member() {
                        // Standard legal cases.
                        let token = self.eat_token();
                        mod_tok = self.convert_to_keyword(token);
                    } else if next_kind == SyntaxKind::NamespaceKeyword {
                        // Error reported in binding
                        let token = self.eat_token();
                        mod_tok = self.convert_to_keyword(token);
                    } else if matches!(next_kind, SyntaxKind::EnumKeyword | SyntaxKind::DelegateKeyword)
                        || (Self::is_possible_start_of_type_declaration(next_kind)
                            && self.get_modifier_excluding_scoped(next_token) != DeclarationModifiers::NONE)
                    {
                        // Error reported in ModifierUtils.
                        let token = self.eat_token();
                        mod_tok = self.convert_to_keyword(token);
                    } else {
                        return;
                    }
                }

                DeclarationModifiers::REF => {
                    // 'ref' is only a modifier if used on a ref struct
                    // it must be either immediately before the 'struct'
                    // keyword, or immediately before 'partial struct' if
                    // this is a partial ref struct declaration
                    let next = self.peek_token(1);
                    let next_is_partial = self.base.arena_ref().contextual_kind(next) == SyntaxKind::PartialKeyword;
                    if self.parse_modifiers_is_struct_or_record_or_union_keyword(next) || {
                        next_is_partial && {
                            let after = self.peek_token(2);
                            self.parse_modifiers_is_struct_or_record_or_union_keyword(after)
                        }
                    } {
                        mod_tok = self.eat_token();
                    } else if for_accessors && self.is_possible_accessor_modifier() {
                        // Accept ref as a modifier for properties and event accessors, to produce an error later during binding.
                        mod_tok = self.eat_token();
                    } else {
                        return;
                    }
                }

                DeclarationModifiers::FILE => {
                    let token = self
                        .parse_modifiers_parse_as_modifier(MessageID::IDS_FeatureFileTypes, for_top_level_statements);
                    if token.is_null() {
                        return;
                    }
                    mod_tok = token;
                }

                DeclarationModifiers::CLOSED => {
                    let token = self.parse_modifiers_parse_as_modifier(
                        MessageID::IDS_FeatureClosedClasses,
                        for_top_level_statements,
                    );
                    if token.is_null() {
                        return;
                    }
                    mod_tok = token;
                }

                DeclarationModifiers::ASYNC => {
                    if !self.should_contextual_keyword_be_treated_as_modifier(false) {
                        return;
                    }

                    let token = self.eat_token();
                    mod_tok = self.convert_to_keyword(token);
                }

                DeclarationModifiers::REQUIRED => {
                    let token = self.parse_modifiers_parse_as_modifier(
                        MessageID::IDS_FeatureRequiredMembers,
                        for_top_level_statements,
                    );
                    if token.is_null() {
                        return;
                    }
                    mod_tok = token;
                }

                DeclarationModifiers::SAFE => {
                    if for_accessors {
                        if !self.is_possible_accessor_modifier() {
                            return;
                        }

                        let token = self.eat_token();
                        mod_tok = self.convert_to_keyword(token);
                    } else {
                        let token = self.parse_modifiers_parse_as_modifier(
                            MessageID::IDS_FeatureUnsafeEvolution,
                            for_top_level_statements,
                        );
                        if token.is_null() {
                            return;
                        }
                        mod_tok = token;
                    }
                }

                _ => {
                    mod_tok = self.eat_token();
                }
            }

            debug_assert!(!matches!(
                self.base.arena_ref().kind(mod_tok),
                SyntaxKind::OutKeyword | SyntaxKind::InKeyword
            ));
            tokens.push(mod_tok);
        }
    }

    /// `parseAsModifier` (a local function of `ParseModifiers`, LanguageParser.cs:1497): the modifier token, or null
    /// when the word is not one.
    fn parse_modifiers_parse_as_modifier(
        &mut self,
        required_feature: MessageID,
        for_top_level_statements: bool,
    ) -> Green {
        // When 'requiredFeature' is enabled, the associated contextual keyword is always a keyword if not escaped. Otherwise, we reuse the async detection
        // machinery to make a conservative guess as to whether the user meant it to be a keyword, so that they get a good langver
        // diagnostic and all the machinery to upgrade their project kicks in. The only exception to this rule is top level statements,
        // where the user could conceivably have a local with the same name as the modifier. For these locations, we need to disambiguate as well.
        if (!self.is_feature_enabled(required_feature) || for_top_level_statements)
            && !self.should_contextual_keyword_be_treated_as_modifier(false)
        {
            return Green::NULL;
        }

        // LangVersion errors for contextual modifiers are given during binding.
        let token = self.eat_token();
        self.convert_to_keyword(token)
    }

    /// `isStructOrRecordOrUnionKeyword` (a local function of `ParseModifiers`, LanguageParser.cs:1514).
    fn parse_modifiers_is_struct_or_record_or_union_keyword(&self, token: Green) -> bool {
        let a = self.base.arena_ref();
        if a.kind(token) == SyntaxKind::StructKeyword {
            return true;
        }

        match a.contextual_kind(token) {
            // This is an unusual use of LangVersion. Normally we only produce errors when the langversion
            // does not support a feature, but in this case we are effectively making a language breaking
            // change to consider "record" a type declaration in all ambiguous cases. To avoid breaking
            // older code that is not using C# 9 we conditionally parse based on langversion
            SyntaxKind::RecordKeyword => self.is_feature_enabled(MessageID::IDS_FeatureRecords),
            // This is an unusual use of LangVersion. Normally we only produce errors when the langversion
            // does not support a feature, but in this case we are effectively making a language breaking
            // change to consider "union" a type declaration in all ambiguous cases. To avoid breaking
            // older code that is not using C# 15 we conditionally parse based on langversion
            SyntaxKind::UnionKeyword => self.is_feature_enabled(MessageID::IDS_FeatureUnions),
            _ => false,
        }
    }

    /// `private bool ShouldContextualKeywordBeTreatedAsModifier(bool parsingStatementNotDeclaration)` (LanguageParser.cs:1546).
    pub(crate) fn should_contextual_keyword_be_treated_as_modifier(
        &mut self,
        parsing_statement_not_declaration: bool,
    ) -> bool {
        debug_assert!(self.current_kind() == SyntaxKind::IdentifierToken);

        // Adapted from CParser::IsAsyncMethod.

        let next = self.peek_token(1);
        if self.is_non_contextual_modifier(next) {
            // If the next token is a (non-contextual) modifier keyword, then this token is
            // definitely a modifier
            return true;
        }

        // Some of our helpers start at the current token, so we'll have to advance for their
        // sake and then backtrack when we're done.  Don't leave this block without releasing
        // the reset point.
        let reset_point = self.get_disposable_reset_point(true);
        let result = 'scope: {
            self.eat_token(); //move past contextual token

            if !parsing_statement_not_declaration && self.current_contextual_kind() == SyntaxKind::PartialKeyword {
                self.eat_token(); // "partial" doesn't affect our decision, so look past it.
            }

            // ... 'TOKEN' [partial] <typedecl> ...
            // ... 'TOKEN' [partial] <event> ...
            // ... 'TOKEN' [partial] <implicit> <operator> ...
            // ... 'TOKEN' [partial] <explicit> <operator> ...
            // ... 'TOKEN' [partial] <typename> <operator> ...
            // ... 'TOKEN' [partial] <typename> <membername> ...
            // DEVNOTE: Although we parse async user defined conversions, operators, etc. here,
            // anything other than async methods are detected as erroneous later, during the define phase
            // Generally wherever we refer to 'async' here, it can also be 'required' or 'file'.

            if !parsing_statement_not_declaration {
                let current_token_kind = self.current_kind();
                if Self::is_type_modifier_or_type_keyword(current_token_kind)
                    || current_token_kind == SyntaxKind::EventKeyword
                    || (matches!(current_token_kind, SyntaxKind::ExplicitKeyword | SyntaxKind::ImplicitKeyword)
                        && self.peek_kind(1) == SyntaxKind::OperatorKeyword)
                {
                    break 'scope true;
                }
            }

            if self.scan_type(false) != ScanTypeFlags::NOT_TYPE {
                // We've seen "TOKEN TypeName".  Now we have to determine if we should we treat
                // 'TOKEN' as a modifier.  Or is the user actually writing something like
                // "public TOKEN Goo" where 'TOKEN' is actually the return type.

                if self.is_possible_member_name() {
                    // we have: "TOKEN Type X" or "TOKEN Type this", 'TOKEN' is definitely a
                    // modifier here.
                    break 'scope true;
                }

                let current_token_kind = self.current_kind();

                // The file ends with "TOKEN TypeName", it's not legal code, and it's much
                // more likely that this is meant to be a modifier.
                if current_token_kind == SyntaxKind::EndOfFileToken {
                    break 'scope true;
                }

                // "TOKEN TypeName }".  In this case, we just have an incomplete member, and
                // we should definitely default to 'TOKEN' being considered a return type here.
                if current_token_kind == SyntaxKind::CloseBraceToken {
                    break 'scope true;
                }

                // "TOKEN TypeName void". In this case, we just have an incomplete member before
                // an existing member.  Treat this 'TOKEN' as a keyword.
                if facts::is_predefined_type(self.current_kind()) {
                    break 'scope true;
                }

                // "TOKEN TypeName public".  In this case, we just have an incomplete member before
                // an existing member.  Treat this 'TOKEN' as a keyword.
                let current = self.current_token();
                if self.is_non_contextual_modifier(current) {
                    break 'scope true;
                }

                // "TOKEN TypeName class". In this case, we just have an incomplete member before
                // an existing type declaration.  Treat this 'TOKEN' as a keyword.
                if self.is_type_declaration_start() {
                    break 'scope true;
                }

                // "TOKEN TypeName namespace". In this case, we just have an incomplete member before
                // an existing namespace declaration.  Treat this 'TOKEN' as a keyword.
                if current_token_kind == SyntaxKind::NamespaceKeyword {
                    break 'scope true;
                }

                if !parsing_statement_not_declaration && current_token_kind == SyntaxKind::OperatorKeyword {
                    break 'scope true;
                }
            }

            false
        };
        self.dispose_reset_point(reset_point);
        result
    }

    /// `private static bool IsNonContextualModifier(SyntaxToken nextToken)` (LanguageParser.cs:1659).
    pub(crate) fn is_non_contextual_modifier(&self, next_token: Green) -> bool {
        !facts::is_contextual_keyword(self.base.arena_ref().contextual_kind(next_token))
            && self.get_modifier_excluding_scoped(next_token) != DeclarationModifiers::NONE
    }

    /// `private bool IsPartialType()` (LanguageParser.cs:1664).
    pub(crate) fn is_partial_type(&mut self) -> bool {
        debug_assert!(self.current_contextual_kind() == SyntaxKind::PartialKeyword);
        let next_token = self.peek_token(1);
        let a = self.base.arena_ref();
        match a.kind(next_token) {
            SyntaxKind::StructKeyword | SyntaxKind::ClassKeyword | SyntaxKind::InterfaceKeyword => return true,
            _ => {}
        }

        match a.contextual_kind(next_token) {
            // This is an unusual use of LangVersion. Normally we only produce errors when the langversion
            // does not support a feature, but in this case we are effectively making a language breaking
            // change to consider "record" a type declaration in all ambiguous cases. To avoid breaking
            // older code that is not using C# 9 we conditionally parse based on langversion
            SyntaxKind::RecordKeyword => self.is_feature_enabled(MessageID::IDS_FeatureRecords),
            // This is an unusual use of LangVersion. Normally we only produce errors when the langversion
            // does not support a feature, but in this case we are effectively making a language breaking
            // change to consider "union" a type declaration in all ambiguous cases. To avoid breaking
            // older code that is not using C# 15 we conditionally parse based on langversion
            SyntaxKind::UnionKeyword => self.is_feature_enabled(MessageID::IDS_FeatureUnions),
            _ => false,
        }
    }

    /// `private bool IsPartialMember()` (LanguageParser.cs:1700).
    pub(crate) fn is_partial_member(&mut self) -> bool {
        debug_assert!(self.current_contextual_kind() == SyntaxKind::PartialKeyword);

        // Check for:
        //   partial event
        if self.peek_kind(1) == SyntaxKind::EventKeyword {
            return true;
        }

        // Check for constructor:
        //   partial Identifier(
        if self.peek_kind(1) == SyntaxKind::IdentifierToken && self.peek_kind(2) == SyntaxKind::OpenParenToken {
            return self.is_feature_enabled(MessageID::IDS_FeaturePartialEventsAndConstructors);
        }

        // Check for method/property:
        //   partial ReturnType MemberName
        let reset_point = self.get_disposable_reset_point(true);
        let result = 'scope: {
            self.eat_token(); // partial

            if self.scan_type(false) == ScanTypeFlags::NOT_TYPE {
                break 'scope false;
            }

            self.is_possible_member_name()
        };
        self.dispose_reset_point(reset_point);
        result
    }

    /// `private bool IsPossibleMemberName()` (LanguageParser.cs:1733).
    pub(crate) fn is_possible_member_name(&mut self) -> bool {
        match self.current_kind() {
            SyntaxKind::IdentifierToken => {
                if self.current_contextual_kind() == SyntaxKind::GlobalKeyword
                    && self.peek_kind(1) == SyntaxKind::UsingKeyword
                {
                    return false;
                }

                true
            }
            SyntaxKind::ThisKeyword => true,
            _ => false,
        }
    }

    /// `private MemberDeclarationSyntax ParseTypeDeclaration(SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers)` (LanguageParser.cs:1751).
    pub(crate) fn parse_type_declaration(&mut self, attributes: Green, modifiers: &mut SyntaxListBuilder) -> Green {
        // "top-level" expressions and statements should never occur inside an asynchronous context
        debug_assert!(!self.is_in_async);

        match self.current_kind() {
            SyntaxKind::ClassKeyword => self.parse_main_type_declaration(attributes, modifiers),

            SyntaxKind::StructKeyword => self.parse_main_type_declaration(attributes, modifiers),

            SyntaxKind::InterfaceKeyword => self.parse_main_type_declaration(attributes, modifiers),

            SyntaxKind::DelegateKeyword => self.parse_delegate_declaration(attributes, modifiers),

            SyntaxKind::EnumKeyword => self.parse_enum_declaration(attributes, modifiers),

            SyntaxKind::IdentifierToken => {
                debug_assert!(matches!(
                    self.current_contextual_kind(),
                    SyntaxKind::RecordKeyword | SyntaxKind::ExtensionKeyword | SyntaxKind::UnionKeyword
                ));
                self.parse_main_type_declaration(attributes, modifiers)
            }

            // `throw ExceptionUtilities.UnexpectedValue`: no caller gets here (each checks the kinds above first).
            _ => self.parse_main_type_declaration(attributes, modifiers),
        }
    }

    /// `private TypeDeclarationSyntax ParseMainTypeDeclaration(SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers)` (LanguageParser.cs:1784).
    pub(crate) fn parse_main_type_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
    ) -> Green {
        // "top-level" expressions and statements should never occur inside an asynchronous context
        debug_assert!(!self.is_in_async);

        let mut keyword = Green::NULL;
        let mut record_modifier = Green::NULL;
        if !self.parse_main_type_declaration_try_scan_record_start(&mut keyword, &mut record_modifier) {
            let token = self.eat_token();
            keyword = self.convert_to_keyword(token);
        }

        let keyword_kind = self.base.arena_ref().kind(keyword);
        let is_extension = keyword_kind == SyntaxKind::ExtensionKeyword;
        let is_union = keyword_kind == SyntaxKind::UnionKeyword;
        let outer_save_term = self.term_state;
        self.term_state = self.term_state | TerminatorState::IS_END_OF_TYPE_SIGNATURE;

        let save_term = self.term_state;
        self.term_state = self.term_state | TerminatorState::IS_POSSIBLE_AGGREGATE_CLAUSE_START_OR_STOP;

        let name;
        if is_extension {
            name = Green::NULL;
            if self.current_kind() == SyntaxKind::IdentifierToken {
                let token = self.eat_token();
                let token = self.add_error(token, ErrorCode::ERR_ExtensionDisallowsName);
                keyword = self.add_trailing_skipped_syntax(keyword, token);
            }
        } else {
            name = self.parse_identifier_token(ErrorCode::ERR_IdentifierExpected);
        }

        let type_parameters = self.parse_type_parameter_list();

        // For extension declarations, there must be a parameter list
        let param_list = if self.current_kind() == SyntaxKind::OpenParenToken || is_extension {
            self.parse_parenthesized_parameter_list(is_extension || is_union)
        } else {
            Green::NULL
        };

        let base_list = if is_extension { Green::NULL } else { self.parse_base_list() };
        self.term_state = save_term;

        // Parse class body
        let mut parse_members = true;
        let mut members = self.pool.allocate();
        let mut constraints = self.pool.allocate();
        if self.current_contextual_kind() == SyntaxKind::WhereKeyword {
            self.parse_type_parameter_constraint_clauses(&mut constraints);
        }

        self.term_state = outer_save_term;

        let semicolon;
        let mut open_brace;
        let close_brace;
        if self.current_kind() == SyntaxKind::SemicolonToken {
            semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
            open_brace = Green::NULL;
            close_brace = Green::NULL;
        } else {
            open_brace = self.eat_token_kind(SyntaxKind::OpenBraceToken);

            // ignore members if missing open curly
            if self.base.arena_ref().is_missing(open_brace) {
                parse_members = false;
            }

            // even if we saw a { or think we should parse members bail out early since
            // we know namespaces can't be nested inside types
            if parse_members {
                loop {
                    let kind = self.current_kind();

                    if Self::can_start_member(kind) {
                        // This token can start a member -- go parse it
                        let save_term2 = self.term_state;
                        self.term_state = self.term_state | TerminatorState::IS_POSSIBLE_MEMBER_START_OR_STOP;

                        let member = self.parse_member_declaration_with_parent_kind(keyword_kind);
                        if !member.is_null() {
                            // statements are accepted here, a semantic error will be reported later
                            members.push(member);
                        } else {
                            // we get here if we couldn't parse the lookahead as a statement or a declaration (we haven't consumed any tokens):
                            self.skip_bad_member_list_tokens(&mut open_brace, &mut members);
                        }

                        self.term_state = save_term2;
                    } else if matches!(kind, SyntaxKind::CloseBraceToken | SyntaxKind::EndOfFileToken)
                        || self.is_terminator()
                    {
                        // This marks the end of members of this class
                        break;
                    } else {
                        // Error -- try to sync up with intended reality
                        self.skip_bad_member_list_tokens(&mut open_brace, &mut members);
                    }
                }
            }

            close_brace = if self.base.arena_ref().is_missing(open_brace) {
                let actual = self.current_kind();
                self.create_missing_token(SyntaxKind::CloseBraceToken, actual)
            } else {
                self.eat_token_kind(SyntaxKind::CloseBraceToken)
            };

            semicolon = self.try_eat_token(SyntaxKind::SemicolonToken);
        }

        let result = self.parse_main_type_declaration_construct_type_declaration(
            attributes,
            modifiers,
            keyword,
            record_modifier,
            name,
            type_parameters,
            param_list,
            base_list,
            &constraints,
            open_brace,
            &members,
            close_brace,
            semicolon,
        );
        self.pool.free(members);
        self.pool.free(constraints);
        result
    }

    /// `tryScanRecordStart` (a local function of `ParseMainTypeDeclaration`, LanguageParser.cs:1926).
    fn parse_main_type_declaration_try_scan_record_start(
        &mut self,
        keyword: &mut Green,
        record_modifier: &mut Green,
    ) -> bool {
        if self.current_contextual_kind() == SyntaxKind::RecordKeyword {
            let token = self.eat_token();
            *keyword = self.convert_to_keyword(token);
            *record_modifier = if matches!(self.current_kind(), SyntaxKind::ClassKeyword | SyntaxKind::StructKeyword) {
                self.eat_token()
            } else {
                Green::NULL
            };

            return true;
        }

        if matches!(self.current_kind(), SyntaxKind::StructKeyword | SyntaxKind::ClassKeyword)
            && self.peek_contextual_kind(1) == SyntaxKind::RecordKeyword
            && self.peek_kind(2) == SyntaxKind::IdentifierToken
        {
            // Provide a specific diagnostic on `struct record S` or `class record C`
            let misplaced_token = self.eat_token();

            // Parse out 'record' but place 'struct/class' as leading skipped trivia on it.
            let token = self.eat_token();
            let token = self.convert_to_keyword(token);
            let token = self.add_error(token, ErrorCode::ERR_MisplacedRecord);
            *keyword = self.add_leading_skipped_syntax(token, misplaced_token);

            // Treat `struct record` as a RecordStructDeclaration, and `class record` as a RecordDeclaration.
            let misplaced_kind = self.base.arena_ref().kind(misplaced_token);
            *record_modifier = self.base.arena().missing_token(misplaced_kind);
            return true;
        }

        *keyword = Green::NULL;
        *record_modifier = Green::NULL;
        false
    }

    /// `constructTypeDeclaration` (a local function of `ParseMainTypeDeclaration`, LanguageParser.cs:1960).
    fn parse_main_type_declaration_construct_type_declaration(
        &mut self,
        attributes: Green,
        modifiers: &SyntaxListBuilder,
        keyword: Green,
        record_modifier: Green,
        name: Green,
        type_parameters: Green,
        param_list: Green,
        base_list: Green,
        constraints: &SyntaxListBuilder,
        open_brace: Green,
        members: &SyntaxListBuilder,
        close_brace: Green,
        semicolon: Green,
    ) -> Green {
        let a = self.base.arena();
        let modifiers_list = a.list(modifiers);
        let members_list = a.list(members);
        let constraints_list = a.list(constraints);
        match a.kind(keyword) {
            SyntaxKind::StructKeyword => a.struct_declaration(
                attributes,
                modifiers_list,
                keyword,
                name,
                type_parameters,
                param_list,
                base_list,
                constraints_list,
                open_brace,
                members_list,
                close_brace,
                semicolon,
            ),

            SyntaxKind::UnionKeyword => a.union_declaration(
                attributes,
                modifiers_list,
                keyword,
                name,
                type_parameters,
                param_list,
                base_list,
                constraints_list,
                open_brace,
                members_list,
                close_brace,
                semicolon,
            ),

            SyntaxKind::InterfaceKeyword => a.interface_declaration(
                attributes,
                modifiers_list,
                keyword,
                name,
                type_parameters,
                param_list,
                base_list,
                constraints_list,
                open_brace,
                members_list,
                close_brace,
                semicolon,
            ),

            SyntaxKind::RecordKeyword => {
                // record struct ...
                // record ...
                // record class ...
                let declaration_kind = if a.kind(record_modifier) == SyntaxKind::StructKeyword {
                    SyntaxKind::RecordStructDeclaration
                } else {
                    SyntaxKind::RecordDeclaration
                };
                a.record_declaration(
                    declaration_kind,
                    attributes,
                    modifiers_list,
                    keyword,
                    record_modifier,
                    name,
                    type_parameters,
                    param_list,
                    base_list,
                    constraints_list,
                    open_brace,
                    members_list,
                    close_brace,
                    semicolon,
                )
            }

            SyntaxKind::ExtensionKeyword => a.extension_block_declaration(
                attributes,
                modifiers_list,
                keyword,
                type_parameters,
                param_list,
                constraints_list,
                open_brace,
                members_list,
                close_brace,
                semicolon,
            ),

            // ClassKeyword; any other kind is `throw ExceptionUtilities.UnexpectedValue`, which no input reaches.
            _ => a.class_declaration(
                attributes,
                modifiers_list,
                keyword,
                name,
                type_parameters,
                param_list,
                base_list,
                constraints_list,
                open_brace,
                members_list,
                close_brace,
                semicolon,
            ),
        }
    }

    /// `private void SkipBadMemberListTokens(ref SyntaxToken openBrace, SyntaxListBuilder members)` (LanguageParser.cs:2078).
    pub(crate) fn skip_bad_member_list_tokens(&mut self, open_brace: &mut Green, members: &mut SyntaxListBuilder) {
        if let Some(last) = members.last_mut() {
            let mut tmp = *last;
            self.skip_bad_member_list_tokens_with_previous_node(&mut tmp);
            *last = tmp;
        } else {
            let mut tmp = *open_brace;
            self.skip_bad_member_list_tokens_with_previous_node(&mut tmp);
            *open_brace = tmp;
        }
    }

    /// `private void SkipBadMemberListTokens(ref GreenNode previousNode)` (LanguageParser.cs:2094).
    pub(crate) fn skip_bad_member_list_tokens_with_previous_node(&mut self, previous_node: &mut Green) {
        let mut curly_count = 0i32;
        let mut tokens = self.pool.allocate();

        let mut done = false;

        // always consume at least one token.
        let token = self.eat_token();
        let token = self.add_error_args(token, ErrorCode::ERR_InvalidMemberDecl, &[DiagArg::TokenText(token)]);
        tokens.push(token);

        while !done {
            let kind = self.current_kind();

            // If this token can start a member, we're done
            if Self::can_start_member(kind)
                && !(kind == SyntaxKind::DelegateKeyword
                    && matches!(self.peek_kind(1), SyntaxKind::OpenBraceToken | SyntaxKind::OpenParenToken))
            {
                done = true;
                continue;
            }

            // <UNDONE>  UNDONE: Seems like this makes sense,
            // but if this token can start a namespace element, but not a member, then
            // perhaps we should bail back up to parsing a namespace body somehow...</UNDONE>

            // Watch curlies and look for end of file/close curly
            match kind {
                SyntaxKind::OpenBraceToken => curly_count += 1,

                SyntaxKind::CloseBraceToken => {
                    let was = curly_count;
                    curly_count -= 1;
                    if was == 0 {
                        done = true;
                        continue;
                    }
                }

                SyntaxKind::EndOfFileToken => {
                    done = true;
                    continue;
                }

                _ => {}
            }

            let token = self.eat_token();
            tokens.push(token);
        }

        let skipped = self.pool.to_list_and_free(self.base.lexer.arena(), tokens);
        *previous_node = self.add_trailing_skipped_syntax(*previous_node, skipped);
    }

    /// `private bool IsPossibleMemberStartOrStop()` (LanguageParser.cs:2154).
    pub(crate) fn is_possible_member_start_or_stop(&mut self) -> bool {
        self.is_possible_member_start() || self.current_kind() == SyntaxKind::CloseBraceToken
    }

    /// `private bool IsPossibleAggregateClauseStartOrStop()` (LanguageParser.cs:2159).
    pub(crate) fn is_possible_aggregate_clause_start_or_stop(&mut self) -> bool {
        matches!(self.current_kind(), SyntaxKind::ColonToken | SyntaxKind::OpenBraceToken)
            || self.is_current_token_where_of_constraint_clause()
    }

    /// `private BaseListSyntax ParseBaseList()` (LanguageParser.cs:2165).
    pub(crate) fn parse_base_list(&mut self) -> Green {
        // We are only called from ParseMainTypeDeclaration which unilaterally sets this.
        debug_assert!(self.term_state.contains(TerminatorState::IS_END_OF_TYPE_SIGNATURE));

        let mut colon = self.try_eat_token(SyntaxKind::ColonToken);
        if colon.is_null() {
            return Green::NULL;
        }

        let mut list = self.pool.allocate();

        // Grammar requires at least one base type follow the colon.
        let first_type = self.parse_type(ParseTypeMode::Normal);
        let first = if self.current_kind() == SyntaxKind::OpenParenToken {
            let arguments = self.parse_parenthesized_argument_list();
            self.base.arena().primary_constructor_base_type(first_type, arguments)
        } else {
            self.base.arena().simple_base_type(first_type)
        };
        list.push(first);

        // Parse any optional base types that follow.
        loop {
            if matches!(self.current_kind(), SyntaxKind::OpenBraceToken | SyntaxKind::SemicolonToken)
                || self.is_current_token_where_of_constraint_clause()
            {
                break;
            }

            if self.current_kind() == SyntaxKind::CommaToken {
                let comma = self.eat_token_kind(SyntaxKind::CommaToken);
                list.push(comma);
                let type_ = self.parse_type(ParseTypeMode::Normal);
                let base = self.base.arena().simple_base_type(type_);
                list.push(base);
                continue;
            }

            // Error recovery.  Code had an element in the base list, but wasn't followed by a comma or the start of
            // any production that normally follows in a type declaration.  See if this is just a case of a missing
            // comma between types in the base list.
            //
            // Note: if we see something that looks more like a modifier than a type (like 'file') do not try to
            // consume it as a type here, as we want to use that to better determine what member is actually following
            // this incomplete type declaration.
            let current = self.current_token();
            if self.get_modifier_excluding_scoped(current) != DeclarationModifiers::NONE {
                break;
            }

            if self.is_possible_type() {
                let comma = self.eat_token_kind(SyntaxKind::CommaToken);
                list.push(comma);
                let type_ = self.parse_type(ParseTypeMode::Normal);
                let base = self.base.arena().simple_base_type(type_);
                list.push(base);
                continue;
            }

            // skipBadBaseListTokens
            if self.skip_bad_separated_list_tokens_with_expected_kind(
                &mut colon,
                &mut list,
                |p| p.current_kind() != SyntaxKind::CommaToken && !p.is_possible_attribute(),
                |p, _| {
                    p.current_kind() == SyntaxKind::OpenBraceToken || p.is_current_token_where_of_constraint_clause()
                },
                SyntaxKind::CommaToken,
                SyntaxKind::None,
            ) == PostSkipAction::Abort
            {
                break;
            }
        }

        let types = self.pool.to_list_and_free(self.base.lexer.arena(), list);
        self.base.arena().base_list(colon, types)
    }

    /// `private bool IsCurrentTokenWhereOfConstraintClause()` (LanguageParser.cs:2234).
    pub(crate) fn is_current_token_where_of_constraint_clause(&mut self) -> bool {
        self.current_contextual_kind() == SyntaxKind::WhereKeyword
            && self.peek_kind(1) == SyntaxKind::IdentifierToken
            && self.peek_kind(2) == SyntaxKind::ColonToken
    }

    /// `private void ParseTypeParameterConstraintClauses(SyntaxListBuilder list)` (LanguageParser.cs:2242).
    pub(crate) fn parse_type_parameter_constraint_clauses(&mut self, list: &mut SyntaxListBuilder) {
        while self.current_contextual_kind() == SyntaxKind::WhereKeyword {
            let clause = self.parse_type_parameter_constraint_clause();
            list.push(clause);
        }
    }

    /// `private TypeParameterConstraintClauseSyntax ParseTypeParameterConstraintClause()` (LanguageParser.cs:2250).
    pub(crate) fn parse_type_parameter_constraint_clause(&mut self) -> Green {
        let where_ = self.eat_contextual_token(SyntaxKind::WhereKeyword);
        let name = if !self.is_true_identifier() {
            let missing = self.create_missing_identifier_name();
            self.add_error(missing, ErrorCode::ERR_IdentifierExpected)
        } else {
            self.parse_identifier_name(ErrorCode::ERR_IdentifierExpected)
        };

        let colon = self.eat_token_kind(SyntaxKind::ColonToken);

        let mut bounds = self.pool.allocate();

        // first bound
        if self.current_kind() == SyntaxKind::OpenBraceToken || self.is_current_token_where_of_constraint_clause() {
            let missing = self.create_missing_identifier_name();
            let missing = self.add_error(missing, ErrorCode::ERR_TypeExpected);
            let constraint = self.base.arena().type_constraint(missing);
            bounds.push(constraint);
        } else {
            let mut constraint = self.parse_type_parameter_constraint();
            bounds.push(constraint);

            // remaining bounds
            loop {
                let have_comma;

                if self.current_kind() == SyntaxKind::OpenBraceToken
                    || (self.term_state.contains(TerminatorState::IS_END_OF_TYPE_SIGNATURE)
                        && self.current_kind() == SyntaxKind::SemicolonToken)
                    || self.current_kind() == SyntaxKind::EqualsGreaterThanToken
                    || self.current_contextual_kind() == SyntaxKind::WhereKeyword
                {
                    break;
                } else if {
                    have_comma = self.current_kind() == SyntaxKind::CommaToken;
                    have_comma
                } || self.is_possible_type_parameter_constraint()
                {
                    let separator_token = self.eat_token_kind(SyntaxKind::CommaToken);

                    if self.base.arena_ref().kind(constraint) == SyntaxKind::AllowsConstraintClause
                        && have_comma
                        && !self.is_possible_type_parameter_constraint()
                    {
                        let skipped = self.add_error_args(
                            separator_token,
                            ErrorCode::ERR_UnexpectedToken,
                            &[DiagArg::Str(facts::get_text(SyntaxKind::CommaToken))],
                        );
                        self.add_trailing_skipped_syntax_to_list(&mut bounds, skipped);
                        break;
                    }

                    bounds.push(separator_token);
                    if self.is_current_token_where_of_constraint_clause() {
                        let missing = self.create_missing_identifier_name();
                        let missing = self.add_error(missing, ErrorCode::ERR_TypeExpected);
                        let constraint = self.base.arena().type_constraint(missing);
                        bounds.push(constraint);
                        break;
                    } else {
                        constraint = self.parse_type_parameter_constraint();
                        bounds.push(constraint);
                    }
                } else {
                    // skipBadTypeParameterConstraintTokens
                    let mut tmp = Green::NULL;
                    debug_assert!(!bounds.is_empty());
                    if self.skip_bad_separated_list_tokens_with_expected_kind(
                        &mut tmp,
                        &mut bounds,
                        |p| p.current_kind() != SyntaxKind::CommaToken && !p.is_possible_type_parameter_constraint(),
                        |p, _| {
                            p.current_kind() == SyntaxKind::OpenBraceToken
                                || p.is_current_token_where_of_constraint_clause()
                        },
                        SyntaxKind::CommaToken,
                        SyntaxKind::None,
                    ) == PostSkipAction::Abort
                    {
                        break;
                    }
                }
            }
        }

        let bounds = self.pool.to_list_and_free(self.base.lexer.arena(), bounds);
        self.base.arena().type_parameter_constraint_clause(where_, name, colon, bounds)
    }

    /// `private bool IsPossibleTypeParameterConstraint()` (LanguageParser.cs:2329).
    pub(crate) fn is_possible_type_parameter_constraint(&mut self) -> bool {
        match self.current_kind() {
            SyntaxKind::NewKeyword
            | SyntaxKind::ClassKeyword
            | SyntaxKind::StructKeyword
            | SyntaxKind::DefaultKeyword => true,
            SyntaxKind::IdentifierToken => {
                (self.current_contextual_kind() == SyntaxKind::AllowsKeyword
                    && self.peek_kind(1) == SyntaxKind::RefKeyword)
                    || self.is_true_identifier()
            }
            kind => Self::is_predefined_type(kind),
        }
    }

    /// `private TypeParameterConstraintSyntax ParseTypeParameterConstraint()` (LanguageParser.cs:2346).
    pub(crate) fn parse_type_parameter_constraint(&mut self) -> Green {
        match self.current_kind() {
            SyntaxKind::NewKeyword => {
                let new_keyword = self.eat_token();
                let open = self.eat_token_kind(SyntaxKind::OpenParenToken);
                let close = self.eat_token_kind(SyntaxKind::CloseParenToken);
                self.base.arena().constructor_constraint(new_keyword, open, close)
            }

            SyntaxKind::StructKeyword => {
                let class_or_struct_keyword = self.eat_token();
                let question = if self.current_kind() == SyntaxKind::QuestionToken {
                    let token = self.eat_token();
                    self.add_error_args(
                        token,
                        ErrorCode::ERR_UnexpectedToken,
                        &[DiagArg::Str(facts::get_text(SyntaxKind::QuestionToken))],
                    )
                } else {
                    Green::NULL
                };
                self.base.arena().class_or_struct_constraint(
                    SyntaxKind::StructConstraint,
                    class_or_struct_keyword,
                    question,
                )
            }

            SyntaxKind::ClassKeyword => {
                let class_or_struct_keyword = self.eat_token();
                let question = self.try_eat_token(SyntaxKind::QuestionToken);
                self.base.arena().class_or_struct_constraint(
                    SyntaxKind::ClassConstraint,
                    class_or_struct_keyword,
                    question,
                )
            }

            SyntaxKind::DefaultKeyword => {
                let default_keyword = self.eat_token();
                self.base.arena().default_constraint(default_keyword)
            }

            SyntaxKind::EnumKeyword => {
                let missing = self.create_missing_identifier_name();
                let missing = self.add_error(missing, ErrorCode::ERR_NoEnumConstraint);
                let token = self.eat_token();
                let type_ = self.add_trailing_skipped_syntax(missing, token);
                self.base.arena().type_constraint(type_)
            }

            // Produce a specific diagnostic for `where T : delegate`
            // but not `where T : delegate*<...>
            SyntaxKind::DelegateKeyword => {
                if self.peek_kind(1) == SyntaxKind::AsteriskToken {
                    let type_ = self.parse_type(ParseTypeMode::Normal);
                    self.base.arena().type_constraint(type_)
                } else {
                    let missing = self.create_missing_identifier_name();
                    let missing = self.add_error(missing, ErrorCode::ERR_NoDelegateConstraint);
                    let token = self.eat_token();
                    let type_ = self.add_trailing_skipped_syntax(missing, token);
                    self.base.arena().type_constraint(type_)
                }
            }

            _ => self.parse_type_parameter_constraint_parse_type_or_allows_constraint(),
        }
    }

    /// `parseTypeOrAllowsConstraint` (a local function of `ParseTypeParameterConstraint`, LanguageParser.cs:2390).
    fn parse_type_parameter_constraint_parse_type_or_allows_constraint(&mut self) -> Green {
        if self.current_contextual_kind() == SyntaxKind::AllowsKeyword && self.peek_kind(1) == SyntaxKind::RefKeyword {
            let allows = self.eat_contextual_token(SyntaxKind::AllowsKeyword);

            let mut bounds = self.pool.allocate();

            loop {
                let ref_keyword = self.eat_token_kind(SyntaxKind::RefKeyword);
                let struct_keyword = self.eat_token_kind(SyntaxKind::StructKeyword);
                let constraint = self.base.arena().ref_struct_constraint(ref_keyword, struct_keyword);
                bounds.push(constraint);

                if self.current_kind() == SyntaxKind::CommaToken && self.peek_kind(1) == SyntaxKind::RefKeyword {
                    let comma = self.eat_token_kind(SyntaxKind::CommaToken);
                    bounds.push(comma);
                    continue;
                }

                break;
            }

            let bounds = self.pool.to_list_and_free(self.base.lexer.arena(), bounds);
            return self.base.arena().allows_constraint_clause(allows, bounds);
        }

        let type_ = self.parse_type(ParseTypeMode::Normal);
        self.base.arena().type_constraint(type_)
    }

    /// `private bool IsPossibleMemberStart()` (LanguageParser.cs:2422).
    pub(crate) fn is_possible_member_start(&mut self) -> bool {
        Self::can_start_member(self.current_kind())
    }

    /// `private static bool CanStartMember(SyntaxKind kind)` (LanguageParser.cs:2427).
    pub(crate) fn can_start_member(kind: SyntaxKind) -> bool {
        matches!(
            kind,
            SyntaxKind::AbstractKeyword
                | SyntaxKind::BoolKeyword
                | SyntaxKind::ByteKeyword
                | SyntaxKind::CharKeyword
                | SyntaxKind::ClassKeyword
                | SyntaxKind::ConstKeyword
                | SyntaxKind::DecimalKeyword
                | SyntaxKind::DelegateKeyword
                | SyntaxKind::DoubleKeyword
                | SyntaxKind::EnumKeyword
                | SyntaxKind::EventKeyword
                | SyntaxKind::ExternKeyword
                | SyntaxKind::FixedKeyword
                | SyntaxKind::FloatKeyword
                | SyntaxKind::IntKeyword
                | SyntaxKind::InterfaceKeyword
                | SyntaxKind::InternalKeyword
                | SyntaxKind::LongKeyword
                | SyntaxKind::NewKeyword
                | SyntaxKind::ObjectKeyword
                | SyntaxKind::OverrideKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::PublicKeyword
                | SyntaxKind::ReadOnlyKeyword
                | SyntaxKind::SByteKeyword
                | SyntaxKind::SealedKeyword
                | SyntaxKind::ShortKeyword
                | SyntaxKind::StaticKeyword
                | SyntaxKind::StringKeyword
                | SyntaxKind::StructKeyword
                | SyntaxKind::UIntKeyword
                | SyntaxKind::ULongKeyword
                | SyntaxKind::UnsafeKeyword
                | SyntaxKind::UShortKeyword
                | SyntaxKind::VirtualKeyword
                | SyntaxKind::VoidKeyword
                | SyntaxKind::VolatileKeyword
                | SyntaxKind::IdentifierToken
                | SyntaxKind::TildeToken
                | SyntaxKind::OpenBracketToken
                | SyntaxKind::ImplicitKeyword
                | SyntaxKind::ExplicitKeyword
                | SyntaxKind::OpenParenToken
                | SyntaxKind::RefKeyword
        )
    }

    /// `private bool IsTypeDeclarationStart()` (LanguageParser.cs:2483).
    pub(crate) fn is_type_declaration_start(&mut self) -> bool {
        match self.current_kind() {
            SyntaxKind::ClassKeyword
            | SyntaxKind::EnumKeyword
            | SyntaxKind::InterfaceKeyword
            | SyntaxKind::StructKeyword => true,

            SyntaxKind::DelegateKeyword => !self.is_function_pointer_start(),

            SyntaxKind::IdentifierToken => {
                match self.current_contextual_kind() {
                    // This is an unusual use of LangVersion. Normally we only produce errors when the langversion
                    // does not support a feature, but in this case we are effectively making a language breaking
                    // change to consider "record" a type declaration in all ambiguous cases. To avoid breaking
                    // older code that is not using C# 9 we conditionally parse based on langversion
                    SyntaxKind::RecordKeyword => return self.is_feature_enabled(MessageID::IDS_FeatureRecords),
                    // This is an unusual use of LangVersion. Normally we only produce errors when the langversion
                    // does not support a feature, but in this case we are effectively making a language breaking
                    // change to consider "union" a type declaration in all ambiguous cases. To avoid breaking
                    // older code that is not using C# 15 we conditionally parse based on langversion
                    SyntaxKind::UnionKeyword => return self.is_feature_enabled(MessageID::IDS_FeatureUnions),
                    _ => {}
                }

                if self.is_extension_container_start() {
                    return true;
                }

                false
            }

            _ => false,
        }
    }

    /// `private bool CanReuseMemberDeclaration(SyntaxKind kind, bool isGlobal)` (LanguageParser.cs:2529): incremental
    /// parsing only; the global field and method case reads the old tree's parent, which a full parse has not.
    #[allow(dead_code)] // incremental parsing only: there is no old tree
    pub(crate) fn can_reuse_member_declaration(&mut self, kind: SyntaxKind, is_global: bool) -> bool {
        match kind {
            SyntaxKind::ClassDeclaration
            | SyntaxKind::StructDeclaration
            | SyntaxKind::UnionDeclaration
            | SyntaxKind::InterfaceDeclaration
            | SyntaxKind::EnumDeclaration
            | SyntaxKind::DelegateDeclaration
            | SyntaxKind::EventFieldDeclaration
            | SyntaxKind::PropertyDeclaration
            | SyntaxKind::EventDeclaration
            | SyntaxKind::IndexerDeclaration
            | SyntaxKind::OperatorDeclaration
            | SyntaxKind::ConversionOperatorDeclaration
            | SyntaxKind::DestructorDeclaration
            | SyntaxKind::ConstructorDeclaration
            | SyntaxKind::NamespaceDeclaration
            | SyntaxKind::FileScopedNamespaceDeclaration
            | SyntaxKind::RecordDeclaration
            | SyntaxKind::RecordStructDeclaration => true,
            SyntaxKind::FieldDeclaration | SyntaxKind::MethodDeclaration => !is_global || self.is_script(),
            SyntaxKind::GlobalStatement => is_global,
            _ => false,
        }
    }

    /// `public MemberDeclarationSyntax ParseMemberDeclaration()` (LanguageParser.cs:2570).
    pub(crate) fn parse_member_declaration(&mut self) -> Green {
        // Use a parent kind that causes inclusion of only member declarations that could appear in a struct
        // e.g. including fixed member declarations, but not statements.
        self.parse_with_stack_guard(
            |this| this.parse_member_declaration_with_parent_kind(SyntaxKind::StructDeclaration),
            // Creates a dummy declaration node to which we can attach a stack overflow message
            |this| {
                let missing = this.create_missing_identifier_name();
                this.base.arena().incomplete_member(Green::NULL, Green::NULL, missing)
            },
        )
    }

    /// `internal MemberDeclarationSyntax ParseMemberDeclarationOrStatement(SyntaxKind parentKind)` (LanguageParser.cs:2590).
    pub(crate) fn parse_member_declaration_or_statement(&mut self, parent_kind: SyntaxKind) -> Green {
        self.recursion_depth += 1;
        self.ensure_sufficient_execution_stack();
        let result = self.parse_member_declaration_or_statement_core(parent_kind);
        self.recursion_depth -= 1;
        result
    }

    /// `private MemberDeclarationSyntax ParseMemberDeclarationOrStatementCore(SyntaxKind parentKind)` (LanguageParser.cs:2605).
    pub(crate) fn parse_member_declaration_or_statement_core(&mut self, parent_kind: SyntaxKind) -> Green {
        // "top-level" expressions and statements should never occur inside an asynchronous context
        debug_assert!(!self.is_in_async);
        debug_assert!(parent_kind == SyntaxKind::CompilationUnit);

        let save_term_state = self.term_state;

        let attributes = self.parse_statement_attribute_declarations();
        let have_attributes = !attributes.is_null();

        let mut after_attributes_point = self.get_reset_point();

        let mut modifiers = self.pool.allocate();

        let result = 'outer: {
            //
            // Check for the following cases to disambiguate between member declarations and expressions.
            // Doing this before parsing modifiers simplifies further analysis since some of these keywords can act as modifiers as well.
            //
            // unsafe { ... }
            // unsafe (...)
            // fixed (...) { ... }
            // delegate (...) { ... }
            // delegate { ... }
            // new { ... }
            // new[] { ... }
            // new T (...)
            // new T [...]
            //
            // `!haveAttributes || !IsScript`: always.
            {
                // We are implicitly in an async context in a non-script environment.
                let resetter = ParserSyntaxContextResetter::new(self, Some(true), None, None, None);

                let statement = match self.current_kind() {
                    SyntaxKind::UnsafeKeyword => {
                        if self.peek_kind(1) == SyntaxKind::OpenBraceToken {
                            self.parse_unsafe_statement(attributes)
                        } else if self.peek_kind(1) == SyntaxKind::OpenParenToken {
                            self.parse_expression_statement_or_local_function_starting_with_unsafe(attributes)
                        } else {
                            Green::NULL
                        }
                    }

                    SyntaxKind::FixedKeyword => {
                        if self.peek_kind(1) == SyntaxKind::OpenParenToken {
                            self.parse_fixed_statement(attributes)
                        } else {
                            Green::NULL
                        }
                    }

                    SyntaxKind::DelegateKeyword => {
                        // Check if this is an anonymous delegate expression or a delegate type declaration.
                        // Anonymous delegate: delegate { } or delegate (params) { }
                        // Delegate declaration: delegate Type Name(params);
                        if self.is_anonymous_delegate_expression() {
                            self.parse_expression_statement(attributes)
                        } else {
                            Green::NULL
                        }
                    }

                    SyntaxKind::NewKeyword => {
                        if self.is_possible_new_expression() {
                            self.parse_expression_statement(attributes)
                        } else {
                            Green::NULL
                        }
                    }

                    _ => Green::NULL,
                };
                if !statement.is_null() {
                    let global = self.base.arena().global_statement(Green::NULL, Green::NULL, statement);
                    resetter.dispose(self);
                    break 'outer global;
                }
                resetter.dispose(self);
            }

            // All modifiers that might start an expression are processed above.
            let mut is_possible_type_declaration = false;
            self.parse_modifiers(&mut modifiers, false, true, &mut is_possible_type_declaration);
            let have_modifiers = !modifiers.is_empty();
            let mut result = Green::NULL;

            // Check for constructor form
            if self.current_kind() == SyntaxKind::IdentifierToken && self.peek_kind(1) == SyntaxKind::OpenParenToken {
                // Script:
                // Constructor definitions are not allowed. We parse them as method calls with semicolon missing error:
                //
                // Script(...) { ... }
                //            ^
                //            missing ';'
                //
                // Unless modifiers or attributes are present this is more likely to be a method call than a method definition.
                if have_attributes || have_modifiers {
                    // `voidType`, which only a script parses a method with.
                    let missing = self.base.arena().missing_token(SyntaxKind::VoidKeyword);
                    let missing = self.add_error(missing, ErrorCode::ERR_MemberNeedsType);
                    let _void_type = self.base.arena().predefined_type(missing);

                    // !IsScript
                    if self.parse_member_declaration_or_statement_core_try_parse_local_declaration_statement_from_start_point(
                        attributes,
                        &mut after_attributes_point,
                        SyntaxKind::LocalFunctionStatement,
                        &mut result,
                    ) {
                        break 'outer result;
                    }
                }
            }

            // Destructors are disallowed in global code, skipping check for them.
            // TODO: better error messages for script

            // Check for constant
            if self.current_kind() == SyntaxKind::ConstKeyword {
                // !IsScript
                if self
                    .parse_member_declaration_or_statement_core_try_parse_local_declaration_statement_from_start_point(
                        attributes,
                        &mut after_attributes_point,
                        SyntaxKind::LocalDeclarationStatement,
                        &mut result,
                    )
                {
                    break 'outer result;
                }

                // Prefers const field over const local variable decl
                break 'outer self.parse_constant_field_declaration(attributes, &mut modifiers, parent_kind);
            }

            // Check for event.
            if self.current_kind() == SyntaxKind::EventKeyword {
                break 'outer self.parse_event_declaration(attributes, &mut modifiers, parent_kind);
            }

            // check for fixed size buffers.
            if self.current_kind() == SyntaxKind::FixedKeyword {
                break 'outer self.parse_fixed_size_buffer_declaration(attributes, &mut modifiers, parent_kind);
            }

            // Check for conversion operators (implicit/explicit)
            result = self.try_parse_conversion_operator_declaration(attributes, &mut modifiers);
            if !result.is_null() {
                break 'outer result;
            }

            if self.current_kind() == SyntaxKind::NamespaceKeyword {
                break 'outer self.parse_namespace_declaration(attributes, &mut modifiers);
            }

            // It's valid to have a type declaration here -- check for those
            if is_possible_type_declaration && self.is_type_declaration_start() {
                break 'outer self.parse_type_declaration(attributes, &mut modifiers);
            }

            let mut type_ = self.parse_return_type();

            let mut after_type_reset_point = self.get_reset_point();

            let inner = 'inner: {
                // Try as a regular statement rather than a member declaration, if appropriate.
                // `(!haveAttributes || !IsScript)`: always.
                if !have_modifiers
                    && (self.base.arena_ref().kind(type_) == SyntaxKind::RefType || {
                        let mut unused = Green::NULL;
                        !self.is_operator_start(&mut unused, false)
                    })
                {
                    self.reset(&mut after_attributes_point);

                    if !matches!(self.current_kind(), SyntaxKind::CloseBraceToken | SyntaxKind::EndOfFileToken)
                        && self.is_possible_statement()
                    {
                        let save_term = self.term_state;
                        self.term_state = self.term_state | TerminatorState::IS_POSSIBLE_STATEMENT_START_OR_STOP; // partial statements can abort if a new statement starts

                        // We are implicitly in an async context in a non-script environment.
                        let resetter = ParserSyntaxContextResetter::new(self, Some(true), None, None, None);

                        // In Script we don't allow local declaration statements at the top level.  We want
                        // to fall out below and parse them instead as fields. For top-level statements, we allow
                        // them, but want to try properties , etc. first.
                        let statement = self.parse_statement_core(attributes, true);

                        self.term_state = save_term;

                        if self.parse_member_declaration_or_statement_core_is_acceptable_non_declaration_statement(
                            statement,
                        ) {
                            let global = self.base.arena().global_statement(Green::NULL, Green::NULL, statement);
                            resetter.dispose(self);
                            break 'inner global;
                        }
                        resetter.dispose(self);
                    }

                    self.reset(&mut after_type_reset_point);
                }

                // Everything that's left -- methods, fields, properties, locals,
                // indexers, and non-conversion operators -- starts with a type
                // (possibly void).

                // Check for misplaced modifiers.  if we see any, then consider this member
                // terminated and restart parsing.
                if self.is_misplaced_modifier(&mut modifiers, attributes, type_, &mut result) {
                    break 'inner result;
                }

                // parse_member_name:
                loop {
                    // If we've seen the ref keyword, we know we must have an indexer, method, property, or local.
                    let type_is_ref = self.base.arena_ref().kind(type_) == SyntaxKind::RefType;
                    let mut explicit_interface_opt = Green::NULL;

                    // Check here for operators
                    // Allow old-style implicit/explicit casting operator syntax, just so we can give a better error
                    if !type_is_ref && self.is_operator_start(&mut explicit_interface_opt, true) {
                        break 'inner self.parse_operator_declaration(
                            attributes,
                            &mut modifiers,
                            type_,
                            explicit_interface_opt,
                        );
                    }

                    // `(!typeIsRef || !IsScript)`: always.
                    if self.is_field_declaration(false, true) {
                        let save_term = self.term_state;

                        // `(!haveAttributes && !haveModifiers) || !IsScript`: always.
                        {
                            // if we are at top-level then statements can occur
                            self.term_state = self.term_state | TerminatorState::IS_POSSIBLE_STATEMENT_START_OR_STOP;

                            // !IsScript
                            self.reset(&mut after_attributes_point);
                            if self.parse_member_declaration_or_statement_core_try_parse_local_declaration_statement(
                                attributes,
                                SyntaxKind::LocalDeclarationStatement,
                                &mut result,
                            ) {
                                break 'inner result;
                            }

                            self.reset(&mut after_type_reset_point);
                        }

                        if !type_is_ref {
                            break 'inner self.parse_normal_field_declaration(
                                attributes,
                                &mut modifiers,
                                type_,
                                parent_kind,
                            );
                        } else {
                            self.term_state = save_term;
                        }
                    }

                    // At this point we can either have indexers, methods, or
                    // properties (or something unknown).  Try to break apart
                    // the following name and determine what to do from there.
                    let mut identifier_or_this_opt = Green::NULL;
                    let mut type_parameter_list_opt = Green::NULL;
                    self.parse_member_name(
                        &mut explicit_interface_opt,
                        &mut identifier_or_this_opt,
                        &mut type_parameter_list_opt,
                        false,
                    );

                    // `!IsScript`: always.
                    if !have_modifiers
                        && !have_attributes
                        && explicit_interface_opt.is_null()
                        && identifier_or_this_opt.is_null()
                        && type_parameter_list_opt.is_null()
                        && !self.base.arena_ref().is_missing(type_)
                        && self.base.arena_ref().kind(type_) != SyntaxKind::RefType
                        && !self.parse_member_declaration_or_statement_core_is_followed_by_possible_using_directive()
                        && self
                            .parse_member_declaration_or_statement_core_try_parse_local_declaration_statement_from_start_point(
                                attributes,
                                &mut after_attributes_point,
                                SyntaxKind::LocalDeclarationStatement,
                                &mut result,
                            )
                    {
                        break 'inner result;
                    }

                    // First, check if we got absolutely nothing.  If so, then
                    // We need to consume a bad member and try again.
                    if self.is_none_or_incomplete_member(
                        parent_kind,
                        attributes,
                        &mut modifiers,
                        type_,
                        explicit_interface_opt,
                        identifier_or_this_opt,
                        type_parameter_list_opt,
                        &mut result,
                    ) {
                        break 'inner result;
                    }

                    // If the modifiers did not include "async", and the type we got was "async", and there was an
                    // error in the identifier or its type parameters, then the user is probably in the midst of typing
                    // an async method.  In that case we reconsider "async" to be a modifier, and treat the identifier
                    // (with the type parameters) as the type (with type arguments).  Then we go back to looking for
                    // the member name again.
                    // For example, if we get
                    //     async Task<
                    // then we want async to be a modifier and Task<MISSING> to be a type.
                    if self.reconsidered_type_as_async_modifier(
                        &mut modifiers,
                        &mut type_,
                        &mut after_type_reset_point,
                        &mut explicit_interface_opt,
                        &mut identifier_or_this_opt,
                        &mut type_parameter_list_opt,
                    ) {
                        continue;
                    }

                    // identifierOrThisOpt may be null if we're doing error recovery for a property with missing identifier
                    debug_assert!(!identifier_or_this_opt.is_null() || self.is_start_of_property_body(0));

                    if self.try_parse_indexer_or_property_declaration(
                        attributes,
                        &mut modifiers,
                        type_,
                        explicit_interface_opt,
                        identifier_or_this_opt,
                        type_parameter_list_opt,
                        &mut result,
                    ) {
                        break 'inner result;
                    }

                    // !IsScript
                    if explicit_interface_opt.is_null()
                        && self
                            .parse_member_declaration_or_statement_core_try_parse_local_declaration_statement_from_start_point(
                                attributes,
                                &mut after_attributes_point,
                                SyntaxKind::LocalFunctionStatement,
                                &mut result,
                            )
                    {
                        break 'inner result;
                    }

                    if !have_modifiers
                        && self.parse_member_declaration_or_statement_core_try_parse_statement(
                            attributes,
                            &mut after_attributes_point,
                            &mut result,
                        )
                    {
                        break 'inner result;
                    }

                    // treat anything else as a method.

                    break 'inner self.parse_method_declaration(
                        attributes,
                        &mut modifiers,
                        type_,
                        explicit_interface_opt,
                        identifier_or_this_opt,
                        type_parameter_list_opt,
                    );
                }
            };
            self.release(&mut after_type_reset_point);
            inner
        };

        self.pool.free(modifiers);
        self.term_state = save_term_state;
        self.release(&mut after_attributes_point);
        result
    }

    /// `tryParseLocalDeclarationStatement<DeclarationSyntax>` (a local function of
    /// `ParseMemberDeclarationOrStatementCore`, LanguageParser.cs:2934): `declaration_kind` is `DeclarationSyntax`'s
    /// kind.
    fn parse_member_declaration_or_statement_core_try_parse_local_declaration_statement(
        &mut self,
        attributes: Green,
        declaration_kind: SyntaxKind,
        result: &mut Green,
    ) -> bool {
        // We are implicitly in an async context
        let resetter = ParserSyntaxContextResetter::new(self, Some(true), None, None, None);
        let mut last_token_position = -1;
        self.is_making_progress(&mut last_token_position, true);

        let top_level_statement = self.parse_local_declaration_statement(attributes);
        let parsed = if self.base.arena_ref().kind(top_level_statement) == declaration_kind
            && self.is_making_progress(&mut last_token_position, false)
        {
            *result = self.base.arena().global_statement(Green::NULL, Green::NULL, top_level_statement);
            true
        } else {
            *result = Green::NULL;
            false
        };
        resetter.dispose(self);
        parsed
    }

    /// `tryParseStatement` (a local function of `ParseMemberDeclarationOrStatementCore`, LanguageParser.cs:2952).
    fn parse_member_declaration_or_statement_core_try_parse_statement(
        &mut self,
        attributes: Green,
        after_attributes_point: &mut ResetPoint,
        result: &mut Green,
    ) -> bool {
        let mut reset_on_failure_point = self.get_disposable_reset_point(false);

        self.reset(after_attributes_point);

        if self.is_possible_statement() {
            let save_term = self.term_state;
            self.term_state = self.term_state | TerminatorState::IS_POSSIBLE_STATEMENT_START_OR_STOP; // partial statements can abort if a new statement starts

            // We are implicitly in an async context
            let resetter = ParserSyntaxContextResetter::new(self, Some(true), None, None, None);
            let statement = self.parse_statement_core(attributes, true);

            self.term_state = save_term;

            if !statement.is_null() {
                *result = self.base.arena().global_statement(Green::NULL, Green::NULL, statement);
                resetter.dispose(self);
                self.dispose_reset_point(reset_on_failure_point);
                return true;
            }
            resetter.dispose(self);
        }

        self.reset_disposable(&mut reset_on_failure_point);

        *result = Green::NULL;
        self.dispose_reset_point(reset_on_failure_point);
        false
    }

    /// `tryParseLocalDeclarationStatementFromStartPoint<DeclarationSyntax>` (a local function of
    /// `ParseMemberDeclarationOrStatementCore`, LanguageParser.cs:2982).
    fn parse_member_declaration_or_statement_core_try_parse_local_declaration_statement_from_start_point(
        &mut self,
        attributes: Green,
        start_point: &mut ResetPoint,
        declaration_kind: SyntaxKind,
        result: &mut Green,
    ) -> bool {
        let mut reset_on_failure_point = self.get_disposable_reset_point(false);

        self.reset(start_point);

        if self.parse_member_declaration_or_statement_core_try_parse_local_declaration_statement(
            attributes,
            declaration_kind,
            result,
        ) {
            self.dispose_reset_point(reset_on_failure_point);
            return true;
        }

        self.reset_disposable(&mut reset_on_failure_point);
        self.dispose_reset_point(reset_on_failure_point);
        false
    }

    /// `isAcceptableNonDeclarationStatement` (a local function of `ParseMemberDeclarationOrStatementCore`,
    /// LanguageParser.cs:2997), `isScript` false.
    fn parse_member_declaration_or_statement_core_is_acceptable_non_declaration_statement(
        &self,
        statement: Green,
    ) -> bool {
        let a = self.base.arena_ref();
        match a.kind(statement) {
            SyntaxKind::None | SyntaxKind::LocalFunctionStatement => false,
            SyntaxKind::ExpressionStatement => {
                // Do not parse a single identifier as an expression statement in a Simple Program, this could be a beginning of a keyword and
                // we want completion to offer it.
                let expression = a.slot(statement, T::ExpressionStatementSyntax::EXPRESSION);
                let semicolon = a.slot(statement, T::ExpressionStatementSyntax::SEMICOLON_TOKEN);
                !(a.kind(expression) == SyntaxKind::IdentifierName && a.is_missing(semicolon))
            }
            SyntaxKind::LocalDeclarationStatement => {
                !a.slot(statement, T::LocalDeclarationStatementSyntax::USING_KEYWORD).is_null()
            }
            _ => true,
        }
    }

    /// `isFollowedByPossibleUsingDirective` (a local function of `ParseMemberDeclarationOrStatementCore`,
    /// LanguageParser.cs:3019).
    fn parse_member_declaration_or_statement_core_is_followed_by_possible_using_directive(&mut self) -> bool {
        if self.current_kind() == SyntaxKind::UsingKeyword {
            return !self.is_possible_top_level_using_local_declaration_statement();
        }

        if self.current_contextual_kind() == SyntaxKind::GlobalKeyword && self.peek_kind(1) == SyntaxKind::UsingKeyword
        {
            let reset_point = self.get_disposable_reset_point(true);

            // Skip 'global' keyword
            self.eat_token();
            let result = !self.is_possible_top_level_using_local_declaration_statement();
            self.dispose_reset_point(reset_point);
            return result;
        }

        false
    }

    /// `private bool IsMisplacedModifier(SyntaxListBuilder modifiers, SyntaxList<AttributeListSyntax> attributes, TypeSyntax type, out MemberDeclarationSyntax result)` (LanguageParser.cs:3039).
    pub(crate) fn is_misplaced_modifier(
        &mut self,
        modifiers: &mut SyntaxListBuilder,
        attributes: Green,
        type_: Green,
        result: &mut Green,
    ) -> bool {
        let current = self.current_token();
        if self.get_modifier_excluding_scoped(current) != DeclarationModifiers::NONE
            && !matches!(
                self.current_contextual_kind(),
                SyntaxKind::PartialKeyword
                    | SyntaxKind::AsyncKeyword
                    | SyntaxKind::RequiredKeyword
                    | SyntaxKind::FileKeyword
                    | SyntaxKind::ClosedKeyword
                    | SyntaxKind::SafeKeyword
            )
            && self.is_complete(type_)
        {
            let misplaced_modifier = self.current_token();
            let a = self.base.arena_ref();
            // We're attaching a diagnostic for the misplaced modifier on the 'type' node.  So the offset will
            // be *relative* to the *start* (not *full start*) of 'type'. That offset will then be the width of
            // type itself, plus any trailing trivia it has, plus the leading trivia of the modifier itself.
            let offset =
                (a.width(type_) + a.trailing_trivia_width(type_) + a.leading_trivia_width(misplaced_modifier)) as i32;
            let width = a.width(misplaced_modifier) as i32;
            let type_ = self.add_error_at(
                type_,
                offset,
                width,
                ErrorCode::ERR_BadModifierLocation,
                &[DiagArg::TokenText(misplaced_modifier)],
            );

            let a = self.base.arena();
            let modifiers = a.list(modifiers);
            *result = a.incomplete_member(attributes, modifiers, type_);
            return true;
        }

        *result = Green::NULL;
        false
    }

    /// `private bool IsNoneOrIncompleteMember( SyntaxKind parentKind, SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, TypeSyntax type, ExplicitInterfaceSpecifierSyntax explicitInterfaceOpt, SyntaxToken identifierOrThisOpt, TypeParameterListSyntax typeParameterListOpt, out MemberDeclarationSyntax result)` (LanguageParser.cs:3064).
    pub(crate) fn is_none_or_incomplete_member(
        &mut self,
        parent_kind: SyntaxKind,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        type_: Green,
        explicit_interface_opt: Green,
        identifier_or_this_opt: Green,
        type_parameter_list_opt: Green,
        result: &mut Green,
    ) -> bool {
        if explicit_interface_opt.is_null() && identifier_or_this_opt.is_null() && type_parameter_list_opt.is_null() {
            if self.is_none_or_incomplete_member_looks_like_start_of_property_body(modifiers, type_) {
                *result = Green::NULL;
                return false;
            }

            if attributes.is_null()
                && modifiers.is_empty()
                && self.base.arena_ref().is_missing(type_)
                && self.base.arena_ref().kind(type_) != SyntaxKind::RefType
            {
                // we haven't advanced, the caller needs to consume the tokens ahead
                *result = Green::NULL;
                return true;
            }

            let a = self.base.arena();
            let modifiers_list = a.list(modifiers);
            let member_type = if a.is_missing(type_) { Green::NULL } else { type_ };
            let incomplete_member = a.incomplete_member(attributes, modifiers_list, member_type);
            if self.contains_error_diagnostic(incomplete_member) {
                *result = incomplete_member;
            } else if matches!(
                parent_kind,
                SyntaxKind::NamespaceDeclaration | SyntaxKind::FileScopedNamespaceDeclaration
            ) || parent_kind == SyntaxKind::CompilationUnit
            {
                *result = self.add_error_to_last_token(incomplete_member, ErrorCode::ERR_NamespaceUnexpected);
            } else {
                //the error position should indicate CurrentToken
                let current = self.current_token();
                let a = self.base.arena_ref();
                // We're attaching a diagnostic for the current token on the 'incompleteMember' node.  So the
                // offset will be *relative* to the *start* (not *full start*) of 'incompleteMember'. That
                // offset will then be the width of incompleteMember itself, plus any trailing trivia it has,
                // plus the leading trivia of the modifier itself.
                let offset = (a.width(incomplete_member)
                    + a.trailing_trivia_width(incomplete_member)
                    + a.leading_trivia_width(current)) as i32;
                let width = a.width(current) as i32;
                *result = self.add_error_at(
                    incomplete_member,
                    offset,
                    width,
                    ErrorCode::ERR_InvalidMemberDecl,
                    &[DiagArg::TokenText(current)],
                );
            }

            return true;
        }

        *result = Green::NULL;
        false
    }

    /// `looksLikeStartOfPropertyBody` (a local function of `IsNoneOrIncompleteMember`, LanguageParser.cs:3120).
    fn is_none_or_incomplete_member_looks_like_start_of_property_body(
        &mut self,
        modifiers: &SyntaxListBuilder,
        type_: Green,
    ) -> bool {
        // Need to see at least some modifiers, or something that strongly looks like a property type in order
        // for us to at least consider this a property.  Otherwise, we end up in cases where errant `{` and `=>`
        // tokens would lead us to think we're a property in inappropriate places.
        if modifiers.is_empty() && !self.is_none_or_incomplete_member_looks_like_property_type(type_) {
            return false;
        }

        if !self.is_start_of_property_body(0) {
            return false;
        }

        true
    }

    /// `looksLikePropertyType` (a local function of `IsNoneOrIncompleteMember`, LanguageParser.cs:3134).
    fn is_none_or_incomplete_member_looks_like_property_type(&mut self, type_: Green) -> bool {
        let mut property_type = type_;

        // We really want to see at least `Type {` or `Type =>` to think of this as a property. And we want the
        // parser to be confident it got a good return type for the property, not something it was confused
        // about and already reported errors on.  Otherwise, we can too easily end up in cases where an error is
        // next to a `{` and we don't want that to be a property.
        if self.base.arena_ref().is_missing(property_type) || self.contains_error_diagnostic(property_type) {
            return false;
        }

        let a = self.base.arena_ref();
        if a.kind(property_type) == SyntaxKind::RefType {
            property_type = a.slot(property_type, T::RefTypeSyntax::TYPE);
        }

        // Unlikely cases to actually be a property.  Likely some runaway error recovery case.  For example:
        //
        // with { }   (likely a with expression)
        // extension { }  (likely an extension block being written)
        // get { } (likely an accessor, not a property called 'get').  Same for set/add/remove/init.
        //
        // Properties are virtually always going to have a pascal cased identifier name.  So if we see one that
        // is actually a contextual keyword, then it's unlikely to be a property.
        if a.kind(property_type) == SyntaxKind::IdentifierName {
            let identifier = a.slot(property_type, T::IdentifierNameSyntax::IDENTIFIER);
            if facts::is_keyword_kind(a.contextual_kind(identifier)) {
                return false;
            }
        }

        // Can add more cases here if we find ourselves being too eager.

        true
    }

    /// `private bool ReconsideredTypeAsAsyncModifier(ref SyntaxListBuilder modifiers, ref TypeSyntax type, ref ResetPoint afterTypeResetPoint, ref ExplicitInterfaceSpecifierSyntax explicitInterfaceOpt, ref SyntaxToken identifierOrThisOpt, ref TypeParameterListSyntax typeParameterListOpt)` (LanguageParser.cs:3168).
    pub(crate) fn reconsidered_type_as_async_modifier(
        &mut self,
        modifiers: &mut SyntaxListBuilder,
        type_: &mut Green,
        after_type_reset_point: &mut ResetPoint,
        explicit_interface_opt: &mut Green,
        identifier_or_this_opt: &mut Green,
        type_parameter_list_opt: &mut Green,
    ) -> bool {
        if self.base.arena_ref().kind(*type_) != SyntaxKind::RefType
            && !identifier_or_this_opt.is_null()
            && ((!type_parameter_list_opt.is_null()
                && self.base.arena_ref().contains_diagnostics(*type_parameter_list_opt))
                || !matches!(
                    self.current_kind(),
                    SyntaxKind::OpenParenToken | SyntaxKind::OpenBraceToken | SyntaxKind::EqualsGreaterThanToken
                ))
            && self.reconsider_type_as_async_modifier(modifiers, *type_, *identifier_or_this_opt)
        {
            self.reset(after_type_reset_point);
            *explicit_interface_opt = Green::NULL;
            *identifier_or_this_opt = Green::NULL;
            *type_parameter_list_opt = Green::NULL;
            self.release(after_type_reset_point);
            *type_ = self.parse_return_type();
            *after_type_reset_point = self.get_reset_point();
            return true;
        }

        false
    }

    /// `private bool TryParseIndexerOrPropertyDeclaration( SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, TypeSyntax type, ExplicitInterfaceSpecifierSyntax explicitInterfaceOpt, SyntaxToken identifierOrThisOpt, TypeParameterListSyntax typeParameterListOpt, out MemberDeclarationSyntax result)` (LanguageParser.cs:3191).
    pub(crate) fn try_parse_indexer_or_property_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        type_: Green,
        explicit_interface_opt: Green,
        identifier_or_this_opt: Green,
        type_parameter_list_opt: Green,
        result: &mut Green,
    ) -> bool {
        if self.base.arena_ref().kind(identifier_or_this_opt) == SyntaxKind::ThisKeyword {
            *result = self.parse_indexer_declaration(
                attributes,
                modifiers,
                type_,
                explicit_interface_opt,
                identifier_or_this_opt,
                type_parameter_list_opt,
            );
            return true;
        }

        if self.is_start_of_property_body(0) {
            // Error recovery: if we see a property body but no identifier, synthesize a missing identifier
            let mut identifier_or_this_opt = identifier_or_this_opt;
            if identifier_or_this_opt.is_null() {
                let missing = self.create_missing_identifier_token();
                identifier_or_this_opt = self.add_error(missing, ErrorCode::ERR_IdentifierExpected);
            }

            *result = self.parse_property_declaration(
                attributes,
                modifiers,
                type_,
                explicit_interface_opt,
                identifier_or_this_opt,
                type_parameter_list_opt,
            );
            return true;
        }

        *result = Green::NULL;
        false
    }

    /// `private bool IsStartOfPropertyBody(int index)` (LanguageParser.cs:3219).
    pub(crate) fn is_start_of_property_body(&mut self, index: i32) -> bool {
        let mut kind = self.peek_kind(index);

        // If we have `{` or `=>`, definitely a property body.
        if matches!(kind, SyntaxKind::OpenBraceToken | SyntaxKind::EqualsGreaterThanToken) {
            return true;
        }

        // For error recovery, also consider `; {` and `; =>` as starting a property body.
        if kind == SyntaxKind::SemicolonToken {
            kind = self.peek_kind(index + 1);
            if matches!(kind, SyntaxKind::OpenBraceToken | SyntaxKind::EqualsGreaterThanToken) {
                return true;
            }
        }

        false
    }

    /// `internal MemberDeclarationSyntax ParseMemberDeclaration(SyntaxKind parentKind)` (LanguageParser.cs:3239).
    pub(crate) fn parse_member_declaration_with_parent_kind(&mut self, parent_kind: SyntaxKind) -> Green {
        self.recursion_depth += 1;
        self.ensure_sufficient_execution_stack();
        let result = self.parse_member_declaration_core(parent_kind);
        self.recursion_depth -= 1;
        result
    }

    /// `private MemberDeclarationSyntax ParseMemberDeclarationCore(SyntaxKind parentKind)` (LanguageParser.cs:3254).
    pub(crate) fn parse_member_declaration_core(&mut self, parent_kind: SyntaxKind) -> Green {
        // "top-level" expressions and statements should never occur inside an asynchronous context
        debug_assert!(!self.is_in_async);
        debug_assert!(parent_kind != SyntaxKind::CompilationUnit);

        let mut modifiers = self.pool.allocate();

        let save_term_state = self.term_state;

        let result = 'outer: {
            let attributes = self.parse_attribute_declarations(false);

            let mut is_possible_type_declaration = false;
            self.parse_modifiers(&mut modifiers, false, false, &mut is_possible_type_declaration);

            if self.is_extension_container_start() {
                break 'outer self.parse_main_type_declaration(attributes, &mut modifiers);
            }

            // Check for constructor form
            if self.current_kind() == SyntaxKind::IdentifierToken && self.peek_kind(1) == SyntaxKind::OpenParenToken {
                break 'outer self.parse_constructor_declaration(attributes, &mut modifiers);
            }

            // Check for destructor form
            if self.current_kind() == SyntaxKind::TildeToken {
                break 'outer self.parse_destructor_declaration(attributes, &mut modifiers);
            }

            // Check for constant
            if self.current_kind() == SyntaxKind::ConstKeyword {
                break 'outer self.parse_constant_field_declaration(attributes, &mut modifiers, parent_kind);
            }

            // Check for event.
            if self.current_kind() == SyntaxKind::EventKeyword {
                break 'outer self.parse_event_declaration(attributes, &mut modifiers, parent_kind);
            }

            // check for fixed size buffers.
            if self.current_kind() == SyntaxKind::FixedKeyword {
                break 'outer self.parse_fixed_size_buffer_declaration(attributes, &mut modifiers, parent_kind);
            }

            // Check for conversion operators (implicit/explicit)
            let mut result = self.try_parse_conversion_operator_declaration(attributes, &mut modifiers);
            if !result.is_null() {
                break 'outer result;
            }

            // Namespaces should be handled by the caller, not checking for them

            // It's valid to have a type declaration here -- check for those
            if is_possible_type_declaration && self.is_type_declaration_start() {
                break 'outer self.parse_type_declaration(attributes, &mut modifiers);
            }

            // Everything that's left -- methods, fields, properties,
            // indexers, and non-conversion operators -- starts with a type
            // (possibly void).
            let mut type_ = self.parse_return_type();

            let mut after_type_reset_point = self.get_reset_point();

            let inner = 'inner: {
                // Check for misplaced modifiers.  if we see any, then consider this member
                // terminated and restart parsing.
                if self.is_misplaced_modifier(&mut modifiers, attributes, type_, &mut result) {
                    break 'inner result;
                }

                // parse_member_name:
                loop {
                    let mut explicit_interface_opt = Green::NULL;

                    // If we've seen the ref keyword, we know we must have an indexer, method, field, or property.
                    if self.base.arena_ref().kind(type_) != SyntaxKind::RefType {
                        // Check here for operators
                        // Allow old-style implicit/explicit casting operator syntax, just so we can give a better error
                        if self.is_operator_start(&mut explicit_interface_opt, true) {
                            break 'inner self.parse_operator_declaration(
                                attributes,
                                &mut modifiers,
                                type_,
                                explicit_interface_opt,
                            );
                        }
                    }

                    if self.is_field_declaration(false, false) {
                        break 'inner self.parse_normal_field_declaration(
                            attributes,
                            &mut modifiers,
                            type_,
                            parent_kind,
                        );
                    }

                    // At this point we can either have indexers, methods, or
                    // properties (or something unknown).  Try to break apart
                    // the following name and determine what to do from there.
                    let mut identifier_or_this_opt = Green::NULL;
                    let mut type_parameter_list_opt = Green::NULL;
                    self.parse_member_name(
                        &mut explicit_interface_opt,
                        &mut identifier_or_this_opt,
                        &mut type_parameter_list_opt,
                        false,
                    );

                    // First, check if we got absolutely nothing.  If so, then
                    // We need to consume a bad member and try again.
                    if self.is_none_or_incomplete_member(
                        parent_kind,
                        attributes,
                        &mut modifiers,
                        type_,
                        explicit_interface_opt,
                        identifier_or_this_opt,
                        type_parameter_list_opt,
                        &mut result,
                    ) {
                        break 'inner result;
                    }

                    // If the modifiers did not include "async", and the type we got was "async", and there was an
                    // error in the identifier or its type parameters, then the user is probably in the midst of typing
                    // an async method.  In that case we reconsider "async" to be a modifier, and treat the identifier
                    // (with the type parameters) as the type (with type arguments).  Then we go back to looking for
                    // the member name again.
                    // For example, if we get
                    //     async Task<
                    // then we want async to be a modifier and Task<MISSING> to be a type.
                    if self.reconsidered_type_as_async_modifier(
                        &mut modifiers,
                        &mut type_,
                        &mut after_type_reset_point,
                        &mut explicit_interface_opt,
                        &mut identifier_or_this_opt,
                        &mut type_parameter_list_opt,
                    ) {
                        continue;
                    }

                    // identifierOrThisOpt may be null if we're doing error recovery for a property with missing identifier
                    debug_assert!(!identifier_or_this_opt.is_null() || self.is_start_of_property_body(0));

                    if self.try_parse_indexer_or_property_declaration(
                        attributes,
                        &mut modifiers,
                        type_,
                        explicit_interface_opt,
                        identifier_or_this_opt,
                        type_parameter_list_opt,
                        &mut result,
                    ) {
                        break 'inner result;
                    }

                    // treat anything else as a method.
                    break 'inner self.parse_method_declaration(
                        attributes,
                        &mut modifiers,
                        type_,
                        explicit_interface_opt,
                        identifier_or_this_opt,
                        type_parameter_list_opt,
                    );
                }
            };
            self.release(&mut after_type_reset_point);
            inner
        };

        self.pool.free(modifiers);
        self.term_state = save_term_state;
        result
    }

    /// `private bool IsExtensionContainerStart()` (LanguageParser.cs:3412).
    pub(crate) fn is_extension_container_start(&mut self) -> bool {
        // For error recovery, we recognize `extension` followed by `<` even in older language versions
        self.current_contextual_kind() == SyntaxKind::ExtensionKeyword
            && (self.is_feature_enabled(MessageID::IDS_FeatureExtensions)
                || self.peek_kind(1) == SyntaxKind::LessThanToken)
    }

    /// `private static bool ReconsiderTypeAsAsyncModifier( ref SyntaxListBuilder modifiers, TypeSyntax type, SyntaxToken identifierOrThisOpt)` (LanguageParser.cs:3422).
    pub(crate) fn reconsider_type_as_async_modifier(
        &mut self,
        modifiers: &mut SyntaxListBuilder,
        type_: Green,
        identifier_or_this_opt: Green,
    ) -> bool {
        let a = self.base.arena_ref();
        if a.kind(type_) != SyntaxKind::IdentifierName {
            return false;
        }

        if a.kind(identifier_or_this_opt) != SyntaxKind::IdentifierToken {
            return false;
        }

        let identifier = a.slot(type_, T::IdentifierNameSyntax::IDENTIFIER);
        let contextual_kind = a.contextual_kind(identifier);
        if contextual_kind != SyntaxKind::AsyncKeyword || modifiers.iter().any(|&m| a.kind(m) == contextual_kind) {
            return false;
        }

        let keyword = self.convert_to_keyword(identifier);
        modifiers.push(keyword);
        true
    }

    /// `private bool IsFieldDeclaration(bool isEvent, bool isGlobalScriptLevel)` (LanguageParser.cs:3445).
    pub(crate) fn is_field_declaration(&mut self, is_event: bool, is_global_script_level: bool) -> bool {
        if self.current_kind() != SyntaxKind::IdentifierToken {
            return false;
        }

        if self.current_contextual_kind() == SyntaxKind::GlobalKeyword && self.peek_kind(1) == SyntaxKind::UsingKeyword
        {
            return false;
        }

        // Treat this as a field, unless we have anything following that
        // makes us:
        //   a) explicit
        //   b) generic
        //   c) a property
        //   d) a method (unless we already know we're parsing an event)

        // Error recovery, don't allow a misplaced semicolon after the name in a property to throw off the entire parse.
        //
        // e.g. `public int MyProperty; { get; set; }` should still be parsed as a property with a skipped token.
        if !is_global_script_level && self.is_start_of_property_body(1) {
            return false;
        }

        match self.peek_kind(1) {
            SyntaxKind::DotToken             // Goo.     explicit
            | SyntaxKind::ColonColonToken    // Goo::    explicit
            | SyntaxKind::LessThanToken      // Goo<     explicit or generic method
            | SyntaxKind::OpenBraceToken     // Goo {    property
            | SyntaxKind::EqualsGreaterThanToken // Goo =>   property
            => false,
            SyntaxKind::OpenParenToken => is_event, // Goo(     method
            _ => true,
        }
    }

    /// `private bool IsOperatorKeyword()` (LanguageParser.cs:3488).
    pub(crate) fn is_operator_keyword(&mut self) -> bool {
        matches!(
            self.current_kind(),
            SyntaxKind::ImplicitKeyword | SyntaxKind::ExplicitKeyword | SyntaxKind::OperatorKeyword
        )
    }

    /// `public static bool IsComplete(CSharpSyntaxNode node)` (LanguageParser.cs:3493): iterative (the recursion is a
    /// tail call into the last child node).
    pub(crate) fn is_complete(&self, node: Green) -> bool {
        if node.is_null() {
            return false;
        }

        let a = self.base.arena_ref();
        let mut node = node;
        let mut children = Vec::new();
        'node: loop {
            a.child_nodes_and_tokens(node, &mut children);
            for &child in children.iter().rev() {
                if !child.is_token() {
                    node = child;
                    continue 'node;
                }

                if a.is_missing(child) {
                    return false;
                }

                if a.kind(child) != SyntaxKind::None {
                    return true;
                }

                // if token was optional, consider the next one..
            }

            return true;
        }
    }

    /// `private ConstructorDeclarationSyntax ParseConstructorDeclaration( SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers)` (LanguageParser.cs:3525).
    pub(crate) fn parse_constructor_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
    ) -> Green {
        let name = self.parse_identifier_token(ErrorCode::ERR_IdentifierExpected);
        let save_term = self.term_state;
        self.term_state = self.term_state | TerminatorState::IS_END_OF_METHOD_SIGNATURE;
        let param_list = self.parse_parenthesized_parameter_list(false);
        let initializer = self.try_parse_constructor_initializer();

        let mut body = Green::NULL;
        let mut expression_body = Green::NULL;
        let mut semicolon = Green::NULL;
        self.parse_block_and_expression_bodies_with_semicolon(&mut body, &mut expression_body, &mut semicolon, true);

        let a = self.base.arena();
        let modifiers = a.list(modifiers);
        let result = a.constructor_declaration(
            attributes,
            modifiers,
            name,
            param_list,
            initializer,
            body,
            expression_body,
            semicolon,
        );
        self.term_state = save_term;
        result
    }

    /// `private ConstructorInitializerSyntax? TryParseConstructorInitializer()` (LanguageParser.cs:3546).
    pub(crate) fn try_parse_constructor_initializer(&mut self) -> Green {
        let current_token_kind = self.current_kind();
        let should_parse = current_token_kind == SyntaxKind::ColonToken
            || (current_token_kind == SyntaxKind::EqualsGreaterThanToken
                && matches!(self.peek_kind(1), SyntaxKind::ThisKeyword | SyntaxKind::BaseKeyword)
                && self.peek_kind(2) == SyntaxKind::OpenParenToken);

        if !should_parse {
            return Green::NULL;
        }

        self.parse_constructor_initializer()
    }

    /// `private ConstructorInitializerSyntax ParseConstructorInitializer()` (LanguageParser.cs:3560).
    pub(crate) fn parse_constructor_initializer(&mut self) -> Green {
        // Normally called for `:` but also in some error recovery circumstances for `=>`. EatTokenAsKind handles
        // both cases properly, producing the right errors we need in the latter case, and always consuming
        // whichever token we're coming into this method on.
        debug_assert!(matches!(self.current_kind(), SyntaxKind::ColonToken | SyntaxKind::EqualsGreaterThanToken));
        let colon = self.eat_token_as_kind(SyntaxKind::ColonToken);

        let token = if matches!(self.current_kind(), SyntaxKind::BaseKeyword | SyntaxKind::ThisKeyword) {
            self.eat_token()
        } else {
            self.eat_token_code(SyntaxKind::ThisKeyword, ErrorCode::ERR_ThisOrBaseExpected, true)
        };

        let argument_list = if self.current_kind() == SyntaxKind::OpenParenToken {
            self.parse_parenthesized_argument_list()
        } else {
            let report_error = !self.base.arena_ref().contains_diagnostics(token);
            let open = self.eat_token_report(SyntaxKind::OpenParenToken, report_error);
            let close = self.eat_token_report(SyntaxKind::CloseParenToken, report_error);
            self.base.arena().argument_list(open, Green::NULL, close)
        };

        let kind = if self.base.arena_ref().kind(token) == SyntaxKind::BaseKeyword {
            SyntaxKind::BaseConstructorInitializer
        } else {
            SyntaxKind::ThisConstructorInitializer
        };
        self.base.arena().constructor_initializer(kind, colon, token, argument_list)
    }

    /// `private DestructorDeclarationSyntax ParseDestructorDeclaration(SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers)` (LanguageParser.cs:3588).
    pub(crate) fn parse_destructor_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
    ) -> Green {
        debug_assert!(self.current_kind() == SyntaxKind::TildeToken);
        let tilde = self.eat_token_kind(SyntaxKind::TildeToken);

        let name = self.parse_identifier_token(ErrorCode::ERR_IdentifierExpected);
        let open = self.eat_token_kind(SyntaxKind::OpenParenToken);
        let close = self.eat_token_kind(SyntaxKind::CloseParenToken);
        let parameter_list = self.base.arena().parameter_list(open, Green::NULL, close);

        let mut body = Green::NULL;
        let mut expression_body = Green::NULL;
        let mut semicolon = Green::NULL;
        self.parse_block_and_expression_bodies_with_semicolon(&mut body, &mut expression_body, &mut semicolon, true);

        let a = self.base.arena();
        let modifiers = a.list(modifiers);
        a.destructor_declaration(attributes, modifiers, tilde, name, parameter_list, body, expression_body, semicolon)
    }

    /// `private void ParseBlockAndExpressionBodiesWithSemicolon( out BlockSyntax blockBody, out ArrowExpressionClauseSyntax expressionBody, out SyntaxToken semicolon, bool parseSemicolonAfterBlock = true)` (LanguageParser.cs:3609).
    pub(crate) fn parse_block_and_expression_bodies_with_semicolon(
        &mut self,
        block_body: &mut Green,
        expression_body: &mut Green,
        semicolon: &mut Green,
        parse_semicolon_after_block: bool,
    ) {
        // Check for 'forward' declarations with no block of any kind
        if self.current_kind() == SyntaxKind::SemicolonToken {
            *block_body = Green::NULL;
            *expression_body = Green::NULL;
            *semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
            return;
        }

        *block_body = if self.current_kind() == SyntaxKind::OpenBraceToken {
            self.parse_method_or_accessor_body_block(Green::NULL, false)
        } else {
            Green::NULL
        };

        *expression_body = if self.current_kind() == SyntaxKind::EqualsGreaterThanToken {
            self.parse_arrow_expression_clause()
        } else {
            Green::NULL
        };

        // Expression-bodies need semicolons and native behavior
        // expects a semicolon if there is no body
        if !expression_body.is_null() || block_body.is_null() {
            *semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
        }
        // Check for bad semicolon after block body
        else if parse_semicolon_after_block && self.current_kind() == SyntaxKind::SemicolonToken {
            *semicolon = self.eat_token_with_prejudice(ErrorCode::ERR_UnexpectedSemicolon, &[]);
        } else {
            *semicolon = Green::NULL;
        }
    }

    /// `private bool IsEndOfTypeParameterList()` (LanguageParser.cs:3649).
    pub(crate) fn is_end_of_type_parameter_list(&mut self) -> bool {
        if self.current_kind() == SyntaxKind::OpenParenToken {
            // void Goo<T (
            return true;
        }

        if self.current_kind() == SyntaxKind::ColonToken {
            // class C<T :
            return true;
        }

        if self.current_kind() == SyntaxKind::OpenBraceToken {
            // class C<T {
            return true;
        }

        if self.is_current_token_where_of_constraint_clause() {
            // class C<T where T :
            return true;
        }

        false
    }

    /// `private bool IsEndOfMethodSignature()` (LanguageParser.cs:3678).
    pub(crate) fn is_end_of_method_signature(&mut self) -> bool {
        matches!(self.current_kind(), SyntaxKind::SemicolonToken | SyntaxKind::OpenBraceToken)
    }

    /// `private bool IsEndOfTypeSignature()` (LanguageParser.cs:3681).
    pub(crate) fn is_end_of_type_signature(&mut self) -> bool {
        matches!(self.current_kind(), SyntaxKind::SemicolonToken | SyntaxKind::OpenBraceToken)
    }

    /// `private bool IsEndOfNameInExplicitInterface()` (LanguageParser.cs:3686).
    pub(crate) fn is_end_of_name_in_explicit_interface(&mut self) -> bool {
        matches!(self.current_kind(), SyntaxKind::DotToken | SyntaxKind::ColonColonToken)
    }

    /// `private bool IsEndOfFunctionPointerParameterList(bool errored)` (LanguageParser.cs:3689).
    pub(crate) fn is_end_of_function_pointer_parameter_list(&mut self, errored: bool) -> bool {
        self.current_kind() == if errored { SyntaxKind::CloseParenToken } else { SyntaxKind::GreaterThanToken }
    }

    /// `private bool IsEndOfFunctionPointerCallingConvention()` (LanguageParser.cs:3692).
    pub(crate) fn is_end_of_function_pointer_calling_convention(&mut self) -> bool {
        self.current_kind() == SyntaxKind::CloseBracketToken
    }

    /// `private MethodDeclarationSyntax ParseMethodDeclaration( SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, TypeSyntax type, ExplicitInterfaceSpecifierSyntax explicitInterfaceOpt, SyntaxToken identifier, TypeParameterListSyntax typeParameterList)` (LanguageParser.cs:3695).
    pub(crate) fn parse_method_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        type_: Green,
        explicit_interface_opt: Green,
        identifier: Green,
        type_parameter_list: Green,
    ) -> Green {
        // Parse the name (it could be qualified)
        let save_term = self.term_state;
        self.term_state = self.term_state | TerminatorState::IS_END_OF_METHOD_SIGNATURE;

        let mut param_list = self.parse_parenthesized_parameter_list(false);

        let mut constraints = self.pool.allocate();
        if self.current_contextual_kind() == SyntaxKind::WhereKeyword {
            self.parse_type_parameter_constraint_clauses(&mut constraints);
        } else if self.current_kind() == SyntaxKind::ColonToken {
            // Use else if, rather than if, because if we see both a constructor initializer and a constraint clause, we're too lost to recover.
            let colon_token = self.current_token();

            let initializer = self.parse_constructor_initializer();
            let initializer = self.add_error_to_first_token(
                initializer,
                ErrorCode::ERR_UnexpectedToken,
                &[DiagArg::TokenText(colon_token)],
            );
            param_list = self.add_trailing_skipped_syntax(param_list, initializer);

            // CONSIDER: Parsing an invalid constructor initializer could, conceivably, get us way
            // off track.  If this becomes a problem, an alternative approach would be to generalize
            // EatTokenWithPrejudice in such a way that we can just skip everything until we recognize
            // our context again (perhaps an open brace).
        }

        self.term_state = save_term;

        // Method declarations cannot be nested or placed inside async lambdas, and so cannot occur in an
        // asynchronous context. Therefore the IsInAsync state of the parent scope is not saved and
        // restored, just assumed to be false and reset accordingly after parsing the method body.
        debug_assert!(!self.is_in_async);

        let is_async = modifiers.iter().any(|&m| self.base.arena_ref().kind(m) == SyntaxKind::AsyncKeyword);
        let resetter = ParserSyntaxContextResetter::new(self, Some(is_async), None, None, None);
        let mut block_body = Green::NULL;
        let mut expression_body = Green::NULL;
        let mut semicolon = Green::NULL;
        self.parse_block_and_expression_bodies_with_semicolon(
            &mut block_body,
            &mut expression_body,
            &mut semicolon,
            true,
        );

        let a = self.base.arena();
        let modifiers = a.list(modifiers);
        let constraints_list = self.pool.to_list_and_free(self.base.lexer.arena(), constraints);
        let result = self.base.arena().method_declaration(
            attributes,
            modifiers,
            type_,
            explicit_interface_opt,
            identifier,
            type_parameter_list,
            param_list,
            constraints_list,
            block_body,
            expression_body,
            semicolon,
        );
        resetter.dispose(self);
        result
    }

    /// `private TypeSyntax ParseReturnType()` (LanguageParser.cs:3754).
    pub(crate) fn parse_return_type(&mut self) -> Green {
        let save_term = self.term_state;
        self.term_state = self.term_state | TerminatorState::IS_END_OF_RETURN_TYPE;
        let type_ = self.parse_type_or_void();
        self.term_state = save_term;
        type_
    }

    /// `private bool IsEndOfReturnType()` (LanguageParser.cs:3763).
    pub(crate) fn is_end_of_return_type(&mut self) -> bool {
        matches!(
            self.current_kind(),
            SyntaxKind::OpenParenToken | SyntaxKind::OpenBraceToken | SyntaxKind::SemicolonToken
        )
    }

    /// `private ConversionOperatorDeclarationSyntax TryParseConversionOperatorDeclaration(SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers)` (LanguageParser.cs:3776).
    pub(crate) fn try_parse_conversion_operator_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
    ) -> Green {
        let mut point = self.get_reset_point();

        let result = 'try_: {
            let mut have_explicit_interface_name = false;

            if !matches!(self.current_kind(), SyntaxKind::ImplicitKeyword | SyntaxKind::ExplicitKeyword) {
                let mut separator_kind = SyntaxKind::None;

                if self.current_kind() == SyntaxKind::IdentifierToken {
                    // Scan possible ExplicitInterfaceSpecifier

                    loop {
                        // now, scan past the next name.  if it's followed by a dot then
                        // it's part of the explicit name we're building up.  Otherwise,
                        // it should be an operator token

                        if self.current_kind() == SyntaxKind::OperatorKeyword {
                            // We're past any explicit interface portion
                            break;
                        } else {
                            let mut scan_name_part_point = self.get_disposable_reset_point(false);

                            let mut last_token_position = -1;
                            self.is_making_progress(&mut last_token_position, true);
                            self.scan_named_type_part();

                            if self.is_dot_or_colon_colon()
                                || (self.is_making_progress(&mut last_token_position, false)
                                    && self.current_kind() != SyntaxKind::OpenParenToken)
                            {
                                have_explicit_interface_name = true;

                                if self.is_dot_or_colon_colon() {
                                    separator_kind = self.current_kind();
                                    self.eat_token();
                                } else {
                                    separator_kind = SyntaxKind::None;
                                }
                                self.dispose_reset_point(scan_name_part_point);
                            } else {
                                self.reset_disposable(&mut scan_name_part_point);
                                self.dispose_reset_point(scan_name_part_point);

                                // We're past any explicit interface portion
                                break;
                            }
                        }
                    }
                }

                #[allow(clippy::needless_late_init, reason = "the C# declares it, then assigns it per case")]
                let possible_conversion;

                if self.current_kind() != SyntaxKind::OperatorKeyword
                    || (have_explicit_interface_name && separator_kind != SyntaxKind::DotToken)
                {
                    possible_conversion = false;
                } else if matches!(self.peek_kind(1), SyntaxKind::CheckedKeyword | SyntaxKind::UncheckedKeyword) {
                    possible_conversion = !facts::is_any_overloadable_operator(self.peek_kind(2));
                } else {
                    possible_conversion = !facts::is_any_overloadable_operator(self.peek_kind(1));
                }

                self.reset(&mut point);

                if !possible_conversion {
                    break 'try_ Green::NULL;
                }
            }

            let mut style = if matches!(self.current_kind(), SyntaxKind::ImplicitKeyword | SyntaxKind::ExplicitKeyword)
            {
                self.eat_token()
            } else {
                self.eat_token_kind(SyntaxKind::ExplicitKeyword)
            };

            let mut explicit_interface_opt =
                self.try_parse_conversion_operator_declaration_try_parse_explicit_interface_specifier();
            debug_assert!(
                !self.base.arena_ref().is_missing(style)
                    || have_explicit_interface_name == !explicit_interface_opt.is_null()
            );

            let mut op_keyword;
            let mut type_;

            if !self.base.arena_ref().is_missing(style)
                && !explicit_interface_opt.is_null()
                && self.current_kind() != SyntaxKind::OperatorKeyword
                && self.trailing_trivia_has_end_of_line(style)
            {
                // Not likely an explicit interface implementation. Likely a beginning of the next member on the next line.
                self.reset(&mut point);
                style = self.eat_token();
                explicit_interface_opt = Green::NULL;
                op_keyword = self.eat_token_kind(SyntaxKind::OperatorKeyword);
                let missing = self.create_missing_identifier_name();
                type_ = self.add_error(missing, ErrorCode::ERR_IdentifierExpected);

                let a = self.base.arena();
                let modifiers = a.list(modifiers);
                let open = a.missing_token(SyntaxKind::OpenParenToken);
                let close = a.missing_token(SyntaxKind::CloseParenToken);
                let parameter_list = a.parameter_list(open, Green::NULL, close);
                let semicolon = a.missing_token(SyntaxKind::SemicolonToken);
                break 'try_ a.conversion_operator_declaration(
                    attributes,
                    modifiers,
                    style,
                    explicit_interface_opt,
                    op_keyword,
                    Green::NULL,
                    type_,
                    parameter_list,
                    Green::NULL,
                    Green::NULL,
                    semicolon,
                );
            }

            op_keyword = self.eat_token_kind(SyntaxKind::OperatorKeyword);
            let checked_keyword = self.try_eat_checked_or_handle_unchecked(&mut op_keyword);

            self.release(&mut point);
            point = self.get_reset_point();

            let could_be_parameter_list = self.current_kind() == SyntaxKind::OpenParenToken;
            type_ = self.parse_type(ParseTypeMode::Normal);

            if could_be_parameter_list && self.current_kind() != SyntaxKind::OpenParenToken && {
                let a = self.base.arena_ref();
                a.kind(type_) == SyntaxKind::TupleType && {
                    let elements = a.slot(type_, T::TupleTypeSyntax::ELEMENTS);
                    elements.is_list()
                        && a.slot_count(elements) == 3
                        && a.is_missing(a.slot(elements, 1))
                        && a.is_missing(a.slot(elements, 2))
                }
            } {
                // It looks like the type is missing and we parsed parameter list as the type. Recover.
                self.reset(&mut point);
                type_ = self.parse_identifier_name(ErrorCode::ERR_IdentifierExpected);
            }

            let param_list = self.parse_parenthesized_parameter_list(false);

            let mut block_body = Green::NULL;
            let mut expression_body = Green::NULL;
            let mut semicolon = Green::NULL;
            self.parse_block_and_expression_bodies_with_semicolon(
                &mut block_body,
                &mut expression_body,
                &mut semicolon,
                true,
            );

            let a = self.base.arena();
            let modifiers = a.list(modifiers);
            a.conversion_operator_declaration(
                attributes,
                modifiers,
                style,
                explicit_interface_opt,
                op_keyword,
                checked_keyword,
                type_,
                param_list,
                block_body,
                expression_body,
                semicolon,
            )
        };
        self.release(&mut point);
        result
    }

    /// `tryParseExplicitInterfaceSpecifier` (a local function of `TryParseConversionOperatorDeclaration`,
    /// LanguageParser.cs:3938).
    fn try_parse_conversion_operator_declaration_try_parse_explicit_interface_specifier(&mut self) -> Green {
        if self.current_kind() != SyntaxKind::IdentifierToken {
            return Green::NULL;
        }

        let mut explicit_interface_name = Green::NULL;
        let mut separator = Green::NULL;

        loop {
            // now, scan past the next name.  if it's followed by a dot then
            // it's part of the explicit name we're building up.  Otherwise,
            // it should be an operator token

            let is_part_of_interface_name;
            {
                let reset_point = self.get_disposable_reset_point(true);
                if self.current_kind() == SyntaxKind::OperatorKeyword {
                    is_part_of_interface_name = false;
                } else {
                    let mut last_token_position = -1;
                    self.is_making_progress(&mut last_token_position, true);
                    self.scan_named_type_part();
                    is_part_of_interface_name = self.is_dot_or_colon_colon()
                        || (self.is_making_progress(&mut last_token_position, false)
                            && self.current_kind() != SyntaxKind::OpenParenToken);
                }
                self.dispose_reset_point(reset_point);
            }

            if !is_part_of_interface_name {
                // We're past any explicit interface portion
                if self.base.arena_ref().kind(separator) == SyntaxKind::ColonColonToken {
                    separator = self.add_error(separator, ErrorCode::ERR_AliasQualAsExpression);
                    separator = self.convert_to_missing_with_trailing_trivia(separator, SyntaxKind::DotToken);
                }

                break;
            } else {
                // If we saw a . or :: then we must have something explicit.
                self.accumulate_explicit_interface_name(&mut explicit_interface_name, &mut separator);
            }
        }

        if explicit_interface_name.is_null() {
            return Green::NULL;
        }

        let separator_kind = self.base.arena_ref().kind(separator);
        if separator_kind != SyntaxKind::DotToken {
            let a = self.base.arena_ref();
            let (offset, width) = (a.leading_trivia_width(separator) as i32, a.width(separator) as i32);
            let error = self.get_expected_token_error(SyntaxKind::DotToken, separator_kind, offset, width);
            separator = self.with_additional_diagnostics(separator, &[error]);
            separator = self.convert_to_missing_with_trailing_trivia(separator, SyntaxKind::DotToken);
        }

        self.base.arena().explicit_interface_specifier(explicit_interface_name, separator)
    }

    /// `private SyntaxToken TryEatCheckedOrHandleUnchecked(ref SyntaxToken operatorKeyword)` (LanguageParser.cs:4004).
    pub(crate) fn try_eat_checked_or_handle_unchecked(&mut self, operator_keyword: &mut Green) -> Green {
        if self.current_kind() == SyntaxKind::UncheckedKeyword {
            // if we encounter `operator unchecked`, we place the `unchecked` as skipped trivia on `operator`
            let token = self.eat_token();
            let misplaced_token = self.add_error(token, ErrorCode::ERR_MisplacedUnchecked);
            *operator_keyword = self.add_trailing_skipped_syntax(*operator_keyword, misplaced_token);
            return Green::NULL;
        }

        self.try_eat_token(SyntaxKind::CheckedKeyword)
    }

    /// `private MemberDeclarationSyntax ParseOperatorDeclaration( SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, TypeSyntax type, ExplicitInterfaceSpecifierSyntax explicitInterfaceOpt)` (LanguageParser.cs:4017).
    pub(crate) fn parse_operator_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        type_: Green,
        explicit_interface_opt: Green,
    ) -> Green {
        let mut type_ = type_;
        // We can get here after seeing `explicit` or `implicit` or `operator`.  `ret-type explicit op ...` is not
        // legal though.
        let first_token = self.current_token();
        if matches!(self.base.arena_ref().kind(first_token), SyntaxKind::ExplicitKeyword | SyntaxKind::ImplicitKeyword)
            && self.peek_kind(1) == SyntaxKind::OperatorKeyword
        {
            let conversion_operator = self.try_parse_conversion_operator_declaration(attributes, modifiers);
            if !conversion_operator.is_null() {
                // We need to ensure the type syntax the user provided gets an appropriate error and is placed as
                // leading skipped trivia for the explicit/implicit keyword.
                let keyword_index = T::ConversionOperatorDeclarationSyntax::IMPLICIT_OR_EXPLICIT_KEYWORD;
                let keyword = self.base.arena_ref().slot(conversion_operator, keyword_index);
                let skipped =
                    self.add_error_args(type_, ErrorCode::ERR_BadOperatorSyntax, &[DiagArg::TokenText(first_token)]);
                let new_implicit_or_explicit_keyword = self.add_leading_skipped_syntax(keyword, skipped);
                let mut slots = self.base.arena_ref().slots(conversion_operator).to_vec();
                slots[keyword_index] = new_implicit_or_explicit_keyword;
                return self.base.arena().update_node(conversion_operator, &slots);
            }
        }

        let mut op_keyword = self.eat_token_kind(SyntaxKind::OperatorKeyword);
        let checked_keyword = self.try_eat_checked_or_handle_unchecked(&mut op_keyword);
        let mut op_token;
        let op_token_error_offset;
        let mut op_token_error_width;

        if facts::is_any_overloadable_operator(self.current_kind()) {
            op_token = self.eat_token();
            debug_assert!(!self.base.arena_ref().is_missing(op_token));
            op_token_error_offset = self.base.arena_ref().leading_trivia_width(op_token) as i32;
            op_token_error_width = self.base.arena_ref().width(op_token) as i32;
        } else if matches!(self.current_kind(), SyntaxKind::ImplicitKeyword | SyntaxKind::ExplicitKeyword) {
            // Grab the offset and width before we consume the invalid keyword and change our position.
            let current = self.current_token();
            (op_token_error_offset, op_token_error_width) = (0, self.base.arena_ref().width(current) as i32);
            let token = self.eat_token();
            op_token = self.convert_to_missing_with_trailing_trivia(token, SyntaxKind::PlusToken);
            debug_assert!(self.base.arena_ref().is_missing(op_token)); // ConvertToMissingWithTrailingTrivia should have converted to a missing token.

            debug_assert!(!type_.is_null()); // How could it be?  The only caller got it from ParseReturnType.

            if self.base.arena_ref().is_missing(type_) {
                let diag_info = self.make_error(
                    op_token_error_offset,
                    op_token_error_width,
                    ErrorCode::ERR_BadOperatorSyntax,
                    &[DiagArg::Str(facts::get_text(SyntaxKind::PlusToken))],
                );
                op_token = self.with_additional_diagnostics(op_token, &[diag_info]);
            } else {
                // Dev10 puts this error on the type (if there is one).
                type_ = self.add_error_args(
                    type_,
                    ErrorCode::ERR_BadOperatorSyntax,
                    &[DiagArg::Str(facts::get_text(SyntaxKind::PlusToken))],
                );
            }
        } else {
            // Consume whatever follows the operator keyword as the operator token.  If it is not we'll add an
            // error below (when we can guess the arity). Handle .. as well so we can give the user a good
            // message if they do `operator ..`
            op_token = if self.is_at_dot_dot_token() { self.eat_dot_dot_token() } else { self.eat_token() };
            debug_assert!(!self.base.arena_ref().is_missing(op_token));
            op_token_error_offset = self.base.arena_ref().leading_trivia_width(op_token) as i32;
            op_token_error_width = self.base.arena_ref().width(op_token) as i32;
        }

        // check for >> and >>>
        if self.base.arena_ref().kind(op_token) == SyntaxKind::GreaterThanToken {
            let mut tk = self.current_token();

            if self.base.arena_ref().kind(tk) == SyntaxKind::GreaterThanToken {
                if self.no_trivia_between(op_token, tk) {
                    // no trailing trivia and no leading trivia
                    let mut op_token2 = self.eat_token();
                    tk = self.current_token();

                    let tk_kind = self.base.arena_ref().kind(tk);
                    let merged = if tk_kind == SyntaxKind::GreaterThanToken && self.no_trivia_between(op_token2, tk) {
                        // no trailing trivia and no leading trivia
                        op_token2 = self.eat_token();
                        SyntaxKind::GreaterThanGreaterThanGreaterThanToken
                    } else if tk_kind == SyntaxKind::GreaterThanEqualsToken && self.no_trivia_between(op_token2, tk) {
                        // no trailing trivia and no leading trivia
                        op_token2 = self.eat_token();
                        SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken
                    } else {
                        SyntaxKind::GreaterThanGreaterThanToken
                    };
                    let a = self.base.arena();
                    let leading = a.leading_trivia(op_token);
                    let trailing = a.trailing_trivia(op_token2);
                    op_token = a.token_with_trivia(leading, merged, trailing);
                    op_token_error_width = a.width(op_token) as i32;
                }
            } else if self.base.arena_ref().kind(tk) == SyntaxKind::GreaterThanEqualsToken
                && self.no_trivia_between(op_token, tk)
            {
                // no trailing trivia and no leading trivia
                let op_token2 = self.eat_token();
                let a = self.base.arena();
                let leading = a.leading_trivia(op_token);
                let trailing = a.trailing_trivia(op_token2);
                op_token = a.token_with_trivia(leading, SyntaxKind::GreaterThanGreaterThanEqualsToken, trailing);
                op_token_error_width = a.width(op_token) as i32;
            }
        }

        let op_kind = self.base.arena_ref().kind(op_token);
        let param_list = self.parse_parenthesized_parameter_list(false);

        let parameters = self.base.arena_ref().slot(param_list, T::ParameterListSyntax::PARAMETERS);
        let parameter_count = separated_count(self.base.arena_ref(), parameters);
        match parameter_count {
            1 => {
                if self.base.arena_ref().is_missing(op_token)
                    || !(facts::is_overloadable_unary_operator(op_kind)
                        || facts::is_overloadable_compound_assignment_operator(op_kind))
                {
                    let diag_info = self.make_error(
                        op_token_error_offset,
                        op_token_error_width,
                        ErrorCode::ERR_OvlUnaryOperatorExpected,
                        &[],
                    );
                    op_token = self.with_additional_diagnostics(op_token, &[diag_info]);
                }
            }
            2 => {
                if self.base.arena_ref().is_missing(op_token) || !facts::is_overloadable_binary_operator(op_kind) {
                    let diag_info = self.make_error(
                        op_token_error_offset,
                        op_token_error_width,
                        ErrorCode::ERR_OvlBinaryOperatorExpected,
                        &[],
                    );
                    op_token = self.with_additional_diagnostics(op_token, &[diag_info]);
                }
            }
            _ => {
                if self.base.arena_ref().is_missing(op_token) {
                    let diag_info = self.make_error(
                        op_token_error_offset,
                        op_token_error_width,
                        ErrorCode::ERR_OvlOperatorExpected,
                        &[],
                    );
                    op_token = self.with_additional_diagnostics(op_token, &[diag_info]);
                } else if facts::is_overloadable_binary_operator(op_kind) {
                    op_token = self.add_error_args(
                        op_token,
                        ErrorCode::ERR_BadBinOpArgs,
                        &[DiagArg::Str(facts::get_text(op_kind))],
                    );
                } else if facts::is_overloadable_unary_operator(op_kind) {
                    if !matches!(op_kind, SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken)
                        || parameter_count != 0
                    {
                        op_token = self.add_error_args(
                            op_token,
                            ErrorCode::ERR_BadUnOpArgs,
                            &[DiagArg::Str(facts::get_text(op_kind))],
                        );
                    }
                } else if facts::is_overloadable_compound_assignment_operator(op_kind) {
                    op_token = self.add_error_args(
                        op_token,
                        ErrorCode::ERR_BadCompoundAssignmentOpArgs,
                        &[DiagArg::Str(facts::get_text(op_kind))],
                    );
                } else {
                    op_token = self.add_error(op_token, ErrorCode::ERR_OvlOperatorExpected);
                }
            }
        }

        let mut block_body = Green::NULL;
        let mut expression_body = Green::NULL;
        let mut semicolon = Green::NULL;
        self.parse_block_and_expression_bodies_with_semicolon(
            &mut block_body,
            &mut expression_body,
            &mut semicolon,
            true,
        );

        // if the operator is invalid, then switch it to plus (which will work either way) so that
        // we can finish building the tree
        if !(op_kind == SyntaxKind::IsKeyword
            || facts::is_overloadable_unary_operator(op_kind)
            || facts::is_overloadable_binary_operator(op_kind)
            || facts::is_overloadable_compound_assignment_operator(op_kind))
        {
            op_token = self.convert_to_missing_with_trailing_trivia(op_token, SyntaxKind::PlusToken);
        }

        let a = self.base.arena();
        let modifiers = a.list(modifiers);
        a.operator_declaration(
            attributes,
            modifiers,
            type_,
            explicit_interface_opt,
            op_keyword,
            checked_keyword,
            op_token,
            param_list,
            block_body,
            expression_body,
            semicolon,
        )
    }

    /// `private IndexerDeclarationSyntax ParseIndexerDeclaration( SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, TypeSyntax type, ExplicitInterfaceSpecifierSyntax explicitInterfaceOpt, SyntaxToken thisKeyword, TypeParameterListSyntax typeParameterList)` (LanguageParser.cs:4217).
    pub(crate) fn parse_indexer_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        type_: Green,
        explicit_interface_opt: Green,
        this_keyword: Green,
        type_parameter_list: Green,
    ) -> Green {
        debug_assert!(self.base.arena_ref().kind(this_keyword) == SyntaxKind::ThisKeyword);
        let mut this_keyword = this_keyword;

        // check to see if the user tried to create a generic indexer.
        if !type_parameter_list.is_null() {
            this_keyword = self.add_trailing_skipped_syntax(this_keyword, type_parameter_list);
            this_keyword = self.add_error(this_keyword, ErrorCode::ERR_UnexpectedGenericName);
        }

        let parameter_list = self.parse_bracketed_parameter_list();

        let mut accessor_list = Green::NULL;
        let mut expression_body = Green::NULL;
        let mut semicolon = Green::NULL;
        // Try to parse accessor list unless there is an expression
        // body and no accessor list
        if self.current_kind() == SyntaxKind::EqualsGreaterThanToken {
            expression_body = self.parse_arrow_expression_clause();
            semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
        } else {
            accessor_list = self.parse_accessor_list(AccessorDeclaringKind::Indexer);
            if self.current_kind() == SyntaxKind::SemicolonToken {
                semicolon = self.eat_token_with_prejudice(ErrorCode::ERR_UnexpectedSemicolon, &[]);
            }
        }

        // If the user has erroneously provided both an accessor list
        // and an expression body, but no semicolon, we want to parse
        // the expression body and report the error (which is done later)
        if self.current_kind() == SyntaxKind::EqualsGreaterThanToken && semicolon.is_null() {
            expression_body = self.parse_arrow_expression_clause();
            semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
        }

        let a = self.base.arena();
        let modifiers = a.list(modifiers);
        a.indexer_declaration(
            attributes,
            modifiers,
            type_,
            explicit_interface_opt,
            this_keyword,
            parameter_list,
            accessor_list,
            expression_body,
            semicolon,
        )
    }

    /// `private PropertyDeclarationSyntax ParsePropertyDeclaration( SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, TypeSyntax type, ExplicitInterfaceSpecifierSyntax explicitInterfaceOpt, SyntaxToken identifier, TypeParameterListSyntax typeParameterList)` (LanguageParser.cs:4277).
    pub(crate) fn parse_property_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        type_: Green,
        explicit_interface_opt: Green,
        identifier: Green,
        type_parameter_list: Green,
    ) -> Green {
        let mut identifier = identifier;
        // check to see if the user tried to create a generic property.
        if !type_parameter_list.is_null() {
            identifier = self.add_trailing_skipped_syntax(identifier, type_parameter_list);
            identifier = self.add_error(identifier, ErrorCode::ERR_UnexpectedGenericName);
        }

        // Error recovery: add an errant semicolon to the identifier token and keep going.
        if self.current_kind() == SyntaxKind::SemicolonToken {
            let token = self.eat_token_even_with_incorrect_kind(SyntaxKind::OpenBraceToken);
            identifier = self.add_trailing_skipped_syntax(identifier, token);
        }

        // We know we are parsing a property because we have seen either an open brace or an arrow token
        debug_assert!(self.is_start_of_property_body(0));

        let accessor_list = if self.current_kind() == SyntaxKind::OpenBraceToken {
            self.parse_accessor_list(AccessorDeclaringKind::Property)
        } else {
            Green::NULL
        };

        let mut expression_body = Green::NULL;
        let mut initializer = Green::NULL;

        // Check for expression body
        if self.current_kind() == SyntaxKind::EqualsGreaterThanToken {
            let resetter = ParserSyntaxContextResetter::new(self, None, None, Some(true), None);
            expression_body = self.parse_arrow_expression_clause();
            resetter.dispose(self);
        }
        // Check if we have an initializer
        else if self.current_kind() == SyntaxKind::EqualsToken {
            let equals = self.eat_token_kind(SyntaxKind::EqualsToken);
            let value = self.parse_variable_initializer();
            initializer = self.base.arena().equals_value_clause(equals, value);
        }

        let mut semicolon = Green::NULL;
        if !expression_body.is_null() || !initializer.is_null() {
            semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
        } else if self.current_kind() == SyntaxKind::SemicolonToken {
            semicolon = self.eat_token_with_prejudice(ErrorCode::ERR_UnexpectedSemicolon, &[]);
        }

        let a = self.base.arena();
        let modifiers = a.list(modifiers);
        a.property_declaration(
            attributes,
            modifiers,
            type_,
            explicit_interface_opt,
            identifier,
            accessor_list,
            expression_body,
            initializer,
            semicolon,
        )
    }

    // `ParserSyntaxContextResetter` (LanguageParser.cs:4344) is the struct of that name below.
}

/// `AccessorDeclaringKind` (LanguageParser.cs:4393).
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub(crate) enum AccessorDeclaringKind {
    #[default]
    Property,
    Indexer,
    Event,
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private AccessorListSyntax ParseAccessorList(AccessorDeclaringKind declaringKind)` (LanguageParser.cs:4400).
    pub(crate) fn parse_accessor_list(&mut self, declaring_kind: AccessorDeclaringKind) -> Green {
        let mut open_brace = self.eat_token_kind(SyntaxKind::OpenBraceToken);
        let mut accessors = Green::NULL;

        if !self.base.arena_ref().is_missing(open_brace) || !self.is_terminator() {
            // parse property accessors
            let mut builder = self.pool.allocate();

            loop {
                if self.current_kind() == SyntaxKind::CloseBraceToken {
                    break;
                } else if self.is_possible_accessor() {
                    let acc = self.parse_accessor_declaration(declaring_kind);
                    builder.push(acc);
                } else if self.skip_bad_accessor_list_tokens(
                    &mut open_brace,
                    &mut builder,
                    if declaring_kind == AccessorDeclaringKind::Event {
                        ErrorCode::ERR_AddOrRemoveExpected
                    } else {
                        ErrorCode::ERR_GetOrSetExpected
                    },
                ) == PostSkipAction::Abort
                {
                    break;
                }
            }

            accessors = self.pool.to_list_and_free(self.base.lexer.arena(), builder);
        }

        let close_brace = self.eat_token_kind(SyntaxKind::CloseBraceToken);
        self.base.arena().accessor_list(open_brace, accessors, close_brace)
    }

    /// `private ArrowExpressionClauseSyntax ParseArrowExpressionClause()` (LanguageParser.cs:4437).
    pub(crate) fn parse_arrow_expression_clause(&mut self) -> Green {
        let arrow = self.eat_token_kind(SyntaxKind::EqualsGreaterThanToken);
        let expression = self.parse_possible_ref_expression();
        self.base.arena().arrow_expression_clause(arrow, expression)
    }

    /// `private ExpressionSyntax ParsePossibleRefExpression()` (LanguageParser.cs:4444).
    pub(crate) fn parse_possible_ref_expression(&mut self) -> Green {
        // check for lambda expression with explicit ref return type: `ref int () => { ... }`
        let ref_keyword = if self.current_kind() == SyntaxKind::RefKeyword
            && !self.is_possible_lambda_expression(Precedence::EXPRESSION)
        {
            self.eat_token()
        } else {
            Green::NULL
        };

        let expression = self.parse_expression_core();
        if ref_keyword.is_null() { expression } else { self.base.arena().ref_expression(ref_keyword, expression) }
    }

    /// `private PostSkipAction SkipBadAccessorListTokens(ref SyntaxToken openBrace, SyntaxListBuilder<AccessorDeclarationSyntax> list, ErrorCode error)` (LanguageParser.cs:4455).
    pub(crate) fn skip_bad_accessor_list_tokens(
        &mut self,
        open_brace: &mut Green,
        list: &mut SyntaxListBuilder,
        error: ErrorCode,
    ) -> PostSkipAction {
        self.skip_bad_list_tokens_with_error_code(
            open_brace,
            list,
            |p| p.current_kind() != SyntaxKind::CloseBraceToken && !p.is_possible_accessor(),
            |p| p.is_terminator(),
            error,
        )
    }

    /// `private bool IsPossibleAccessor()` (LanguageParser.cs:4463).
    pub(crate) fn is_possible_accessor(&mut self) -> bool {
        self.current_kind() == SyntaxKind::IdentifierToken
            || self.is_possible_attribute_declaration()
            || facts::get_accessor_declaration_kind(self.current_contextual_kind()) != SyntaxKind::None
            || self.current_kind() == SyntaxKind::OpenBraceToken // for accessor blocks w/ missing keyword
            || self.current_kind() == SyntaxKind::SemicolonToken // for empty body accessors w/ missing keyword
            || self.is_possible_accessor_modifier()
    }

    /// `private bool IsPossibleAccessorModifier()` (LanguageParser.cs:4473).
    pub(crate) fn is_possible_accessor_modifier(&mut self) -> bool {
        // We only want to accept a modifier as the start of an accessor if the modifiers are
        // actually followed by "get/set/add/remove".  Otherwise, we might thing think we're
        // starting an accessor when we're actually starting a normal class member.  For example:
        //
        //      class C {
        //          public int Prop { get { this.
        //          private DateTime x;
        //
        // We don't want to think of the "private" in "private DateTime x" as starting an accessor
        // here.  If we do, we'll get totally thrown off in parsing the remainder and that will
        // throw off the rest of the features that depend on a good syntax tree.
        //
        // Note: we allow all modifiers here.  That's because we want to parse things like
        // "abstract get" as an accessor.  This way we can provide a good error message
        // to the user that this is not allowed.

        let current = self.current_token();
        if self.get_modifier_excluding_scoped(current) == DeclarationModifiers::NONE {
            return false;
        }

        let mut peek_index = 1;
        loop {
            let token = self.peek_token(peek_index);
            if self.get_modifier_excluding_scoped(token) == DeclarationModifiers::NONE {
                break;
            }
            peek_index += 1;
        }

        let token = self.peek_token(peek_index);
        let a = self.base.arena_ref();
        if matches!(a.kind(token), SyntaxKind::CloseBraceToken | SyntaxKind::EndOfFileToken) {
            // If we see "{ get { } public }
            // then we will think that "public" likely starts an accessor.
            return true;
        }

        matches!(
            a.contextual_kind(token),
            SyntaxKind::GetKeyword
                | SyntaxKind::SetKeyword
                | SyntaxKind::InitKeyword
                | SyntaxKind::AddKeyword
                | SyntaxKind::RemoveKeyword
        )
    }
}

/// `PostSkipAction` (LanguageParser.cs:4523).
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Default)]
pub(crate) enum PostSkipAction {
    #[default]
    Continue,
    Abort,
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private PostSkipAction SkipBadSeparatedListTokensWithExpectedKind<T, TNode>( ref T startToken, SeparatedSyntaxListBuilder<TNode> list, Func<LanguageParser, bool> isNotExpectedFunction, Func<LanguageParser, SyntaxKind, bool> abortFunction, SyntaxKind expected, SyntaxKind closeKind = SyntaxKind.None) where T : CSharpSyntaxNode where TNode : CSharpSyntaxNode` (LanguageParser.cs:4529).
    pub(crate) fn skip_bad_separated_list_tokens_with_expected_kind(
        &mut self,
        start_token: &mut Green,
        list: &mut SyntaxListBuilder,
        is_not_expected_function: fn(&mut LanguageParser<'_, '_>) -> bool,
        abort_function: fn(&mut LanguageParser<'_, '_>, SyntaxKind) -> bool,
        expected: SyntaxKind,
        close_kind: SyntaxKind,
    ) -> PostSkipAction {
        // We're going to cheat here and pass the underlying SyntaxListBuilder of "list" to the helper method so that
        // it can append skipped trivia to the last element, regardless of whether that element is a node or a token.
        let mut trailing_trivia = Green::NULL;
        let action = self.skip_bad_list_tokens_with_expected_kind_helper(
            list,
            is_not_expected_function,
            abort_function,
            expected,
            close_kind,
            &mut trailing_trivia,
        );
        // (Roslyn would dereference a null start token here; no caller passes one with an empty list.)
        if !trailing_trivia.is_null() && !start_token.is_null() {
            *start_token = self.add_trailing_skipped_syntax(*start_token, trailing_trivia);
        }
        action
    }

    /// `private PostSkipAction SkipBadListTokensWithErrorCode<T, TNode>( ref T startToken, SyntaxListBuilder<TNode> list, Func<LanguageParser, bool> isNotExpectedFunction, Func<LanguageParser, bool> abortFunction, ErrorCode error) where T : CSharpSyntaxNode where TNode : CSharpSyntaxNode` (LanguageParser.cs:4550).
    pub(crate) fn skip_bad_list_tokens_with_error_code(
        &mut self,
        start_token: &mut Green,
        list: &mut SyntaxListBuilder,
        is_not_expected_function: fn(&mut LanguageParser<'_, '_>) -> bool,
        abort_function: fn(&mut LanguageParser<'_, '_>) -> bool,
        error: ErrorCode,
    ) -> PostSkipAction {
        let mut trailing_trivia = Green::NULL;
        let action = self.skip_bad_list_tokens_with_error_code_helper(
            list,
            is_not_expected_function,
            abort_function,
            error,
            &mut trailing_trivia,
        );
        if !trailing_trivia.is_null() && !start_token.is_null() {
            *start_token = self.add_trailing_skipped_syntax(*start_token, trailing_trivia);
        }
        action
    }

    /// `private PostSkipAction SkipBadListTokensWithExpectedKindHelper( SyntaxListBuilder list, Func<LanguageParser, bool> isNotExpectedFunction, Func<LanguageParser, SyntaxKind, bool> abortFunction, SyntaxKind expected, SyntaxKind closeKind, out GreenNode trailingTrivia)` (LanguageParser.cs:4572).
    pub(crate) fn skip_bad_list_tokens_with_expected_kind_helper(
        &mut self,
        list: &mut SyntaxListBuilder,
        is_not_expected_function: fn(&mut LanguageParser<'_, '_>) -> bool,
        abort_function: fn(&mut LanguageParser<'_, '_>, SyntaxKind) -> bool,
        expected: SyntaxKind,
        close_kind: SyntaxKind,
        trailing_trivia: &mut Green,
    ) -> PostSkipAction {
        if list.is_empty() {
            self.skip_bad_tokens_with_expected_kind(
                is_not_expected_function,
                abort_function,
                expected,
                close_kind,
                trailing_trivia,
            )
        } else {
            let mut last_item_trailing_trivia = Green::NULL;
            let action = self.skip_bad_tokens_with_expected_kind(
                is_not_expected_function,
                abort_function,
                expected,
                close_kind,
                &mut last_item_trailing_trivia,
            );
            if !last_item_trailing_trivia.is_null() {
                self.add_trailing_skipped_syntax_to_list(list, last_item_trailing_trivia);
            }
            *trailing_trivia = Green::NULL;
            action
        }
    }

    /// `private PostSkipAction SkipBadListTokensWithErrorCodeHelper<TNode>( SyntaxListBuilder<TNode> list, Func<LanguageParser, bool> isNotExpectedFunction, Func<LanguageParser, bool> abortFunction, ErrorCode error, out GreenNode trailingTrivia) where TNode : CSharpSyntaxNode` (LanguageParser.cs:4597).
    pub(crate) fn skip_bad_list_tokens_with_error_code_helper(
        &mut self,
        list: &mut SyntaxListBuilder,
        is_not_expected_function: fn(&mut LanguageParser<'_, '_>) -> bool,
        abort_function: fn(&mut LanguageParser<'_, '_>) -> bool,
        error: ErrorCode,
        trailing_trivia: &mut Green,
    ) -> PostSkipAction {
        if list.is_empty() {
            self.skip_bad_tokens_with_error_code(is_not_expected_function, abort_function, error, trailing_trivia)
        } else {
            let mut last_item_trailing_trivia = Green::NULL;
            let action = self.skip_bad_tokens_with_error_code(
                is_not_expected_function,
                abort_function,
                error,
                &mut last_item_trailing_trivia,
            );
            if !last_item_trailing_trivia.is_null() {
                self.add_trailing_skipped_syntax_to_list(list, last_item_trailing_trivia);
            }
            *trailing_trivia = Green::NULL;
            action
        }
    }

    /// `private PostSkipAction SkipBadTokensWithExpectedKind( Func<LanguageParser, bool> isNotExpectedFunction, Func<LanguageParser, SyntaxKind, bool> abortFunction, SyntaxKind expected, SyntaxKind closeKind, out GreenNode trailingTrivia)` (LanguageParser.cs:4621).
    pub(crate) fn skip_bad_tokens_with_expected_kind(
        &mut self,
        is_not_expected_function: fn(&mut LanguageParser<'_, '_>) -> bool,
        abort_function: fn(&mut LanguageParser<'_, '_>, SyntaxKind) -> bool,
        expected: SyntaxKind,
        close_kind: SyntaxKind,
        trailing_trivia: &mut Green,
    ) -> PostSkipAction {
        let mut nodes = self.pool.allocate();
        let mut first = true;
        let mut action = PostSkipAction::Continue;
        while is_not_expected_function(self) {
            if abort_function(self, close_kind) || self.is_terminator() {
                action = PostSkipAction::Abort;
                break;
            }

            let current = self.current_token();
            let token = if first && !self.base.arena_ref().contains_diagnostics(current) {
                self.eat_token_even_with_incorrect_kind(expected)
            } else {
                self.eat_token()
            };
            first = false;
            nodes.push(token);
        }

        *trailing_trivia = self.pool.to_list_and_free(self.base.lexer.arena(), nodes);
        action
    }

    /// `private PostSkipAction SkipBadTokensWithErrorCode( Func<LanguageParser, bool> isNotExpectedFunction, Func<LanguageParser, bool> abortFunction, ErrorCode errorCode, out GreenNode trailingTrivia)` (LanguageParser.cs:4648).
    pub(crate) fn skip_bad_tokens_with_error_code(
        &mut self,
        is_not_expected_function: fn(&mut LanguageParser<'_, '_>) -> bool,
        abort_function: fn(&mut LanguageParser<'_, '_>) -> bool,
        error_code: ErrorCode,
        trailing_trivia: &mut Green,
    ) -> PostSkipAction {
        let mut nodes = self.pool.allocate();
        let mut first = true;
        let mut action = PostSkipAction::Continue;
        while is_not_expected_function(self) {
            if abort_function(self) {
                action = PostSkipAction::Abort;
                break;
            }

            let current = self.current_token();
            let token = if first && !self.base.arena_ref().contains_diagnostics(current) {
                self.eat_token_with_prejudice(error_code, &[])
            } else {
                self.eat_token()
            };
            first = false;
            nodes.push(token);
        }

        *trailing_trivia = self.pool.to_list_and_free(self.base.lexer.arena(), nodes);
        action
    }

    /// `private AccessorDeclarationSyntax ParseAccessorDeclaration(AccessorDeclaringKind declaringKind)` (LanguageParser.cs:4674).
    pub(crate) fn parse_accessor_declaration(&mut self, declaring_kind: AccessorDeclaringKind) -> Green {
        let resetter = ParserSyntaxContextResetter::new(
            self,
            None,
            None,
            Some(declaring_kind == AccessorDeclaringKind::Property),
            None,
        );

        let mut acc_mods = self.pool.allocate();

        let acc_attrs = self.parse_attribute_declarations(false);
        let mut unused = false;
        self.parse_modifiers(&mut acc_mods, true, false, &mut unused);

        let expected_error = if declaring_kind == AccessorDeclaringKind::Event {
            ErrorCode::ERR_AddOrRemoveExpected
        } else {
            ErrorCode::ERR_GetOrSetExpected
        };
        let mut accessor_name = self.eat_token_code(SyntaxKind::IdentifierToken, expected_error, true);
        let accessor_kind = self.get_accessor_kind(accessor_name);

        // Only convert the identifier to a keyword if it's a valid one.  Otherwise any
        // other contextual keyword (like 'partial') will be converted into a keyword
        // and will be invalid.
        if accessor_kind == SyntaxKind::UnknownAccessorDeclaration {
            // We'll have an UnknownAccessorDeclaration either because we didn't have
            // an IdentifierToken or because we have an IdentifierToken which is not
            // add/remove/get/set.  In the former case, we'll already have reported
            // an error and will have a missing token.  But in the latter case we need
            // to report that the identifier is incorrect.
            if !self.base.arena_ref().is_missing(accessor_name) {
                accessor_name = self.add_error(accessor_name, expected_error);
            } else {
                debug_assert!(self.base.arena_ref().contains_diagnostics(accessor_name));
            }
        } else {
            accessor_name = self.convert_to_keyword(accessor_name);
        }

        let mut block_body = Green::NULL;
        let mut expression_body = Green::NULL;
        let mut semicolon = Green::NULL;

        let current_token_is_semicolon = self.current_kind() == SyntaxKind::SemicolonToken;
        let current_token_is_arrow = self.current_kind() == SyntaxKind::EqualsGreaterThanToken;
        let current_token_is_open_brace_token = self.current_kind() == SyntaxKind::OpenBraceToken;

        if current_token_is_open_brace_token || current_token_is_arrow {
            self.parse_block_and_expression_bodies_with_semicolon(
                &mut block_body,
                &mut expression_body,
                &mut semicolon,
                true,
            );
        } else if current_token_is_semicolon {
            semicolon = self.eat_accessor_semicolon();
        } else {
            // We didn't get something we recognized.  If we got an accessor type we
            // recognized (i.e. get/set/init/add/remove) then try to parse out a block.
            // Only do this if it doesn't seem like we're at the end of the accessor/property.
            // for example, if we have "get set", don't actually try to parse out the
            // block.  Otherwise we'll consume the 'set'.  In that case, just end the
            // current accessor with a semicolon so we can properly consume the next
            // in the calling method's loop.
            if accessor_kind != SyntaxKind::UnknownAccessorDeclaration {
                if !self.is_terminator() {
                    block_body = self.parse_method_or_accessor_body_block(Green::NULL, true);
                } else {
                    semicolon = self.eat_accessor_semicolon();
                }
            } else {
                // Don't bother eating anything if we didn't even have a valid accessor.
                // It will just lead to more errors.  Note: we will have already produced
                // a good error by now.
                debug_assert!(self.base.arena_ref().contains_diagnostics(accessor_name));
            }
        }

        let modifiers = self.pool.to_list_and_free(self.base.lexer.arena(), acc_mods);
        let result = self.base.arena().accessor_declaration(
            accessor_kind,
            acc_attrs,
            modifiers,
            accessor_name,
            block_body,
            expression_body,
            semicolon,
        );
        resetter.dispose(self);
        result
    }

    /// `private SyntaxToken EatAccessorSemicolon()` (LanguageParser.cs:4773).
    pub(crate) fn eat_accessor_semicolon(&mut self) -> Green {
        let code = if self.is_feature_enabled(MessageID::IDS_FeatureExpressionBodiedAccessor) {
            ErrorCode::ERR_SemiOrLBraceOrArrowExpected
        } else {
            ErrorCode::ERR_SemiOrLBraceExpected
        };
        self.eat_token_code(SyntaxKind::SemicolonToken, code, true)
    }

    /// `private static SyntaxKind GetAccessorKind(SyntaxToken accessorName)` (LanguageParser.cs:4779).
    pub(crate) fn get_accessor_kind(&self, accessor_name: Green) -> SyntaxKind {
        match self.base.arena_ref().contextual_kind(accessor_name) {
            SyntaxKind::GetKeyword => SyntaxKind::GetAccessorDeclaration,
            SyntaxKind::SetKeyword => SyntaxKind::SetAccessorDeclaration,
            SyntaxKind::InitKeyword => SyntaxKind::InitAccessorDeclaration,
            SyntaxKind::AddKeyword => SyntaxKind::AddAccessorDeclaration,
            SyntaxKind::RemoveKeyword => SyntaxKind::RemoveAccessorDeclaration,
            _ => SyntaxKind::UnknownAccessorDeclaration,
        }
    }

    /// `internal ParameterListSyntax ParseParenthesizedParameterList(bool forExtensionOrUnion)` (LanguageParser.cs:4792).
    pub(crate) fn parse_parenthesized_parameter_list(&mut self, for_extension_or_union: bool) -> Green {
        let mut open = Green::NULL;
        let mut close = Green::NULL;
        let parameters = self.parse_parameter_list(
            &mut open,
            &mut close,
            SyntaxKind::OpenParenToken,
            SyntaxKind::CloseParenToken,
            for_extension_or_union,
        );
        self.base.arena().parameter_list(open, parameters, close)
    }

    /// `internal BracketedParameterListSyntax ParseBracketedParameterList()` (LanguageParser.cs:4803).
    pub(crate) fn parse_bracketed_parameter_list(&mut self) -> Green {
        let mut open = Green::NULL;
        let mut close = Green::NULL;
        let parameters = self.parse_parameter_list(
            &mut open,
            &mut close,
            SyntaxKind::OpenBracketToken,
            SyntaxKind::CloseBracketToken,
            false,
        );
        self.base.arena().bracketed_parameter_list(open, parameters, close)
    }

    /// `private static bool CanReuseParameterList(Syntax.ParameterListSyntax list, bool identifierIsOptional)` (LanguageParser.cs:4814):
    /// incremental parsing only (`CurrentNode`, a node of the old tree).
    #[allow(dead_code)] // incremental parsing only: there is no old tree
    pub(crate) fn can_reuse_parameter_list(&self, list: Green, identifier_is_optional: bool) -> bool {
        if list.is_null() {
            return false;
        }

        let a = self.base.arena_ref();
        if a.is_missing(a.slot(list, T::ParameterListSyntax::OPEN_PAREN_TOKEN)) {
            return false;
        }

        if a.is_missing(a.slot(list, T::ParameterListSyntax::CLOSE_PAREN_TOKEN)) {
            return false;
        }

        let parameters = a.slot(list, T::ParameterListSyntax::PARAMETERS);
        separated_nodes(a, parameters).all(|parameter| self.can_reuse_parameter(parameter, identifier_is_optional))
    }

    /// `private static bool CanReuseBracketedParameterList(Syntax.BracketedParameterListSyntax list)` (LanguageParser.cs:4842):
    /// incremental parsing only.
    #[allow(dead_code)] // incremental parsing only: there is no old tree
    pub(crate) fn can_reuse_bracketed_parameter_list(&self, list: Green) -> bool {
        if list.is_null() {
            return false;
        }

        let a = self.base.arena_ref();
        if a.is_missing(a.slot(list, T::BracketedParameterListSyntax::OPEN_BRACKET_TOKEN)) {
            return false;
        }

        if a.is_missing(a.slot(list, T::BracketedParameterListSyntax::CLOSE_BRACKET_TOKEN)) {
            return false;
        }

        let parameters = a.slot(list, T::BracketedParameterListSyntax::PARAMETERS);
        separated_nodes(a, parameters).all(|parameter| self.can_reuse_parameter(parameter, false))
    }

    /// `private SeparatedSyntaxList<ParameterSyntax> ParseParameterList( out SyntaxToken open, out SyntaxToken close, SyntaxKind openKind, SyntaxKind closeKind, bool forExtensionOrUnion)` (LanguageParser.cs:4870).
    pub(crate) fn parse_parameter_list(
        &mut self,
        open: &mut Green,
        close: &mut Green,
        open_kind: SyntaxKind,
        close_kind: SyntaxKind,
        for_extension_or_union: bool,
    ) -> Green {
        *open = self.eat_token_kind(open_kind);

        let save_term = self.term_state;
        self.term_state = self.term_state | TerminatorState::IS_END_OF_PARAMETER_LIST;

        let parse_element: fn(&mut LanguageParser<'_, '_>) -> Green =
            if for_extension_or_union { |this| this.parse_parameter(true) } else { |this| this.parse_parameter(false) };

        let parameters = self.parse_comma_separated_syntax_list(
            open,
            close_kind,
            |this| this.is_possible_parameter(),
            parse_element,
            Self::parse_parameter_list_skip_bad_parameter_list_tokens,
            false,
            for_extension_or_union, // For extension/union declarations, we require at least one parameter
            false,
        );

        self.term_state = save_term;
        *close = self.eat_token_kind(close_kind);

        parameters
    }

    /// `skipBadParameterListTokens` (a local function of `ParseParameterList`, LanguageParser.cs:4901).
    fn parse_parameter_list_skip_bad_parameter_list_tokens(
        this: &mut LanguageParser<'_, '_>,
        open: &mut Green,
        list: &mut SyntaxListBuilder,
        expected_kind: SyntaxKind,
        close_kind: SyntaxKind,
    ) -> PostSkipAction {
        this.skip_bad_separated_list_tokens_with_expected_kind(
            open,
            list,
            |p| p.current_kind() != SyntaxKind::CommaToken && !p.is_possible_parameter(),
            |p, close_kind| p.current_kind() == close_kind,
            expected_kind,
            close_kind,
        )
    }

    /// `private bool IsEndOfParameterList()` (LanguageParser.cs:4911).
    pub(crate) fn is_end_of_parameter_list(&mut self) -> bool {
        matches!(
            self.current_kind(),
            SyntaxKind::CloseParenToken | SyntaxKind::CloseBracketToken | SyntaxKind::SemicolonToken
        )
    }

    /// `private bool IsPossibleParameter()` (LanguageParser.cs:4916).
    pub(crate) fn is_possible_parameter(&mut self) -> bool {
        match self.current_kind() {
            SyntaxKind::OpenBracketToken // attribute
            | SyntaxKind::ArgListKeyword
            | SyntaxKind::OpenParenToken // tuple
            => true,

            SyntaxKind::DelegateKeyword if self.is_function_pointer_start() => true, // Function pointer type

            SyntaxKind::IdentifierToken => self.is_true_identifier(),

            kind => {
                let current = self.current_token();
                self.is_parameter_modifier_excluding_scoped(current)
                    || self.is_definite_scoped_modifier(false, false)
                    || Self::is_predefined_type(kind)
            }
        }
    }

    /// `private static bool CanReuseParameter(CSharp.Syntax.ParameterSyntax parameter, bool identifierIsOptional)` (LanguageParser.cs:4936):
    /// incremental parsing only; the checks of the red tree's parents (lambda parameters) have no green form.
    #[allow(dead_code)] // incremental parsing only: there is no old tree
    pub(crate) fn can_reuse_parameter(&self, parameter: Green, identifier_is_optional: bool) -> bool {
        if parameter.is_null() {
            return false;
        }

        let a = self.base.arena_ref();
        // cannot reuse a node that possibly ends in an expression
        if !a.slot(parameter, T::ParameterSyntax::DEFAULT).is_null() {
            return false;
        }

        // We can only reuse parameters without identifiers (found in extension declarations) in context that allow optional identifiers.
        // The reverse is fine though.  Normal parameters (from non extensions) can be re-used into an extension declaration
        // as all normal parameters are legal extension parameters.
        if !identifier_is_optional && a.slot(parameter, T::ParameterSyntax::IDENTIFIER).is_null() {
            return false;
        }

        true
    }

    /// `private ParameterSyntax ParseParameter(bool identifierIsOptional)` (LanguageParser.cs:4980).
    pub(crate) fn parse_parameter(&mut self, identifier_is_optional: bool) -> Green {
        let attributes = self.parse_attribute_declarations(false);

        let mut modifiers = self.pool.allocate();
        self.parse_parameter_modifiers(&mut modifiers, false, false);

        if self.current_kind() == SyntaxKind::ArgListKeyword {
            // We store an __arglist parameter as a parameter with null type and whose
            // .Identifier has the kind ArgListKeyword.
            let arglist = self.eat_token_kind(SyntaxKind::ArgListKeyword);
            let modifiers = self.pool.to_list_and_free(self.base.lexer.arena(), modifiers);
            return self.base.arena().parameter(attributes, modifiers, Green::NULL, arglist, Green::NULL);
        }

        let type_ = self.parse_type(ParseTypeMode::Parameter);

        let mut identifier;
        if self.current_kind() == SyntaxKind::IdentifierToken && self.is_current_token_where_of_constraint_clause() {
            identifier = if identifier_is_optional {
                Green::NULL
            } else {
                let missing = self.create_missing_identifier_token();
                self.add_error(missing, ErrorCode::ERR_IdentifierExpected)
            };
        } else {
            // The receiver parameter on an extension declaration may have a name or not
            identifier = if identifier_is_optional && self.current_kind() != SyntaxKind::IdentifierToken {
                Green::NULL
            } else {
                self.parse_identifier_token(ErrorCode::ERR_IdentifierExpected)
            };
        }

        // When the user type "int goo[]", give them a useful error
        if !identifier.is_null()
            && self.current_kind() == SyntaxKind::OpenBracketToken
            && self.peek_kind(1) == SyntaxKind::CloseBracketToken
        {
            let open = self.eat_token();
            let open = self.add_error(open, ErrorCode::ERR_BadArraySyntax);
            let close = self.eat_token();
            let skipped = self.base.arena().list(&[open, close]);
            identifier = self.add_trailing_skipped_syntax(identifier, skipped);
        }

        let equals_token = self.try_eat_token(SyntaxKind::EqualsToken);

        let modifiers = self.pool.to_list_and_free(self.base.lexer.arena(), modifiers);
        let default = if equals_token.is_null() {
            Green::NULL
        } else {
            let value = self.parse_expression_core();
            self.base.arena().equals_value_clause(equals_token, value)
        };
        self.base.arena().parameter(attributes, modifiers, type_, identifier, default)
    }

    /// `internal static bool NoTriviaBetween(SyntaxToken token1, SyntaxToken token2)` (LanguageParser.cs:5033).
    pub(crate) fn no_trivia_between(&mut self, token1: Green, token2: Green) -> bool {
        let a = self.base.arena_ref();
        a.trailing_trivia_width(token1) == 0 && a.leading_trivia_width(token2) == 0
    }

    /// `private static bool IsParameterModifierIncludingScoped(SyntaxToken token)` (LanguageParser.cs:5038).
    pub(crate) fn is_parameter_modifier_including_scoped(&mut self, token: Green) -> bool {
        self.is_parameter_modifier_excluding_scoped(token)
            || self.base.arena_ref().contextual_kind(token) == SyntaxKind::ScopedKeyword
    }

    /// `private static bool IsParameterModifierExcludingScoped(SyntaxToken token)` (LanguageParser.cs:5041).
    pub(crate) fn is_parameter_modifier_excluding_scoped(&self, token: Green) -> bool {
        matches!(
            self.base.arena_ref().kind(token),
            SyntaxKind::ThisKeyword
                | SyntaxKind::RefKeyword
                | SyntaxKind::OutKeyword
                | SyntaxKind::InKeyword
                | SyntaxKind::ParamsKeyword
                | SyntaxKind::ReadOnlyKeyword
        )
    }

    /// `private void ParseParameterModifiers(SyntaxListBuilder modifiers, bool isFunctionPointerParameter, bool isLambdaParameter)` (LanguageParser.cs:5057).
    pub(crate) fn parse_parameter_modifiers(
        &mut self,
        modifiers: &mut SyntaxListBuilder,
        is_function_pointer_parameter: bool,
        is_lambda_parameter: bool,
    ) {
        debug_assert!(
            !(is_function_pointer_parameter && is_lambda_parameter),
            "Can't be parsing parameters for both a function pointer and a lambda at the same time"
        );

        let mut seen_scoped = false;
        loop {
            // Normal keyword-modifier (in/out/ref/readonly/params/this).  Always safe to consume.
            let current = self.current_token();
            if self.is_parameter_modifier_excluding_scoped(current) {
                let token = self.eat_token();
                modifiers.push(token);
                continue;
            }

            // 'scoped' modifier.  May be ambiguous with a type/identifier.  And has changed parsing rules between
            // C#13/14 inside a lambda parameter list.
            if self.is_definite_scoped_modifier(is_function_pointer_parameter, is_lambda_parameter) {
                // First scoped-modifier is always considered the modifier.
                if !seen_scoped {
                    seen_scoped = true;
                    let token = self.eat_contextual_token(SyntaxKind::ScopedKeyword);
                    modifiers.push(token);
                    continue;
                } else {
                    // If we've already seen `scoped` then we may have a situation like `scoped scoped`. This could
                    // be duplicated modifier, or it could be that the second `scoped` is actually the identifier of
                    // a parameter.
                    //
                    // Places where it is an identifier are:
                    //
                    //      `(scoped scoped) =>`
                    //      `(scoped scoped, ...) =>`
                    //      `(scoped scoped = ...) =>`
                    if !matches!(
                        self.peek_kind(1),
                        SyntaxKind::CloseParenToken | SyntaxKind::CommaToken | SyntaxKind::EqualsToken
                    ) {
                        let token = self.eat_contextual_token(SyntaxKind::ScopedKeyword);
                        modifiers.push(token);
                        continue;
                    }
                }
            }

            // Not a modifier.  We're done.
            return;
        }
    }

    /// `private FieldDeclarationSyntax ParseFixedSizeBufferDeclaration( SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, SyntaxKind parentKind)` (LanguageParser.cs:5106).
    pub(crate) fn parse_fixed_size_buffer_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        parent_kind: SyntaxKind,
    ) -> Green {
        debug_assert!(self.current_kind() == SyntaxKind::FixedKeyword);

        let fixed = self.eat_token();
        modifiers.push(fixed);

        let type_ = self.parse_type(ParseTypeMode::Normal);

        let modifiers = self.base.arena().list(modifiers);
        let variables = self.parse_field_declaration_variable_declarators(type_, VariableFlags::FIXED, parent_kind);
        let declaration = self.base.arena().variable_declaration(type_, variables);
        let semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
        self.base.arena().field_declaration(attributes, modifiers, declaration, semicolon)
    }

    /// `private MemberDeclarationSyntax ParseEventDeclaration( SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, SyntaxKind parentKind)` (LanguageParser.cs:5124).
    pub(crate) fn parse_event_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        parent_kind: SyntaxKind,
    ) -> Green {
        debug_assert!(self.current_kind() == SyntaxKind::EventKeyword);

        let event_token = self.eat_token();
        let type_ = self.parse_type(ParseTypeMode::Normal);

        if self.is_field_declaration(true, parent_kind == SyntaxKind::CompilationUnit) {
            self.parse_event_field_declaration(attributes, modifiers, event_token, type_, parent_kind)
        } else {
            self.parse_event_declaration_with_accessors(attributes, modifiers, event_token, type_)
        }
    }

    /// `private EventDeclarationSyntax ParseEventDeclarationWithAccessors( SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, SyntaxToken eventToken, TypeSyntax type)` (LanguageParser.cs:5139).
    pub(crate) fn parse_event_declaration_with_accessors(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        event_token: Green,
        type_: Green,
    ) -> Green {
        let mut explicit_interface_opt = Green::NULL;
        let mut identifier_or_this_opt = Green::NULL;
        let mut type_parameter_list = Green::NULL;

        self.parse_member_name(
            &mut explicit_interface_opt,
            &mut identifier_or_this_opt,
            &mut type_parameter_list,
            true,
        );

        // If we got an explicitInterfaceOpt but not an identifier, then we're in the special
        // case for ERR_ExplicitEventFieldImpl (see ParseMemberName for details).
        if !explicit_interface_opt.is_null()
            && !matches!(self.current_kind(), SyntaxKind::OpenBraceToken | SyntaxKind::SemicolonToken)
        {
            debug_assert!(type_parameter_list.is_null(), "Exit condition of ParseMemberName in this scenario");
            let identifier = if identifier_or_this_opt.is_null() {
                self.create_missing_identifier_token()
            } else {
                identifier_or_this_opt
            };
            let a = self.base.arena();
            let modifiers = a.list(modifiers);
            let open = a.missing_token(SyntaxKind::OpenBraceToken);
            let close = a.missing_token(SyntaxKind::CloseBraceToken);
            let accessor_list = a.accessor_list(open, Green::NULL, close);
            return a.event_declaration(
                attributes,
                modifiers,
                event_token,
                type_,
                //already has an appropriate error attached
                explicit_interface_opt,
                // No need for a diagnostic, ParseMemberName has already added one.
                identifier,
                accessor_list,
                Green::NULL,
            );
        }

        let mut identifier;

        if identifier_or_this_opt.is_null() {
            identifier = self.create_missing_identifier_token();
        } else if self.base.arena_ref().kind(identifier_or_this_opt) != SyntaxKind::IdentifierToken {
            debug_assert!(self.base.arena_ref().kind(identifier_or_this_opt) == SyntaxKind::ThisKeyword);
            identifier =
                self.convert_to_missing_with_trailing_trivia(identifier_or_this_opt, SyntaxKind::IdentifierToken);
        } else {
            identifier = identifier_or_this_opt;
        }

        debug_assert!(!identifier.is_null());
        debug_assert!(self.base.arena_ref().kind(identifier) == SyntaxKind::IdentifierToken);

        if self.base.arena_ref().is_missing(identifier) && !self.base.arena_ref().is_missing(type_) {
            identifier = self.add_error(identifier, ErrorCode::ERR_IdentifierExpected);
        }

        if !type_parameter_list.is_null() {
            // check to see if the user tried to create a generic event.
            identifier = self.add_trailing_skipped_syntax(identifier, type_parameter_list);
            identifier = self.add_error(identifier, ErrorCode::ERR_UnexpectedGenericName);
        }

        let mut accessor_list = Green::NULL;
        let mut semicolon = Green::NULL;

        if !explicit_interface_opt.is_null() && self.current_kind() == SyntaxKind::SemicolonToken {
            semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
        } else {
            accessor_list = self.parse_accessor_list(AccessorDeclaringKind::Event);
        }

        let a = self.base.arena();
        let modifiers = a.list(modifiers);
        let decl = a.event_declaration(
            attributes,
            modifiers,
            event_token,
            type_,
            explicit_interface_opt,
            identifier,
            accessor_list,
            semicolon,
        );

        self.eat_unexpected_trailing_semicolon(decl)
    }

    /// `private TNode EatUnexpectedTrailingSemicolon<TNode>(TNode decl) where TNode : CSharpSyntaxNode` (LanguageParser.cs:5229).
    pub(crate) fn eat_unexpected_trailing_semicolon(&mut self, decl: Green) -> Green {
        let mut decl = decl;
        // allow for case of one unexpected semicolon...
        if self.current_kind() == SyntaxKind::SemicolonToken {
            let semi = self.eat_token();
            let semi = self.add_error(semi, ErrorCode::ERR_UnexpectedSemicolon);
            decl = self.add_trailing_skipped_syntax(decl, semi);
        }

        decl
    }

    /// `private FieldDeclarationSyntax ParseNormalFieldDeclaration( SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, TypeSyntax type, SyntaxKind parentKind)` (LanguageParser.cs:5242).
    pub(crate) fn parse_normal_field_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        type_: Green,
        parent_kind: SyntaxKind,
    ) -> Green {
        let mut type_ = type_;
        let variables =
            self.parse_field_declaration_variable_declarators(type_, VariableFlags::LOCAL_OR_FIELD, parent_kind);

        // Make 'scoped' part of the type when it is the last token in the modifiers list
        if let Some(&scoped_keyword) = modifiers.last()
            && scoped_keyword.is_token()
            && self.base.arena_ref().kind(scoped_keyword) == SyntaxKind::ScopedKeyword
        {
            type_ = self.base.arena().scoped_type(scoped_keyword, type_);
            modifiers.pop();
        }

        let a = self.base.arena();
        let modifiers = a.list(modifiers);
        let declaration = a.variable_declaration(type_, variables);
        let semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
        self.base.arena().field_declaration(attributes, modifiers, declaration, semicolon)
    }

    /// `private EventFieldDeclarationSyntax ParseEventFieldDeclaration( SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, SyntaxToken eventToken, TypeSyntax type, SyntaxKind parentKind)` (LanguageParser.cs:5264).
    pub(crate) fn parse_event_field_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        event_token: Green,
        type_: Green,
        parent_kind: SyntaxKind,
    ) -> Green {
        // An attribute specified on an event declaration that omits event accessors can apply
        // to the event being declared, to the associated field (if the event is not abstract),
        // or to the associated add and remove methods. In the absence of an
        // attribute-target-specifier, the attribute applies to the event. The presence of the
        // event attribute-target-specifier indicates that the attribute applies to the event;
        // the presence of the field attribute-target-specifier indicates that the attribute
        // applies to the field; and the presence of the method attribute-target-specifier
        // indicates that the attribute applies to the methods.
        //
        // NOTE(cyrusn): We allow more than the above here.  Specifically, even if the event is
        // abstract, we allow the attribute to specify that it belongs to a field.  Later, in the
        // semantic pass, we will disallow this.

        let mut event_token = event_token;
        let variables = self.parse_field_declaration_variable_declarators(type_, VariableFlags::NONE, parent_kind);
        if self.current_kind() == SyntaxKind::DotToken {
            // Better error message for confusing event situation.
            event_token = self.add_error(event_token, ErrorCode::ERR_ExplicitEventFieldImpl);
        }

        let a = self.base.arena();
        let modifiers = a.list(modifiers);
        let declaration = a.variable_declaration(type_, variables);
        let semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
        self.base.arena().event_field_declaration(attributes, modifiers, event_token, declaration, semicolon)
    }

    /// `private bool IsEndOfFieldDeclaration()` (LanguageParser.cs:5299).
    pub(crate) fn is_end_of_field_declaration(&mut self) -> bool {
        self.current_kind() == SyntaxKind::SemicolonToken
    }

    /// `private SeparatedSyntaxList<VariableDeclaratorSyntax> ParseFieldDeclarationVariableDeclarators( TypeSyntax type, VariableFlags flags, SyntaxKind parentKind)` (LanguageParser.cs:5304).
    pub(crate) fn parse_field_declaration_variable_declarators(
        &mut self,
        type_: Green,
        flags: VariableFlags,
        parent_kind: SyntaxKind,
    ) -> Green {
        // Although we try parse variable declarations in contexts where they are not allowed (non-interactive top-level or a namespace)
        // the reported errors should take into consideration whether or not one expects them in the current context.
        let variable_declarations_expected =
            !matches!(parent_kind, SyntaxKind::NamespaceDeclaration | SyntaxKind::FileScopedNamespaceDeclaration)
                && (parent_kind != SyntaxKind::CompilationUnit || self.is_script());

        let mut variables = self.pool.allocate();

        let save_term = self.term_state;
        self.term_state = self.term_state | TerminatorState::IS_END_OF_FIELD_DECLARATION;

        let mut local_function = Green::NULL;
        self.parse_variable_declarators(
            type_,
            flags,
            &mut variables,
            variable_declarations_expected,
            false,
            // A field declaration doesn't have a `(...)` construct.  So no need to stop if we hit a close paren
            // after a declarator.  Let normal error recovery kick in.
            false,
            Green::NULL,
            Green::NULL,
            &mut local_function,
        );
        debug_assert!(local_function.is_null());

        self.term_state = save_term;
        self.pool.to_list_and_free(self.base.lexer.arena(), variables)
    }

    /// `private void ParseVariableDeclarators( TypeSyntax type, VariableFlags flags, SeparatedSyntaxListBuilder<VariableDeclaratorSyntax> variables, bool variableDeclarationsExpected, bool allowLocalFunctions, bool stopOnCloseParen, SyntaxList<AttributeListSyntax> attributes, SyntaxList<SyntaxToken> mods, out LocalFunctionStatementSyntax localFunction)` (LanguageParser.cs:5336).
    pub(crate) fn parse_variable_declarators(
        &mut self,
        type_: Green,
        flags: VariableFlags,
        variables: &mut SyntaxListBuilder,
        variable_declarations_expected: bool,
        allow_local_functions: bool,
        stop_on_close_paren: bool,
        attributes: Green,
        mods: Green,
        local_function: &mut Green,
    ) {
        let declarator = self.parse_variable_declarator(
            type_,
            flags,
            true,
            allow_local_functions,
            attributes,
            mods,
            local_function,
            false,
        );
        // `SyntaxListBuilder.Add` ignores a null node.
        if !declarator.is_null() {
            variables.push(declarator);
        }

        if !local_function.is_null() {
            // ParseVariableDeclarator returns null, so it is not added to variables
            debug_assert!(variables.is_empty());
            return;
        }

        loop {
            #[allow(clippy::if_same_then_else, reason = "the C# breaks from each branch")]
            if self.current_kind() == SyntaxKind::SemicolonToken {
                break;
            } else if stop_on_close_paren && self.current_kind() == SyntaxKind::CloseParenToken {
                break;
            } else if self.current_kind() == SyntaxKind::CommaToken {
                // If we see `for (int i = 0, i < ...` then we do not want to consume the second 'i' as the next declarator as it
                // is more likely that the user meant to write `for (int i = 0; i < ...` instead and accidentally
                // used a comma instead of a semicolon.
                //
                // Note: the legal forms that we must keep parsing as a variable declarator are:
                //
                //      for (int i = 0, j, k; ...       // identifier comma
                //      for (int i = 0, j = ...         // identifier equals
                //      for (int i = 0, j; ...          // identifier semicolon
                //
                // We also accept: `for (int i = 0, ;` as that's likely an intermediary state prior to writing the
                // next variable. Anything else we'll treat as as more likely to be the following conditional.

                if flags.contains(VariableFlags::FOR_STATEMENT) && self.peek_kind(1) != SyntaxKind::SemicolonToken {
                    let next = self.peek_token(1);
                    let is_legal_variable_declarator_start = self.is_true_identifier_with_token(next)
                        && matches!(
                            self.peek_kind(2),
                            SyntaxKind::CommaToken | SyntaxKind::EqualsToken | SyntaxKind::SemicolonToken
                        );

                    if !is_legal_variable_declarator_start {
                        break;
                    }
                }

                let comma = self.eat_token_kind(SyntaxKind::CommaToken);
                variables.push(comma);
                let declarator =
                    self.parse_variable_declarator(type_, flags, false, false, attributes, mods, local_function, false);
                if !declarator.is_null() {
                    variables.push(declarator);
                }

                debug_assert!(local_function.is_null());
            } else if !variable_declarations_expected
                || self.skip_bad_variable_list_tokens(variables, SyntaxKind::CommaToken) == PostSkipAction::Abort
            {
                break;
            }
        }
    }

    /// `private PostSkipAction SkipBadVariableListTokens(SeparatedSyntaxListBuilder<VariableDeclaratorSyntax> list, SyntaxKind expected)` (LanguageParser.cs:5419).
    pub(crate) fn skip_bad_variable_list_tokens(
        &mut self,
        list: &mut SyntaxListBuilder,
        expected: SyntaxKind,
    ) -> PostSkipAction {
        let mut tmp = Green::NULL;
        debug_assert!(!list.is_empty());
        self.skip_bad_separated_list_tokens_with_expected_kind(
            &mut tmp,
            list,
            |p| p.current_kind() != SyntaxKind::CommaToken,
            |p, _| p.current_kind() == SyntaxKind::SemicolonToken,
            expected,
            SyntaxKind::None,
        )
    }
}

/// `VariableFlags` (LanguageParser.cs:5430).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub(crate) struct VariableFlags(pub(crate) u32);

impl VariableFlags {
    pub(crate) const NONE: VariableFlags = VariableFlags(0);
    pub(crate) const FIXED: VariableFlags = VariableFlags(0x01);
    pub(crate) const CONST_: VariableFlags = VariableFlags(0x02);
    pub(crate) const LOCAL_OR_FIELD: VariableFlags = VariableFlags(0x04);
    pub(crate) const FOR_STATEMENT: VariableFlags = VariableFlags(0x08);
}

impl VariableFlags {
    /// `(value & flag) != 0`.
    #[inline]
    pub(crate) fn contains(self, flag: VariableFlags) -> bool {
        self.0 & flag.0 != 0
    }
}

impl std::ops::BitOr for VariableFlags {
    type Output = VariableFlags;

    #[inline]
    fn bitor(self, other: VariableFlags) -> VariableFlags {
        VariableFlags(self.0 | other.0)
    }
}

impl std::ops::BitAnd for VariableFlags {
    type Output = VariableFlags;

    #[inline]
    fn bitand(self, other: VariableFlags) -> VariableFlags {
        VariableFlags(self.0 & other.0)
    }
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private static SyntaxTokenList GetOriginalModifiers(CSharp.CSharpSyntaxNode decl)` (LanguageParser.cs:5439):
    /// incremental parsing only (a declaration of the old tree).
    #[allow(dead_code)] // incremental parsing only: there is no old tree
    pub(crate) fn get_original_modifiers(&self, decl: Green) -> Green {
        if !decl.is_null() {
            let a = self.base.arena_ref();
            match a.kind(decl) {
                SyntaxKind::FieldDeclaration
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::ConstructorDeclaration
                | SyntaxKind::DestructorDeclaration
                | SyntaxKind::PropertyDeclaration
                | SyntaxKind::EventFieldDeclaration
                | SyntaxKind::AddAccessorDeclaration
                | SyntaxKind::RemoveAccessorDeclaration
                | SyntaxKind::GetAccessorDeclaration
                | SyntaxKind::SetAccessorDeclaration
                | SyntaxKind::InitAccessorDeclaration
                | SyntaxKind::ClassDeclaration
                | SyntaxKind::StructDeclaration
                | SyntaxKind::UnionDeclaration
                | SyntaxKind::InterfaceDeclaration
                | SyntaxKind::RecordDeclaration
                | SyntaxKind::RecordStructDeclaration
                | SyntaxKind::DelegateDeclaration => {
                    // Every one of these keeps its modifiers in slot 1 (after the attribute lists).
                    return a.slot(decl, 1);
                }
                _ => {}
            }
        }

        Green::NULL
    }

    /// `private static bool WasFirstVariable(CSharp.Syntax.VariableDeclaratorSyntax variable)` (LanguageParser.cs:5478):
    /// incremental parsing only; it reads the old tree's parent, which a green node has not, so never.
    #[allow(dead_code)] // incremental parsing only: there is no old tree
    pub(crate) fn was_first_variable(&self, variable: Green) -> bool {
        let _ = variable;
        false
    }

    /// `private static VariableFlags GetOriginalVariableFlags(CSharp.Syntax.VariableDeclaratorSyntax old)` (LanguageParser.cs:5488):
    /// incremental parsing only; without the old tree's parent no flag is known.
    #[allow(dead_code)] // incremental parsing only: there is no old tree
    pub(crate) fn get_original_variable_flags(&self, old: Green) -> VariableFlags {
        let _ = old;
        VariableFlags::NONE
    }

    /// `private static bool CanReuseVariableDeclarator(CSharp.Syntax.VariableDeclaratorSyntax old, VariableFlags flags, bool isFirst)` (LanguageParser.cs:5511):
    /// incremental parsing only: a full parse reuses nothing.
    #[allow(dead_code)] // incremental parsing only: there is no old tree
    pub(crate) fn can_reuse_variable_declarator(&self, old: Green, flags: VariableFlags, is_first: bool) -> bool {
        let _ = (old, flags, is_first);
        false
    }

    /// `private VariableDeclaratorSyntax ParseVariableDeclarator( TypeSyntax parentType, VariableFlags flags, bool isFirst, bool allowLocalFunctions, SyntaxList<AttributeListSyntax> attributes, SyntaxList<SyntaxToken> mods, out LocalFunctionStatementSyntax localFunction, bool isExpressionContext = false)` (LanguageParser.cs:5527).
    pub(crate) fn parse_variable_declarator(
        &mut self,
        parent_type: Green,
        flags: VariableFlags,
        is_first: bool,
        allow_local_functions: bool,
        attributes: Green,
        mods: Green,
        local_function: &mut Green,
        is_expression_context: bool,
    ) -> Green {
        if !is_expression_context {
            // Check for the common pattern of:
            //
            // C                    //<-- here
            // Console.WriteLine();
            //
            // Standard greedy parsing will assume that this should be parsed as a variable
            // declaration: "C Console".  We want to avoid that as it can confused parts of the
            // system further up.  So, if we see certain things following the identifier, then we can
            // assume it's not the actual name.
            //
            // So, if we're after a newline and we see a name followed by the list below, then we
            // assume that we're accidentally consuming too far into the next statement.
            //
            // <dot>, <arrow>, any binary operator (except =), <question>.  None of these characters
            // are allowed in a normal variable declaration.  This also provides a more useful error
            // message to the user.  Instead of telling them that a semicolon is expected after the
            // following token, then instead get a useful message about an identifier being missing.
            // The above list prevents:
            //
            // C                    //<-- here
            // Console.WriteLine();
            //
            // C                    //<-- here
            // Console->WriteLine();
            //
            // C
            // A + B;
            //
            // C
            // A ? B : D;
            //
            // C
            // A()
            let reset_point = self.get_disposable_reset_point(true);

            let mut current_token_kind = self.current_kind();
            if current_token_kind == SyntaxKind::IdentifierToken && !self.base.arena_ref().is_missing(parent_type) {
                let last_token = self.base.arena_ref().last_token(parent_type);
                let is_after_new_line = self.trailing_trivia_has_end_of_line(last_token);
                if is_after_new_line {
                    // Note: this token is free to get.  There is a cached singleton in SyntaxFactory for it.
                    let mut missing_identifier = self.create_missing_identifier_token();
                    let (offset, width) = self.get_diagnostic_span_for_missing_node_or_token(missing_identifier);

                    self.eat_token();
                    current_token_kind = self.current_kind();

                    let is_non_equals_binary_token = current_token_kind != SyntaxKind::EqualsToken
                        && facts::is_binary_expression_operator_token(current_token_kind);

                    if matches!(
                        current_token_kind,
                        SyntaxKind::DotToken | SyntaxKind::OpenParenToken | SyntaxKind::MinusGreaterThanToken
                    ) || is_non_equals_binary_token
                    {
                        let is_possible_local_function_token =
                            matches!(current_token_kind, SyntaxKind::OpenParenToken | SyntaxKind::LessThanToken);

                        // Make sure this isn't a local function
                        if !is_possible_local_function_token || !self.is_local_function_after_identifier() {
                            missing_identifier = self.add_error_at(
                                missing_identifier,
                                offset,
                                width,
                                ErrorCode::ERR_IdentifierExpected,
                                &[],
                            );

                            *local_function = Green::NULL;
                            let result =
                                self.base.arena().variable_declarator(missing_identifier, Green::NULL, Green::NULL);
                            self.dispose_reset_point(reset_point);
                            return result;
                        }
                    }
                }
            }
            self.dispose_reset_point(reset_point);
        }

        // NOTE: Diverges from Dev10.
        //
        // When we see parse an identifier and we see the partial contextual keyword, we check
        // to see whether it is already attached to a partial class or partial method
        // declaration.  However, in the specific case of variable declarators, Dev10
        // specifically treats it as a variable name, even if it could be interpreted as a
        // keyword.
        let mut name = self.parse_identifier_token(ErrorCode::ERR_IdentifierExpected);
        let mut argument_list = Green::NULL;
        let mut initializer = Green::NULL;
        let save_term = self.term_state;
        let is_fixed = flags.contains(VariableFlags::FIXED);
        let is_const = flags.contains(VariableFlags::CONST_);
        let is_local_or_field = flags.contains(VariableFlags::LOCAL_OR_FIELD);

        // Give better error message in the case where the user did something like:
        //
        // X x = 1, Y y = 2;
        // using (X x = expr1, Y y = expr2) ...
        //
        // The superfluous type name is treated as variable (it is an identifier) and a missing ',' is injected after it.
        if !is_first && self.is_true_identifier() {
            name = self.add_error(name, ErrorCode::ERR_MultiTypeInDeclaration);
        }

        // The switch's `goto case`/`goto default` as a loop over the case to run; `None` is `default`.
        let mut case = self.current_kind();
        loop {
            match case {
                SyntaxKind::EqualsToken => {
                    if is_fixed {
                        case = SyntaxKind::None;
                        continue;
                    }

                    let equals = self.eat_token();

                    // check for lambda expression with explicit ref return type: `ref int () => { ... }`
                    let ref_keyword = if is_local_or_field
                        && !is_const
                        && self.current_kind() == SyntaxKind::RefKeyword
                        && !self.is_possible_lambda_expression(Precedence::EXPRESSION)
                    {
                        self.eat_token()
                    } else {
                        Green::NULL
                    };

                    let init = self.parse_variable_initializer();
                    let value =
                        if ref_keyword.is_null() { init } else { self.base.arena().ref_expression(ref_keyword, init) };
                    initializer = self.base.arena().equals_value_clause(equals, value);
                    break;
                }

                SyntaxKind::LessThanToken => {
                    if allow_local_functions && is_first {
                        *local_function =
                            self.try_parse_local_function_statement_body(attributes, mods, parent_type, name);
                        if !local_function.is_null() {
                            return Green::NULL;
                        }
                    }
                    case = SyntaxKind::None;
                    continue;
                }

                SyntaxKind::OpenParenToken => {
                    if allow_local_functions && is_first {
                        *local_function =
                            self.try_parse_local_function_statement_body(attributes, mods, parent_type, name);
                        if !local_function.is_null() {
                            return Green::NULL;
                        }
                    }

                    // Special case for accidental use of C-style constructors
                    // Fake up something to hold the arguments.
                    self.term_state = self.term_state | TerminatorState::IS_POSSIBLE_END_OF_VARIABLE_DECLARATION;
                    argument_list = self.parse_bracketed_argument_list();
                    self.term_state = save_term;
                    argument_list = self.add_error(argument_list, ErrorCode::ERR_BadVarDecl);
                    break;
                }

                SyntaxKind::OpenBracketToken => {
                    let mut saw_non_omitted_size = false;
                    self.term_state = self.term_state | TerminatorState::IS_POSSIBLE_END_OF_VARIABLE_DECLARATION;
                    let specifier = self.parse_array_rank_specifier(&mut saw_non_omitted_size);
                    self.term_state = save_term;
                    let a = self.base.arena_ref();
                    let open = a.slot(specifier, T::ArrayRankSpecifierSyntax::OPEN_BRACKET_TOKEN);
                    let sizes = a.slot(specifier, T::ArrayRankSpecifierSyntax::SIZES);
                    let mut close = a.slot(specifier, T::ArrayRankSpecifierSyntax::CLOSE_BRACKET_TOKEN);
                    if is_fixed && !saw_non_omitted_size {
                        close = self.add_error(close, ErrorCode::ERR_ValueExpected);
                    }

                    let mut args = self.pool.allocate();
                    let mut with_seps = self.pool.allocate();
                    add_range(self.base.arena_ref(), &mut with_seps, sizes);
                    for &item in &with_seps {
                        if !item.is_token() {
                            let mut expression = item;
                            let is_omitted =
                                self.base.arena_ref().kind(expression) == SyntaxKind::OmittedArraySizeExpression;
                            if !is_fixed && !is_omitted {
                                expression = self.add_error(expression, ErrorCode::ERR_ArraySizeInDeclaration);
                            }

                            let argument = self.base.arena().argument(Green::NULL, Green::NULL, expression);
                            args.push(argument);
                        } else {
                            args.push(item);
                        }
                    }
                    self.pool.free(with_seps);

                    let arguments = self.pool.to_list_and_free(self.base.lexer.arena(), args);
                    argument_list = self.base.arena().bracketed_argument_list(open, arguments, close);
                    if !is_fixed {
                        argument_list = self.add_error(argument_list, ErrorCode::ERR_CStyleArray);
                        // If we have "int x[] = new int[10];" then parse the initializer.
                        if self.current_kind() == SyntaxKind::EqualsToken {
                            case = SyntaxKind::EqualsToken;
                            continue;
                        }
                    }

                    break;
                }

                _ => {
                    debug_assert!(argument_list.is_null());
                    debug_assert!(initializer.is_null());

                    // Note: it is ok that we do this work prior to the isConst/isFixed checks below.  If it looks like
                    // a variable initializer, that means we're missing at least an equals and we'll report that error
                    // here.  So it's fine to not report the other errors related to const/fixed as they can be fixed up
                    // once the user adds the '='.
                    if self.parse_variable_declarator_looks_like_variable_initializer(name) {
                        debug_assert!(self.current_kind() != SyntaxKind::EqualsToken);

                        *local_function = Green::NULL;
                        let equals = self.eat_token_kind(SyntaxKind::EqualsToken);
                        let value = self.parse_variable_initializer();
                        let a = self.base.arena();
                        let initializer = a.equals_value_clause(equals, value);
                        return a.variable_declarator(name, Green::NULL, initializer);
                    }

                    if is_const {
                        name = self.add_error(name, ErrorCode::ERR_ConstValueRequired); // Error here for missing constant initializers
                    } else if is_fixed {
                        if self.base.arena_ref().kind(parent_type) == SyntaxKind::ArrayType {
                            // They accidentally put the array before the identifier
                            name = self.add_error(name, ErrorCode::ERR_FixedDimsRequired);
                        } else {
                            case = SyntaxKind::OpenBracketToken;
                            continue;
                        }
                    }

                    break;
                }
            }
        }

        *local_function = Green::NULL;
        self.base.arena().variable_declarator(name, argument_list, initializer)
    }

    /// `looksLikeVariableInitializer` (a local function of `ParseVariableDeclarator`, LanguageParser.cs:5781).
    fn parse_variable_declarator_looks_like_variable_initializer(&mut self, name: Green) -> bool {
        // Note: this check is redundant, as CanStartExpression will return false for an equals-token. However,
        // we want to guarantee that this always holds true, and thus the caller will *always* report an error
        // when trying to consume the equals token.  That ensures that we it's then ok to skip other syntax
        // errors that are reported with variable declarators.
        if self.current_kind() == SyntaxKind::EqualsToken {
            return false;
        }

        // If we see a token that can start an expression after the identifier (e.g., "int value 5;"),
        // treat it as a missing '=' and parse the initializer.
        //
        // Do this except for cases that are better served by saying we have a missing comma.  Specifically:
        //
        //      Type t1 t2 t3
        //      Type t1 t2,
        //      Type t1 t2 = ...
        //      Type t1 t2;
        //      Type t1 t2)    // likely an incorrect tuple.
        let should_parse_as_next_declarator = self.current_kind() == SyntaxKind::IdentifierToken
            && matches!(
                self.peek_kind(1),
                SyntaxKind::IdentifierToken
                    | SyntaxKind::CommaToken
                    | SyntaxKind::EqualsToken
                    | SyntaxKind::SemicolonToken
                    | SyntaxKind::CloseParenToken
                    | SyntaxKind::EndOfFileToken
            );
        if should_parse_as_next_declarator {
            return false;
        }

        if self.contains_error_diagnostic(name) {
            return false;
        }

        if !self.can_start_expression() {
            return false;
        }

        let reset_point = self.get_disposable_reset_point(true);
        let initializer = self.parse_expression_core();

        // If we see a type following, then prefer to view this as a declarator for the next variable.
        let result = if is_type_syntax_class(self.base.arena_ref().kind(initializer)) {
            false
        } else {
            !self.contains_error_diagnostic(initializer)
        };
        self.dispose_reset_point(reset_point);
        result
    }

    /// `private bool IsLocalFunctionAfterIdentifier()` (LanguageParser.cs:5824).
    pub(crate) fn is_local_function_after_identifier(&mut self) -> bool {
        debug_assert!(matches!(self.current_kind(), SyntaxKind::OpenParenToken | SyntaxKind::LessThanToken));

        let reset_point = self.get_disposable_reset_point(true);

        let _type_parameter_list_opt = self.parse_type_parameter_list();
        let param_list = self.parse_parenthesized_parameter_list(false);

        let result = !self.base.arena_ref().is_missing(param_list)
            && (matches!(self.current_kind(), SyntaxKind::OpenBraceToken | SyntaxKind::EqualsGreaterThanToken)
                || self.current_contextual_kind() == SyntaxKind::WhereKeyword);
        self.dispose_reset_point(reset_point);
        result
    }

    /// `private bool IsPossibleEndOfVariableDeclaration()` (LanguageParser.cs:5843).
    pub(crate) fn is_possible_end_of_variable_declaration(&mut self) -> bool {
        matches!(self.current_kind(), SyntaxKind::CommaToken | SyntaxKind::SemicolonToken)
    }

    /// `private ExpressionSyntax ParseVariableInitializer()` (LanguageParser.cs:5855).
    pub(crate) fn parse_variable_initializer(&mut self) -> Green {
        if self.current_kind() == SyntaxKind::OpenBraceToken {
            self.parse_array_initializer()
        } else {
            self.parse_expression_core()
        }
    }

    /// `private bool IsPossibleVariableInitializer()` (LanguageParser.cs:5862).
    pub(crate) fn is_possible_variable_initializer(&mut self) -> bool {
        self.current_kind() == SyntaxKind::OpenBraceToken || self.is_possible_expression()
    }

    /// `private FieldDeclarationSyntax ParseConstantFieldDeclaration(SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers, SyntaxKind parentKind)` (LanguageParser.cs:5867).
    pub(crate) fn parse_constant_field_declaration(
        &mut self,
        attributes: Green,
        modifiers: &mut SyntaxListBuilder,
        parent_kind: SyntaxKind,
    ) -> Green {
        let const_ = self.eat_token_kind(SyntaxKind::ConstKeyword);
        modifiers.push(const_);

        let type_ = self.parse_type(ParseTypeMode::Normal);
        let modifiers = self.base.arena().list(modifiers);
        let variables = self.parse_field_declaration_variable_declarators(type_, VariableFlags::CONST_, parent_kind);
        let declaration = self.base.arena().variable_declaration(type_, variables);
        let semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
        self.base.arena().field_declaration(attributes, modifiers, declaration, semicolon)
    }

    /// `private DelegateDeclarationSyntax ParseDelegateDeclaration(SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers)` (LanguageParser.cs:5881).
    pub(crate) fn parse_delegate_declaration(&mut self, attributes: Green, modifiers: &mut SyntaxListBuilder) -> Green {
        debug_assert!(self.current_kind() == SyntaxKind::DelegateKeyword);

        let delegate_token = self.eat_token_kind(SyntaxKind::DelegateKeyword);
        let type_ = self.parse_return_type();
        let save_term = self.term_state;
        self.term_state = self.term_state | TerminatorState::IS_END_OF_METHOD_SIGNATURE;
        let name = self.parse_identifier_token(ErrorCode::ERR_IdentifierExpected);
        let type_parameters = self.parse_type_parameter_list();
        let parameter_list = self.parse_parenthesized_parameter_list(false);
        let mut constraints = self.pool.allocate();

        if self.current_contextual_kind() == SyntaxKind::WhereKeyword {
            self.parse_type_parameter_constraint_clauses(&mut constraints);
        }

        self.term_state = save_term;

        let modifiers = self.base.arena().list(modifiers);
        let constraints = self.pool.to_list_and_free(self.base.lexer.arena(), constraints);
        let semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
        self.base.arena().delegate_declaration(
            attributes,
            modifiers,
            delegate_token,
            type_,
            name,
            type_parameters,
            parameter_list,
            constraints,
            semicolon,
        )
    }

    /// `private EnumDeclarationSyntax ParseEnumDeclaration(SyntaxList<AttributeListSyntax> attributes, SyntaxListBuilder modifiers)` (LanguageParser.cs:5914).
    pub(crate) fn parse_enum_declaration(&mut self, attributes: Green, modifiers: &mut SyntaxListBuilder) -> Green {
        debug_assert!(self.current_kind() == SyntaxKind::EnumKeyword);

        let enum_token = self.eat_token_kind(SyntaxKind::EnumKeyword);
        let mut name = self.parse_identifier_token(ErrorCode::ERR_IdentifierExpected);

        // check to see if the user tried to create a generic enum.
        let type_parameters = self.parse_type_parameter_list();

        if !type_parameters.is_null() {
            name = self.add_trailing_skipped_syntax(name, type_parameters);
            name = self.add_error(name, ErrorCode::ERR_UnexpectedGenericName);
        }

        let mut base_list = Green::NULL;
        if self.current_kind() == SyntaxKind::ColonToken {
            let colon = self.eat_token_kind(SyntaxKind::ColonToken);
            let type_ = self.parse_type(ParseTypeMode::Normal);
            let base = self.base.arena().simple_base_type(type_);
            base_list = self.base.arena().base_list(colon, base);
        }

        let mut members = Green::NULL;
        let semicolon;
        let mut open_brace;
        let close_brace;

        if self.current_kind() == SyntaxKind::SemicolonToken {
            semicolon = self.eat_token_kind(SyntaxKind::SemicolonToken);
            open_brace = Green::NULL;
            close_brace = Green::NULL;
        } else {
            open_brace = self.eat_token_kind(SyntaxKind::OpenBraceToken);

            if !self.base.arena_ref().is_missing(open_brace) {
                // It's not uncommon for people to use semicolons to separate out enum members.  So be resilient to
                // that, successfully consuming them as separators, while telling the user it needs to be a comma
                // instead.
                members = self.parse_comma_separated_syntax_list(
                    &mut open_brace,
                    SyntaxKind::CloseBraceToken,
                    |this| this.is_possible_enum_member_declaration(),
                    |this| this.parse_enum_member_declaration(),
                    Self::parse_enum_declaration_skip_bad_enum_member_list_tokens,
                    true,
                    false,
                    true,
                );
            }

            close_brace = self.eat_token_kind(SyntaxKind::CloseBraceToken);
            semicolon = self.try_eat_token(SyntaxKind::SemicolonToken);
        }

        let a = self.base.arena();
        let modifiers = a.list(modifiers);
        a.enum_declaration(
            attributes,
            modifiers,
            enum_token,
            name,
            base_list,
            open_brace,
            members,
            close_brace,
            semicolon,
        )
    }

    /// `skipBadEnumMemberListTokens` (a local function of `ParseEnumDeclaration`, LanguageParser.cs:5988).
    fn parse_enum_declaration_skip_bad_enum_member_list_tokens(
        this: &mut LanguageParser<'_, '_>,
        open_brace: &mut Green,
        list: &mut SyntaxListBuilder,
        expected_kind: SyntaxKind,
        close_kind: SyntaxKind,
    ) -> PostSkipAction {
        this.skip_bad_separated_list_tokens_with_expected_kind(
            open_brace,
            list,
            |p| {
                !matches!(p.current_kind(), SyntaxKind::CommaToken | SyntaxKind::SemicolonToken)
                    && !p.is_possible_enum_member_declaration()
            },
            |p, close_kind| p.current_kind() == close_kind,
            expected_kind,
            close_kind,
        )
    }

    /// `private EnumMemberDeclarationSyntax ParseEnumMemberDeclaration()` (LanguageParser.cs:5998).
    pub(crate) fn parse_enum_member_declaration(&mut self) -> Green {
        let member_attrs = self.parse_attribute_declarations(false);
        let member_name = self.parse_identifier_token(ErrorCode::ERR_IdentifierExpected);
        let mut equals_value = Green::NULL;
        if self.current_kind() == SyntaxKind::EqualsToken {
            //an identifier is a valid expression
            let equals = self.eat_token_kind(SyntaxKind::EqualsToken);
            let value = if matches!(self.current_kind(), SyntaxKind::CommaToken | SyntaxKind::CloseBraceToken) {
                self.parse_identifier_name(ErrorCode::ERR_ConstantExpected)
            } else {
                self.parse_expression_core()
            };
            equals_value = self.base.arena().equals_value_clause(equals, value);
        }

        self.base.arena().enum_member_declaration(member_attrs, Green::NULL, member_name, equals_value)
    }

    /// `private bool IsPossibleEnumMemberDeclaration()` (LanguageParser.cs:6021).
    pub(crate) fn is_possible_enum_member_declaration(&mut self) -> bool {
        self.current_kind() == SyntaxKind::OpenBracketToken || self.is_true_identifier()
    }

    /// `private bool IsDotOrColonColon()` (LanguageParser.cs:6026).
    pub(crate) fn is_dot_or_colon_colon(&mut self) -> bool {
        matches!(self.current_kind(), SyntaxKind::DotToken | SyntaxKind::ColonColonToken)
    }

    /// `public NameSyntax ParseName()` (LanguageParser.cs:6032).
    pub(crate) fn parse_name(&mut self) -> Green {
        self.parse_qualified_name(NameOptions::NONE)
    }

    /// `private IdentifierNameSyntax CreateMissingIdentifierName()` (LanguageParser.cs:6037).
    pub(crate) fn create_missing_identifier_name(&mut self) -> Green {
        let token = self.create_missing_identifier_token();
        self.base.arena().identifier_name(token)
    }

    /// `private static SyntaxToken CreateMissingIdentifierToken()` (LanguageParser.cs:6042).
    pub(crate) fn create_missing_identifier_token(&mut self) -> Green {
        self.base.arena().missing_token(SyntaxKind::IdentifierToken)
    }
}

/// `NameOptions` (LanguageParser.cs:6048).
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub(crate) struct NameOptions(pub(crate) u32);

impl NameOptions {
    pub(crate) const NONE: NameOptions = NameOptions(0);
    pub(crate) const IN_EXPRESSION: NameOptions = NameOptions(1 << 0);
    pub(crate) const IN_TYPE_LIST: NameOptions = NameOptions(1 << 1);
    pub(crate) const POSSIBLE_PATTERN: NameOptions = NameOptions(1 << 2);
    pub(crate) const AFTER_IS: NameOptions = NameOptions(1 << 3);
    pub(crate) const DEFINITE_PATTERN: NameOptions = NameOptions(1 << 4);
    pub(crate) const AFTER_OUT: NameOptions = NameOptions(1 << 5);
    pub(crate) const AFTER_TUPLE_COMMA: NameOptions = NameOptions(1 << 6);
    pub(crate) const FIRST_ELEMENT_OF_POSSIBLE_TUPLE_LITERAL: NameOptions = NameOptions(1 << 7);
}

impl NameOptions {
    /// `(value & flag) != 0`.
    #[inline]
    pub(crate) fn contains(self, flag: NameOptions) -> bool {
        self.0 & flag.0 != 0
    }
}

impl std::ops::BitOr for NameOptions {
    type Output = NameOptions;

    #[inline]
    fn bitor(self, other: NameOptions) -> NameOptions {
        NameOptions(self.0 | other.0)
    }
}

impl std::ops::BitAnd for NameOptions {
    type Output = NameOptions;

    #[inline]
    fn bitand(self, other: NameOptions) -> NameOptions {
        NameOptions(self.0 & other.0)
    }
}

impl<'l, 't> LanguageParser<'l, 't> {
    /// `private bool IsTrueIdentifier()` (LanguageParser.cs:6065).
    pub(crate) fn is_true_identifier(&mut self) -> bool {
        if self.current_kind() == SyntaxKind::IdentifierToken
            && !self.is_current_token_partial_keyword_of_partial_member_or_type()
            && !self.is_current_token_query_keyword_in_query()
            && !self.is_current_token_where_of_constraint_clause()
        {
            return true;
        }

        false
    }

    /// `private bool IsTrueIdentifier(SyntaxToken token)` (LanguageParser.cs:6084).
    pub(crate) fn is_true_identifier_with_token(&mut self, token: Green) -> bool {
        self.base.arena_ref().kind(token) == SyntaxKind::IdentifierToken
            && !(self.is_in_query && self.is_token_query_contextual_keyword(token))
    }

    /// `private IdentifierNameSyntax ParseIdentifierName(ErrorCode code = ErrorCode.ERR_IdentifierExpected)` (LanguageParser.cs:6091).
    pub(crate) fn parse_identifier_name(&mut self, code: ErrorCode) -> Green {
        let token = self.parse_identifier_token(code);
        self.base.arena().identifier_name(token)
    }

    /// `private SyntaxToken ParseIdentifierToken(ErrorCode code = ErrorCode.ERR_IdentifierExpected)` (LanguageParser.cs:6105).
    pub(crate) fn parse_identifier_token(&mut self, code: ErrorCode) -> Green {
        let ctk = self.current_kind();
        if ctk == SyntaxKind::IdentifierToken {
            // Error tolerance for IntelliSense. Consider the following case: [EditorBrowsable( partial class Goo {
            // } Because we're parsing an attribute argument we'll end up consuming the "partial" identifier and
            // we'll eventually end up in a pretty confused state.  Because of that it becomes very difficult to
            // show the correct parameter help in this case.  So, when we see "partial" we check if it's being used
            // as an identifier or as a contextual keyword.  If it's the latter then we bail out.  See
            // Bug: vswhidbey/542125
            if self.is_current_token_partial_keyword_of_partial_member_or_type()
                || self.is_current_token_query_keyword_in_query()
            {
                let result = self.create_missing_identifier_token();
                let current = self.current_token();
                return self.add_error_args(result, ErrorCode::ERR_InvalidExprTerm, &[DiagArg::TokenText(current)]);
            }

            let mut identifier_token = self.eat_token();

            if self.is_in_async && self.base.arena_ref().contextual_kind(identifier_token) == SyntaxKind::AwaitKeyword {
                identifier_token = self.add_error(identifier_token, ErrorCode::ERR_BadAwaitAsIdentifier);
            }

            identifier_token
        } else {
            let missing = self.create_missing_identifier_token();
            self.add_error(missing, code)
        }
    }

    /// `private bool IsCurrentTokenQueryKeywordInQuery()` (LanguageParser.cs:6138).
    pub(crate) fn is_current_token_query_keyword_in_query(&mut self) -> bool {
        // `IsCurrentTokenQueryContextualKeyword`: `IsTokenQueryContextualKeyword(this.CurrentToken)`.
        self.is_in_query && {
            let current = self.current_token();
            self.is_token_query_contextual_keyword(current)
        }
    }

    /// `private bool IsCurrentTokenPartialKeywordOfPartialMemberOrType()` (LanguageParser.cs:6143).
    pub(crate) fn is_current_token_partial_keyword_of_partial_member_or_type(&mut self) -> bool {
        if self.current_contextual_kind() == SyntaxKind::PartialKeyword
            && (self.is_partial_type() || self.is_partial_member())
        {
            return true;
        }

        false
    }

    /// `private bool IsCurrentTokenFieldInKeywordContext()` (LanguageParser.cs:6156).
    pub(crate) fn is_current_token_field_in_keyword_context(&mut self) -> bool {
        self.current_contextual_kind() == SyntaxKind::FieldKeyword
            && self.is_in_field_keyword_context
            && self.is_feature_enabled(MessageID::IDS_FeatureFieldKeyword)
    }

    /// `PeekToken(n).ContextualKind` (not a method of Roslyn's: its `is { ContextualKind: ... }` patterns).
    pub(crate) fn peek_contextual_kind(&mut self, n: i32) -> SyntaxKind {
        self.base.peek_contextual_kind(n)
    }

    /// `token.TrailingTrivia.Any((int)SyntaxKind.EndOfLineTrivia)` (not a method of Roslyn's).
    pub(crate) fn trailing_trivia_has_end_of_line(&self, token: Green) -> bool {
        let a = self.base.arena_ref();
        a.trivia_items(a.trailing_trivia(token)).iter().any(|&t| a.trivia_kind(t) == SyntaxKind::EndOfLineTrivia)
    }
}

/// `ParserSyntaxContextResetter` (LanguageParser.cs:4344): sets the parser's syntax context (`IsInAsync`,
/// `IsInQuery`, `IsInFieldKeywordContext`, `ForceConditionalAccessExpression`) where a value is given and restores
/// the previous one on `dispose` (Rust has no `using`: a caller disposes it where the C# `using` scope ends).
#[derive(Copy, Clone, Debug)]
pub(crate) struct ParserSyntaxContextResetter {
    /// `_previousState` (`LanguageParserState`).
    previous_state: u8,
}

impl ParserSyntaxContextResetter {
    /// `LanguageParserState.IsInAsync`.
    const IS_IN_ASYNC: u8 = 1 << 0;
    /// `LanguageParserState.IsInQuery`.
    const IS_IN_QUERY: u8 = 1 << 1;
    /// `LanguageParserState.IsInFieldKeywordContext`.
    const IS_IN_FIELD_KEYWORD_CONTEXT: u8 = 1 << 2;
    /// `LanguageParserState.ForceConditionalAccessExpression`.
    const FORCE_CONDITIONAL_ACCESS_EXPRESSION: u8 = 1 << 3;

    /// `ParserSyntaxContextResetter(LanguageParser parser, bool? isInAsyncContext = null, bool? isInQueryContext =
    /// null, bool? isInFieldKeywordContext = null, bool? forceConditionalAccessExpression = null)`.
    pub(crate) fn new(
        parser: &mut LanguageParser<'_, '_>,
        is_in_async_context: Option<bool>,
        is_in_query_context: Option<bool>,
        is_in_field_keyword_context: Option<bool>,
        force_conditional_access_expression: Option<bool>,
    ) -> ParserSyntaxContextResetter {
        debug_assert!(
            is_in_async_context.is_some()
                || is_in_query_context.is_some()
                || is_in_field_keyword_context.is_some()
                || force_conditional_access_expression.is_some()
        );

        let previous_state = (if parser.is_in_async { Self::IS_IN_ASYNC } else { 0 })
            | (if parser.is_in_query { Self::IS_IN_QUERY } else { 0 })
            | (if parser.is_in_field_keyword_context { Self::IS_IN_FIELD_KEYWORD_CONTEXT } else { 0 })
            | (if parser.force_conditional_access_expression { Self::FORCE_CONDITIONAL_ACCESS_EXPRESSION } else { 0 });

        parser.is_in_async = is_in_async_context.unwrap_or(parser.is_in_async);
        parser.is_in_query = is_in_query_context.unwrap_or(parser.is_in_query);
        parser.is_in_field_keyword_context = is_in_field_keyword_context.unwrap_or(parser.is_in_field_keyword_context);
        parser.force_conditional_access_expression =
            force_conditional_access_expression.unwrap_or(parser.force_conditional_access_expression);
        ParserSyntaxContextResetter { previous_state }
    }

    /// `Dispose()`.
    pub(crate) fn dispose(self, parser: &mut LanguageParser<'_, '_>) {
        parser.is_in_async = self.previous_state & Self::IS_IN_ASYNC != 0;
        parser.is_in_query = self.previous_state & Self::IS_IN_QUERY != 0;
        parser.is_in_field_keyword_context = self.previous_state & Self::IS_IN_FIELD_KEYWORD_CONTEXT != 0;
        parser.force_conditional_access_expression =
            self.previous_state & Self::FORCE_CONDITIONAL_ACCESS_EXPRESSION != 0;
    }
}

/// The slots of a `TypeDeclarationSyntax` that `ParseNamespaceBody` reads and replaces (they differ per node type:
/// a record has `ClassOrStructKeyword`, an extension block no identifier or base list).
#[derive(Copy, Clone, Debug)]
pub(crate) struct TypeDeclarationSlots {
    pub(crate) members: usize,
    pub(crate) close_brace_token: usize,
    pub(crate) semicolon_token: usize,
}

/// `node is TypeDeclarationSyntax`: the slots of a node of `kind` when it is one.
pub(crate) fn type_declaration_slots(kind: SyntaxKind) -> Option<TypeDeclarationSlots> {
    macro_rules! slots {
        ($t:ident) => {
            Some(TypeDeclarationSlots {
                members: T::$t::MEMBERS,
                close_brace_token: T::$t::CLOSE_BRACE_TOKEN,
                semicolon_token: T::$t::SEMICOLON_TOKEN,
            })
        };
    }
    match kind {
        SyntaxKind::ClassDeclaration => slots!(ClassDeclarationSyntax),
        SyntaxKind::StructDeclaration => slots!(StructDeclarationSyntax),
        SyntaxKind::UnionDeclaration => slots!(UnionDeclarationSyntax),
        SyntaxKind::InterfaceDeclaration => slots!(InterfaceDeclarationSyntax),
        SyntaxKind::RecordDeclaration | SyntaxKind::RecordStructDeclaration => slots!(RecordDeclarationSyntax),
        SyntaxKind::ExtensionBlockDeclaration => slots!(ExtensionBlockDeclarationSyntax),
        _ => None,
    }
}

/// `node is TypeSyntax` (the class, which `SyntaxFacts.IsTypeSyntax` is not: it leaves out `OmittedTypeArgument`,
/// `RefType` and `ScopedType`).
pub(crate) fn is_type_syntax_class(kind: SyntaxKind) -> bool {
    facts::is_type_syntax(kind)
        || matches!(kind, SyntaxKind::OmittedTypeArgument | SyntaxKind::RefType | SyntaxKind::ScopedType)
}

/// `SyntaxListBuilder.AddRange(SyntaxList<GreenNode>)` of a node: a list's elements, or the node itself (nothing for
/// null).
pub(crate) fn add_range(a: &crate::green::GreenArena, builder: &mut SyntaxListBuilder, list: Green) {
    if list.is_list() {
        builder.extend_from_slice(a.slots(list));
    } else if !list.is_null() {
        builder.push(list);
    }
}

/// `SeparatedSyntaxList<T>.Count` of a separated list's green node: its elements, separators not counted.
pub(crate) fn separated_count(a: &crate::green::GreenArena, list: Green) -> usize {
    if list.is_null() {
        0
    } else if list.is_list() {
        a.slot_count(list).div_ceil(2)
    } else {
        1
    }
}

/// The elements of a separated list's green node (every other slot of a list, or the node itself).
#[allow(dead_code)] // for the incremental parsing checks only
pub(crate) fn separated_nodes(a: &crate::green::GreenArena, list: Green) -> impl Iterator<Item = Green> + '_ {
    let (slots, single): (&[Green], Option<Green>) = if list.is_list() {
        (a.slots(list), None)
    } else if list.is_null() {
        (&[], None)
    } else {
        (&[], Some(list))
    };
    slots.iter().copied().step_by(2).chain(single)
}
