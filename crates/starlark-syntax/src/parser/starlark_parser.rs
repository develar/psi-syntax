//! `StarlarkParser`: the file entry point of the IntelliJ Starlark parser.

use crate::language::Starlark;
use crate::node_kind::NodeKind;
use crate::parser::parsing_context::ParsingContext;
use crate::tree::FileKind;
use psi_builder::builder::PsiBuilderImpl;

pub(crate) fn parse(builder: &mut PsiBuilderImpl<Starlark>, _file_kind: FileKind) {
    let file = builder.mark();
    let mut context = ParsingContext::new(builder);
    let mut after_semicolon = false;
    while !context.builder.eof() {
        context.push_scope(context.empty_parsing_scope());
        if after_semicolon {
            context.parse_simple_statement();
        } else {
            context.parse_statement();
        }
        after_semicolon = context.get_scope().is_after_semicolon;
        context.pop_scope();
    }
    file.done(context.builder, NodeKind::FILE);
}
