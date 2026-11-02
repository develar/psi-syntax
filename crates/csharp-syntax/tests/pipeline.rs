//! The pipeline end to end while the lexer and the parser are placeholders and stubs: a file parses into a compilation
//! unit whose end of file carries every token as skipped trivia, losslessly; the entries and the lexer dump run.

use csharp_syntax::dump::{lex_to_string, tree_to_string};
use csharp_syntax::{Entry, FileKind, NodeKind, Parser, Tree};

#[test]
fn a_file_parses_losslessly() {
    let text = "class C\r\n{\n  int x; // c\n}\n";
    let tree = Tree::parse(text, FileKind::default());
    assert_eq!(tree.node_kind(tree.root()), Some(NodeKind::CompilationUnit));
    assert_eq!(tree.range(tree.root()), 0..text.len());
    let leaves: Vec<u8> = tree.tokens().flat_map(|id| tree.text(id).to_vec()).collect();
    assert_eq!(leaves, text.as_bytes());
    let dump = tree_to_string(&tree);
    assert!(
        dump.starts_with(
            "CompilationUnit [0..27)\n  L:SkippedTokensTrivia [0..5)\n    IdentifierToken [0..5) 'class'\n"
        ),
        "{dump}"
    );
    assert!(dump.ends_with("  EndOfFileToken [27..27) ''\n"), "{dump}");
}

#[test]
fn a_parser_is_reused_and_every_entry_runs() {
    let mut parser = Parser::new();
    for entry in [Entry::CompilationUnit, Entry::Expression, Entry::Statement, Entry::Member, Entry::Name] {
        for text in ["", "a + b", "\u{FEFF}x\n", "@"] {
            let tree = parser.parse(text, FileKind { entry, ..FileKind::default() });
            let _ = tree_to_string(&tree);
        }
    }
    let lexed = parser.lex("a b\n", FileKind::default());
    assert_eq!(
        lex_to_string(&lexed),
        "IdentifierToken [0..1) 'a'\nT:WhitespaceTrivia [1..2) ' '\nIdentifierToken [2..3) 'b'\nT:EndOfLineTrivia [3..4) '\\n'\nEndOfFileToken [4..4) ''\n"
    );
}
