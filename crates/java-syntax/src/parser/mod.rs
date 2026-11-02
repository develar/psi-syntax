//! The ports of java-syntax's parsers (`com.intellij.java.syntax.parser`), one Rust file per Kotlin file, one function
//! per method in the same order (see AGENTS.md, "Porting conventions"). `JavaParser` wires them as `JavaParser.kt` does:
//! each parser is a view of its `JavaParser` and reaches the others through it (`self.my_parser.reference_parser()`).
//! One method is not ported, `JavaParserUtil.stoppingBuilder(builder, condition)` (JavaParserUtil.kt:133), which
//! nothing calls; `java_parser_util.rs` says why where it would be.
//!
//! Not in the Kotlin: every recursion cycle of the parsers passes a function that counts a level of nesting
//! (`SyntaxTreeBuilder::enter_nesting`; `Java`'s `NESTING_LIMIT`, `TYPE_ARGUMENTS_LIMIT`, `REPARSED_LIMIT`): an
//! expression (`tryParseWithPrecedenceAtMost`, a unary operand), a statement (`parseStatement`), a declaration
//! (`DeclarationParser.parse`), an array initializer, a type (`parseTypeInfo`), a type argument list, an annotation,
//! a pattern (`parseTypeOrRecordPattern`) and an inline doc tag (`parseDataItem`). Past a limit the construct is the
//! error element "Code is too deeply nested" over the rest of it (`java_parser_util::too_deeply_nested`).

pub mod declaration_parser;
pub mod expression_parser;
pub mod file_parser;
pub mod java_doc_parser;
pub mod java_keywords;
pub mod java_parser;
pub mod java_parser_util;
pub mod jshell_parser;
pub mod module_parser;
pub mod pattern_parser;
pub mod pratt_expression_parser;
pub mod reference_parser;
pub mod statement_parser;
