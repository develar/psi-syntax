//! `ParsingContext` holds the builder and the parser's scope stack.

use crate::language::Starlark;
use crate::parser::parsing_scope::ParsingScope;
use psi_builder::builder::{Nesting, PsiBuilderImpl, TOO_DEEPLY_NESTED};

pub(crate) struct ParsingContext<'a> {
    pub(crate) builder: &'a mut PsiBuilderImpl<Starlark>,
    scopes: Vec<ParsingScope>,
}

impl<'a> ParsingContext<'a> {
    pub(crate) fn new(builder: &'a mut PsiBuilderImpl<Starlark>) -> ParsingContext<'a> {
        ParsingContext { builder, scopes: vec![ParsingScope::default()] }
    }

    pub(crate) fn get_scope(&self) -> &ParsingScope {
        self.scopes.last().expect("the root parsing scope is always present")
    }

    pub(crate) fn get_scope_mut(&mut self) -> &mut ParsingScope {
        self.scopes.last_mut().expect("the root parsing scope is always present")
    }

    pub(crate) fn empty_parsing_scope(&self) -> ParsingScope {
        ParsingScope::default()
    }

    pub(crate) fn enter_nesting_level(&mut self) -> bool {
        self.builder.enter_nesting(Nesting::Level)
    }

    pub(crate) fn exit_nesting_level(&mut self) {
        self.builder.exit_nesting(Nesting::Level);
    }

    pub(crate) fn too_deeply_nested(&mut self) {
        let error = self.builder.mark();
        if !self.builder.eof() {
            self.builder.skip_too_deeply_nested(None);
        }
        error.error(self.builder, TOO_DEEPLY_NESTED);
    }

    pub(crate) fn push_scope(&mut self, scope: ParsingScope) {
        let previous_scope = self.get_scope().clone();
        self.scopes.push(scope);
        self.reset_builder_cache(&previous_scope);
    }

    pub(crate) fn pop_scope(&mut self) -> ParsingScope {
        let previous_scope = self.scopes.pop().expect("a pushed parsing scope is present");
        self.reset_builder_cache(&previous_scope);
        previous_scope
    }

    fn reset_builder_cache(&mut self, previous_scope: &ParsingScope) {
        if self.get_scope() != previous_scope {
            let marker = self.builder.mark();
            marker.rollback_to(self.builder);
        }
    }
}
