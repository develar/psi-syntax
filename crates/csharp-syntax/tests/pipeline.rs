//! The pipeline end to end: a file parses losslessly into the compilation unit Roslyn makes of it (its dump is the
//! oracle's, `tools/csharp-oracle/oracle.sh dump`); the entries and the lexer dump run.

use csharp_syntax::dump::{lex_to_string, tree_to_string};
use csharp_syntax::{Entry, FileKind, NodeKind, Parser, PreprocessorSymbols, TokenKind, Tree};

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

/// The names of a tree's class declarations, in document order.
fn class_names(tree: &Tree) -> Vec<String> {
    tree.descendants(tree.root())
        .filter(|&id| tree.node_kind(id) == Some(NodeKind::ClassDeclaration))
        .filter_map(|id| tree.children(id).find(|&c| tree.token_kind(c) == Some(TokenKind::IdentifierToken)))
        .map(|id| tree.utf8_text(id).unwrap_or_default().to_string())
        .collect()
}

/// The preprocessor symbols of the options (`parse_with_symbols`, `CSharpParseOptions.WithPreprocessorSymbols`): an
/// `#if` reads a name as defined when the options hold it and the file's own `#define`s and `#undef`s before it say
/// nothing of it; what they say wins. The inputs and Roslyn's dumps of them are `tests/oracle/symbol_*` (in
/// `manifest.tsv` with the same symbols); the active classes below are the ones those dumps show.
#[test]
fn preprocessor_symbols_from_the_options() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle");
    let mut parser = Parser::new();
    for (name, symbols, with, without) in [
        // `#if A` with A defined by the options: Yes, No disabled; without them the other branch.
        ("symbol_defined_by_option", &["A"][..], &["Yes"][..], &["No"][..]),
        // `#undef A` before `#if A` wins over the options.
        ("symbol_undefined_by_the_file", &["A"], &["No"], &["No"]),
        // `#define B` and `#undef C` win over the options' C: `A && B && !C` holds only with the options' A.
        ("symbol_defined_by_the_file", &["A", "C"], &["Yes"], &[]),
        // The name is the identifier's value: `A\u0042` is `AB`.
        ("symbol_with_unicode_escape", &["AB"], &["Yes"], &["No"]),
    ] {
        let text = std::fs::read(dir.join(format!("{name}.cs"))).expect("the input");
        let oracle = std::fs::read_to_string(dir.join(format!("{name}.tree.txt"))).expect("Roslyn's dump");
        let tree = parser.parse_with_symbols(&text, FileKind::default(), &PreprocessorSymbols::new(symbols));
        assert_eq!(tree_to_string(&tree), oracle, "{name}");
        assert_eq!(class_names(&tree), with, "{name} with {symbols:?}");
        assert_eq!(class_names(&parser.parse(&text, FileKind::default())), without, "{name} without symbols");
        let lexed = parser.lex_with_symbols(&text, FileKind::default(), &PreprocessorSymbols::new(symbols));
        let disabled = |tree: &Tree| {
            tree.descendants(tree.root())
                .filter(|&id| tree.token_kind(id) == Some(TokenKind::DisabledTextTrivia))
                .count()
        };
        assert_eq!(disabled(&lexed), disabled(&tree), "{name}: the lexer reads the same symbols");
    }
}

/// A directive's boolean fields (`Tree::directives`, `DirectiveTriviaSyntax.IsActive`, `BranchTaken`,
/// `ConditionValue`), as Roslyn's lexer sets them for the symbols of the parse: a directive inside a branch not taken is
/// inactive, its condition unevaluated; the tree of a parse and of a lex hold the same.
#[test]
fn a_directive_keeps_its_boolean_fields() {
    let text = "#if A\nclass X {}\n#region r\n#endregion\n#elif !A\nclass Y {}\n#if B\n#endif\n#else\nclass Z {}\n#endif\n";
    let mut parser = Parser::new();
    let row = |tree: &Tree| -> Vec<String> {
        tree.directives()
            .map(|(id, f)| {
                let kind = format!("{:?}", tree.node_kind(id).expect("a node"));
                let flag = |on: bool, c: char| if on { c } else { '-' };
                format!("{} {}{}{}", kind.trim_end_matches("DirectiveTrivia"), flag(f.is_active(), 'a'), flag(f.branch_taken(), 't'), flag(f.condition_value(), 'v'))
            })
            .collect()
    };
    let none = parser.parse(text, FileKind::default());
    assert_eq!(row(&none), ["If a--", "Region ---", "EndRegion ---", "Elif atv", "If a--", "EndIf a--", "Else a--", "EndIf a--"]);
    let a = parser.parse_with_symbols(text, FileKind::default(), &PreprocessorSymbols::new(["A"]));
    assert_eq!(row(&a), ["If atv", "Region a--", "EndRegion a--", "Elif a--", "If ---", "EndIf ---", "Else a--", "EndIf a--"]);
    assert_eq!(row(&parser.lex_with_symbols(text, FileKind::default(), &PreprocessorSymbols::new(["A"]))), row(&a));
    for (id, flags) in a.directives() {
        assert_eq!(a.directive_flags(id), Some(flags));
    }
    assert_eq!(a.directive_flags(a.root()), None);
}
