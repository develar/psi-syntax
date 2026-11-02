//! The pipeline end to end: a file parses losslessly into the compilation unit Roslyn makes of it (its dump is the
//! oracle's, `tools/csharp-oracle/oracle.sh dump`); the entries and the lexer dump run.

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
    assert_eq!(
        tree_to_string(&tree),
        concat!(
            "CompilationUnit [0..27)\n",
            "  ClassDeclaration [0..27)\n",
            "    ClassKeyword [0..5) 'class'\n",
            "    T:WhitespaceTrivia [5..6) ' '\n",
            "    IdentifierToken [6..7) 'C'\n",
            "    T:EndOfLineTrivia [7..9) '\\r\\n'\n",
            "    OpenBraceToken [9..10) '{'\n",
            "    T:EndOfLineTrivia [10..11) '\\n'\n",
            "    FieldDeclaration [11..25)\n",
            "      VariableDeclaration [11..18)\n",
            "        PredefinedType [11..17)\n",
            "          L:WhitespaceTrivia [11..13) '  '\n",
            "          IntKeyword [13..16) 'int'\n",
            "          T:WhitespaceTrivia [16..17) ' '\n",
            "        VariableDeclarator [17..18)\n",
            "          IdentifierToken [17..18) 'x'\n",
            "      SemicolonToken [18..19) ';'\n",
            "      T:WhitespaceTrivia [19..20) ' '\n",
            "      T:SingleLineCommentTrivia [20..24) '// c'\n",
            "      T:EndOfLineTrivia [24..25) '\\n'\n",
            "    CloseBraceToken [25..26) '}'\n",
            "    T:EndOfLineTrivia [26..27) '\\n'\n",
            "  EndOfFileToken [27..27) ''\n",
        )
    );
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
