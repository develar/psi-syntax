//! The Java tree: `psi_builder`'s preorder column tree and parse driver over Java's types (`Tree`, `Parser`), how a
//! file is parsed (`FileKind`: its language level and the parser entry) and what each lazily parsed element is
//! reparsed with (`LazyKind`).

use crate::Java;
use crate::language_level::LanguageLevel;

pub use psi_builder::tree::{Children, NodeId, ParseOptions, Preorder};

/// A parsed Java file.
pub type Tree = psi_builder::Tree<Java>;

/// A reusable Java parser: owns the buffers every builder of a parse works in, cleared and reused from one file to the
/// next. Keep one per thread; `Tree::parse` makes a fresh one per call.
pub type Parser = psi_builder::Parser<Java>;

/// How a file is parsed: at a language level (the lexer's keywords, the parsers' features, the binders' markdown
/// support), through a parser entry.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct FileKind {
    pub level: LanguageLevel,
    pub entry: Entry,
}

impl FileKind {
    /// A `.java` file at `level` (`JavaSyntaxDefinition.parse(level, builder)`).
    pub fn file(level: LanguageLevel) -> FileKind {
        FileKind { level, entry: Entry::File }
    }
}

impl Default for FileKind {
    /// A file at `LanguageLevel.HIGHEST`, as `JavaSyntaxDefinition.parse(builder)` parses it.
    fn default() -> Self {
        FileKind::file(LanguageLevel::HIGHEST)
    }
}

/// The parser entry a text is parsed with: a file (`JavaSyntaxDefinition.parse`, root `java.FILE`), or one of the
/// entries the parser-partial goldens run (`JavaParsingTestConfiguratorBase`: root `test.java.file`, then
/// `root = mark(); ENTRY; if (!eof) { error "Unparsed tokens" over the rest }; root.done(...)`).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Entry {
    /// `JavaSyntaxDefinition.parse(level, builder)`.
    File,
    /// `DeclarationParser.parseAnnotations(builder)`.
    Annotations,
    /// `DeclarationParser.parseClassBodyWithBraces(builder, false, false)`.
    ClassBody,
    /// `DeclarationParser.parseClassBodyWithBraces(builder, true, false)`.
    ClassBodyAnnotation,
    /// `DeclarationParser.parseClassBodyWithBraces(builder, false, true)`.
    ClassBodyEnum,
    /// `ExpressionParser.parse(builder)`.
    Expression,
    /// `FileParser.parse(builder)`.
    FileParser,
    /// `PatternParser.parsePattern(builder)` (when `PatternParser.isPattern(builder)`).
    Pattern,
    /// `ReferenceParser.parseJavaCodeReference(builder, false, false, false, false)`.
    Reference,
    /// `ReferenceParser.parseJavaCodeReference(builder, true, false, false, false)`.
    ReferenceIncomplete,
    /// `ReferenceParser.parseType(builder, ELLIPSIS | DIAMONDS | DISJUNCTIONS)`.
    Type,
    /// `ReferenceParser.parseTypeParameters(builder)`.
    TypeParams,
    /// `StatementParser.parseCodeBlockDeep(builder, true)`.
    CodeBlockDeep,
    /// `StatementParser.parseStatements(builder)`.
    Statements,
}

impl Entry {
    /// The entry of a manifest or oracle list line (`file`, `annotations`, `class_body`, ...).
    pub fn from_name(name: &str) -> Option<Entry> {
        Some(match name {
            "file" => Entry::File,
            "annotations" => Entry::Annotations,
            "class_body" => Entry::ClassBody,
            "class_body_annotation" => Entry::ClassBodyAnnotation,
            "class_body_enum" => Entry::ClassBodyEnum,
            "expression" => Entry::Expression,
            "file_parser" => Entry::FileParser,
            "pattern" => Entry::Pattern,
            "reference" => Entry::Reference,
            "reference_incomplete" => Entry::ReferenceIncomplete,
            "type" => Entry::Type,
            "type_params" => Entry::TypeParams,
            "code_block_deep" => Entry::CodeBlockDeep,
            "statements" => Entry::Statements,
            _ => return None,
        })
    }
}

/// What a lazily parsed element is reparsed with when the tree expands it.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum LazyKind {
    /// `JavaSyntaxElementType.CODE_BLOCK` (`JavaCodeBlockParser`): `StatementParser.parseCodeBlockDeep(builder, true)`
    /// over its tokens.
    CodeBlock,
    /// `JavaDocSyntaxElementType.DOC_COMMENT` (`JavaDocCommentParser(false)`): `JavaDocLexer` and
    /// `JavaDocParser.parseDocCommentText()`.
    DocComment,
    /// `JavaDocSyntaxElementType.DOC_MARKDOWN_COMMENT` (`JavaDocCommentParser(true)`).
    DocMarkdownComment,
    /// `JavaDocSyntaxElementType.DOC_REFERENCE_HOLDER` (`JavaDocReferenceHolderParser`): `JavaLexer(JDK_1_3)` and
    /// `JavaDocParser.parseJavadocReference`.
    DocReferenceHolder,
    /// `JavaDocSyntaxElementType.DOC_TYPE_HOLDER` (`JavaDocTypeParser`): `JavaTypeEscapeLexer(JavaLexer(JDK_1_3))` and
    /// `JavaDocParser.parseJavadocType`.
    DocTypeHolder,
}

// A tree is shared across threads as is, a parser moves to the thread that keeps it.
const _: () = {
    const fn send_sync<T: Send + Sync + 'static>() {}
    const fn send<T: Send + 'static>() {}
    send_sync::<Tree>();
    send::<Parser>();
};
