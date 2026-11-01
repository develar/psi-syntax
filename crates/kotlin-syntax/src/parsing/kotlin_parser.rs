//! The port of `KotlinParser.java`: the entry points that wrap a `PsiBuilder` into a
//! `SemanticWhitespaceAwarePsiBuilderImpl` and run `KotlinParsing.createForTopLevel` on it. `getTreeBuilt()` is the
//! caller's (`Tree`), which builds the tree from the builder afterwards.

use crate::builder::SemanticWhitespaceAwarePsiBuilderImpl;
use crate::parsing::kotlin_parsing::KotlinParsing;
use crate::tree::FileKind;

/// `parse(psiBuilder, psiFile)`: `parseFile()` for a `.kt` file (or no extension), `parseScript()` otherwise.
pub fn parse(psi_builder: &mut SemanticWhitespaceAwarePsiBuilderImpl, file_kind: FileKind) {
    let mut kt_parsing = KotlinParsing::create_for_top_level(psi_builder.handle());
    match file_kind {
        FileKind::Kt => kt_parsing.parse_file(),
        FileKind::Kts => kt_parsing.parse_script(),
    }
}

/// `parseTypeCodeFragment(psiBuilder)`.
pub fn parse_type_code_fragment(psi_builder: &mut SemanticWhitespaceAwarePsiBuilderImpl) {
    KotlinParsing::create_for_top_level(psi_builder.handle()).parse_type_code_fragment();
}

/// `parseExpressionCodeFragment(psiBuilder)`.
pub fn parse_expression_code_fragment(psi_builder: &mut SemanticWhitespaceAwarePsiBuilderImpl) {
    KotlinParsing::create_for_top_level(psi_builder.handle()).parse_expression_code_fragment();
}

/// `parseBlockCodeFragment(psiBuilder)`.
pub fn parse_block_code_fragment(psi_builder: &mut SemanticWhitespaceAwarePsiBuilderImpl) {
    KotlinParsing::create_for_top_level(psi_builder.handle()).parse_block_code_fragment();
}

/// `parseLambdaExpression(psiBuilder)`: what `LambdaExpressionElementType.parseContents` runs on a collapsed lambda.
pub fn parse_lambda_expression(psi_builder: &mut SemanticWhitespaceAwarePsiBuilderImpl) {
    KotlinParsing::create_for_top_level(psi_builder.handle()).parse_lambda_expression();
}

/// `parseBlockExpression(psiBuilder)`: what `BlockExpressionElementType.parseContents` runs on a collapsed block.
pub fn parse_block_expression(psi_builder: &mut SemanticWhitespaceAwarePsiBuilderImpl) {
    KotlinParsing::create_for_top_level(psi_builder.handle()).parse_block_expression();
}
