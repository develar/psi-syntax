//! The place of the Java port (not yet started): a lossless Java lexer and parser whose dump equals PSI's byte for
//! byte, ported from IntelliJ's `community/java/java-syntax` (`com.intellij.java.syntax`: `_JavaLexer.flex`,
//! `_JavaDocLexer.flex`, `JavaParser` with `FileParser`, `DeclarationParser`, `StatementParser`, `ExpressionParser`,
//! `PrattExpressionParser`, `ReferenceParser`, `PatternParser`, `ModuleParser`, `JavaDocParser`, the element types
//! of `JavaSyntaxElementType`, `JavaSyntaxTokenType`, `JavaDocSyntaxElementType`) at IntelliJ
//! 3d034e2526507767a19528888066e47ad2898bb1, on the builder of `community/platform/syntax` at the same revision.
//! It implements `psi_builder::Language` as `kotlin-syntax` does.
