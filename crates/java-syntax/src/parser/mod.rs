//! The ports of java-syntax's parsers (`com.intellij.java.syntax.parser`), one Rust file per Kotlin file, one function
//! per method in the same order (see AGENTS.md, "Porting conventions"). `JavaParser` wires them as `JavaParser.kt` does:
//! each parser is a view of its `JavaParser` and reaches the others through it (`self.my_parser.reference_parser()`).
//! One method is not ported, `JavaParserUtil.stoppingBuilder(builder, condition)`, which no parser calls: its stub says
//! `TODO(port): JavaParserUtil.kt:133` and panics with `todo!`.

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
