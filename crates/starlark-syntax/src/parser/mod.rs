//! The Starlark parser, ported method by method from the Bazel plugin.

mod expression_parsing;
mod function_parsing;
mod messages;
mod parsing;
mod parsing_context;
mod parsing_scope;
mod starlark_parser;
mod statement_parsing;

use crate::language::Starlark;
use crate::tree::FileKind;
use psi_builder::builder::PsiBuilderImpl;

pub(crate) fn parse_file(builder: &mut PsiBuilderImpl<Starlark>, file_kind: FileKind) {
    starlark_parser::parse(builder, file_kind);
}
