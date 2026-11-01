//! `JavaParser.kt`, and `JavaSyntaxDefinition.parse(level, builder)`: the parsers of one language level, wired.

use crate::builder::SyntaxTreeBuilder;
use crate::language_level::LanguageLevel;
use crate::node_kind::NodeKind;
use crate::parser::declaration_parser::DeclarationParser;
use crate::parser::expression_parser::ExpressionParser;
use crate::parser::file_parser::FileParser;
use crate::parser::module_parser::ModuleParser;
use crate::parser::pattern_parser::PatternParser;
use crate::parser::pratt_expression_parser::PrattExpressionParser;
use crate::parser::reference_parser::ReferenceParser;
use crate::parser::statement_parser::StatementParser;

/// `JavaParser(languageLevel)`: its parsers are views made on demand (`fileParser` is `file_parser()`), each holding
/// this parser as `myParser`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct JavaParser {
    pub language_level: LanguageLevel,
}

impl JavaParser {
    /// `JavaParser(languageLevel)`.
    pub fn new(language_level: LanguageLevel) -> Self {
        JavaParser { language_level }
    }

    /// `fileParser`: `FileParser(this)`.
    #[inline]
    pub fn file_parser(&self) -> FileParser<'_> {
        FileParser { my_parser: self }
    }

    /// `moduleParser`: `ModuleParser(this)`.
    #[inline]
    pub fn module_parser(&self) -> ModuleParser<'_> {
        ModuleParser { my_parser: self }
    }

    /// `declarationParser`: `DeclarationParser(this)`.
    #[inline]
    pub fn declaration_parser(&self) -> DeclarationParser<'_> {
        DeclarationParser { my_parser: self }
    }

    /// `statementParser`: `StatementParser(this)` (`parseCodeBlockDeep` false).
    #[inline]
    pub fn statement_parser(&self) -> StatementParser<'_> {
        StatementParser { my_parser: self, parse_code_block_deep: false }
    }

    /// `expressionParser`: `ExpressionParser(PrattExpressionParser(this))`.
    #[inline]
    pub fn expression_parser(&self) -> ExpressionParser<'_> {
        ExpressionParser { my_new_expression_parser: PrattExpressionParser { my_parser: self } }
    }

    /// `referenceParser`: `ReferenceParser(this)`.
    #[inline]
    pub fn reference_parser(&self) -> ReferenceParser<'_> {
        ReferenceParser { my_parser: self }
    }

    /// `patternParser`: `PatternParser(this)`.
    #[inline]
    pub fn pattern_parser(&self) -> PatternParser<'_> {
        PatternParser { my_parser: self }
    }
}

/// `JavaSyntaxDefinition.parse(languageLevel, builder)`: a file under a root `java.FILE`.
pub fn parse(language_level: LanguageLevel, builder: &mut SyntaxTreeBuilder<'_>) {
    let root = builder.mark();
    JavaParser::new(language_level).file_parser().parse(builder);
    root.done(builder, NodeKind::JAVA_FILE);
}
