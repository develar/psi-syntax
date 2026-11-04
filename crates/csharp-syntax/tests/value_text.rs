//! `Tree::value_text` against Roslyn's `SyntaxToken.ValueText`: the expectations of Roslyn's own lexer tests (each
//! test names its source, read at 35d9211b with `git -C ~/.cache/psi-syntax/roslyn show
//! 35d9211b841e7613c1d2f8f5af6d628ace696c4c:src/Compilers/CSharp/Test/Syntax/<path>`), and `values/` against the
//! oracle's `values` lines of it (`tools/csharp-oracle/oracle.sh values`).

mod common;

use csharp_syntax::{Entry, FileKind, NodeId, SyntaxKind, TokenKind, Tree, dump, facts};

/// `LexToken(text)` of `LexicalTests.cs`: the first token `SyntaxFactory.ParseTokens` makes of `text`, as the tree of
/// `Entry::Tokens` holds it.
fn lex_token(text: &str) -> (Tree, NodeId) {
    let tree = Tree::parse(text, FileKind { entry: Entry::Tokens, ..FileKind::default() });
    let token = tree
        .children(tree.root())
        .find(|&id| tree.token_kind(id).is_some_and(|k| !dump::is_trivia(k)))
        .expect("a token");
    (tree, token)
}

/// The kind, text and value text of the first token of `text`.
fn lexed(text: &str) -> (TokenKind, String, Option<String>) {
    let (tree, id) = lex_token(text);
    let kind = tree.token_kind(id).expect("a token");
    (kind, String::from_utf8_lossy(tree.text(id)).into_owned(), tree.value_text(id).map(str::to_owned))
}

fn assert_value(text: &str, kind: TokenKind, value: &str) {
    assert_eq!(lexed(text), (kind, text.to_string(), Some(value.to_string())), "{text:?}");
}

/// The non-trivia tokens of a file, each its text and value text, in document order.
fn tokens(text: &str) -> Vec<(String, Option<String>)> {
    let tree = Tree::parse(text, FileKind::default());
    tree.descendants(tree.root())
        .filter(|&id| tree.token_kind(id).is_some_and(|k| !dump::is_trivia(k)))
        .map(|id| (String::from_utf8_lossy(tree.text(id)).into_owned(), tree.value_text(id).map(str::to_owned)))
        .collect()
}

/// The tokens between the quotes of `<see cref='...'/>` (or `<param name='...'/>`, an element whose name attribute
/// is lexed as names) in a documentation comment, as Roslyn's `CrefLexerTests` and `NameAttributeValueLexerTests` lex
/// `text + "'"` in the attribute's mode: each its text and value text, the parser's missing tokens left out.
fn attribute_tokens(attribute: &str, value: &str) -> Vec<(String, String)> {
    let element = if attribute == "name" { "param" } else { "see" };
    let file = format!("/// <{element} {attribute}='{value}'/>\nclass C {{ }}\n");
    let all = tokens(&file);
    let open = all.iter().position(|(t, _)| t == "'").expect("an opening quote") + 1;
    let close = open + all[open..].iter().position(|(t, _)| t == "'").expect("a closing quote");
    all[open..close]
        .iter()
        .filter(|(t, _)| !t.is_empty())
        .map(|(t, v)| (t.clone(), v.clone().expect("a value text")))
        .collect()
}

fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected.iter().map(|&(t, v)| (t.to_string(), v.to_string())).collect()
}

/// LexicalAndXml/LexicalTests.cs:610-870 (`TestSingleLetterIdentifier` to `TestNonLatinIdentifier`).
#[test]
fn identifiers_as_lexical_tests_expect() {
    for text in ["a", "abc", "a0b1c2", "my\u{34F}very\u{34F}long\u{34F}identifier"] {
        assert_value(text, TokenKind::IdentifierToken, text);
    }
    let combining = format!("F{}o", "o\u{304}".repeat(3));
    assert_value(&combining, TokenKind::IdentifierToken, &combining);
    assert_value("@x", TokenKind::IdentifierToken, "x");
    assert_value("@if", TokenKind::IdentifierToken, "if");
    // `ሴ` and its forms: a value of one, three or four UTF-16 code units (U+1234 is ETHIOPIC SYLLABLE SEE)
    assert_value("\\u1234", TokenKind::IdentifierToken, "\u{1234}");
    assert_value("@\\u1234", TokenKind::IdentifierToken, "\u{1234}");
    assert_value("\\U00001234", TokenKind::IdentifierToken, "\u{1234}");
    assert_value("@\\U00001234", TokenKind::IdentifierToken, "\u{1234}");
    assert_value("a\\u1234b", TokenKind::IdentifierToken, "a\u{1234}b");
    assert_value("a\\u1234\\u5678b", TokenKind::IdentifierToken, "a\u{1234}\u{5678}b");
    assert_value("\u{C6}sh", TokenKind::IdentifierToken, "\u{C6}sh");
    // `TestBadUnicodeEscapeIdentifier`: `\u123` is one character of value (Roslyn's `\u123` decodes as far as it
    // can), with an error
    let (kind, _, value) = lexed("\\u123");
    assert_eq!(kind, TokenKind::IdentifierToken);
    assert_eq!(value.map(|v| v.encode_utf16().count()), Some(1));
}

/// LexicalTests.cs:834 `TestAllVerbatimKeywordsAsIdentifiers`, :876 `TestAllLanguageKeywords`, :892
/// `TestAllLanguagePunctuation`.
#[test]
fn keywords_verbatim_or_not_and_punctuation() {
    for keyword in facts::get_keyword_kinds().filter(|&k| facts::is_reserved_keyword(k)) {
        let text = facts::get_text(keyword);
        assert_value(&format!("@{text}"), TokenKind::IdentifierToken, text);
        let (kind, _, value) = lexed(text);
        assert_eq!((kind.syntax_kind(), value.as_deref()), (keyword, Some(text)), "{text}");
    }
    for punctuation in [
        SyntaxKind::TildeToken,
        SyntaxKind::ExclamationToken,
        SyntaxKind::CaretEqualsToken,
        SyntaxKind::PercentEqualsToken,
        SyntaxKind::QuestionQuestionEqualsToken,
    ] {
        let text = facts::get_text(punctuation);
        let (kind, _, value) = lexed(text);
        assert_eq!((kind.syntax_kind(), value.as_deref()), (punctuation, Some(text)));
    }
}

/// LexicalTests.cs:1703 `TestVerbatimIdentifierWithMultipleAtSign1`, :1719 `...2`; Parsing/NameParsingTests.cs:776
/// `TestFormattingCharacter`, :792 `TestSoftHyphen` (a formatting character is dropped from the value).
#[test]
fn at_signs_and_formatting_characters() {
    assert_value("@@class", TokenKind::IdentifierToken, "class");
    assert_value("@@@class", TokenKind::IdentifierToken, "class");
    assert_value("\u{915}\u{94D}\u{200D}\u{937}", TokenKind::IdentifierToken, "\u{915}\u{94D}\u{937}");
    assert_value("x\u{AD}y", TokenKind::IdentifierToken, "xy");
}

/// LexicalAndXml/CrefLexerTests.cs:27-94 (`TestLexIdentifiers`, `TestLexKeywords`, `TestLexVerbatimKeywords`,
/// `TestLexUnicodeEscapeKeywords`) and :136-188 (`TestLexBraces`, the entities of `<`): Roslyn lexes `text'` in
/// `XmlCrefQuote` mode, here the same text is the cref of a parsed documentation comment.
#[test]
fn cref_tokens_as_cref_lexer_tests_expect() {
    assert_eq!(attribute_tokens("cref", "&#x61;b"), pairs(&[("&#x61;b", "ab")]));
    assert_eq!(attribute_tokens("cref", "a&#x62;"), pairs(&[("a&#x62;", "ab")]));
    assert_eq!(attribute_tokens("cref", "&#103;lobal"), pairs(&[("&#103;lobal", "global")]));
    assert_eq!(attribute_tokens("cref", "&#111;perator"), pairs(&[("&#111;perator", "operator")]));
    assert_eq!(attribute_tokens("cref", "&#116;ru&#101;"), pairs(&[("&#116;ru&#101;", "true")]));
    assert_eq!(attribute_tokens("cref", "@global"), pairs(&[("@global", "global")]));
    assert_eq!(attribute_tokens("cref", "&#64;ref"), pairs(&[("&#64;ref", "ref")]));
    assert_eq!(attribute_tokens("cref", "\\u0067lobal"), pairs(&[("\\u0067lobal", "global")]));
    assert_eq!(attribute_tokens("cref", "\\u006Fperator"), pairs(&[("\\u006Fperator", "operator")]));
    assert_eq!(attribute_tokens("cref", "A{T}"), pairs(&[("A", "A"), ("{", "<"), ("T", "T"), ("}", ">")]));
    for entity in ["&lt;", "&#60;", "&#060;", "&#x3C;", "&#123;"] {
        let cref = format!("A{entity}T>");
        assert_eq!(attribute_tokens("cref", &cref), pairs(&[("A", "A"), (entity, "<"), ("T", "T"), (">", ">")]));
    }
}

/// LexicalAndXml/NameAttributeValueLexerTests.cs:30-68: in `XmlNameQuote` mode every name is an identifier, a
/// keyword's spelling included, and `@` is kept: Parser/Lexer.cs:1641-1652 (`ScanIdentifier_CrefSlowPath`) adds it to
/// the value in a name attribute ("to match dev11") where a cref drops it as a verbatim `@`.
#[test]
fn name_attribute_tokens_as_name_lexer_tests_expect() {
    assert_eq!(attribute_tokens("name", "@a"), pairs(&[("@a", "@a")]));
    assert_eq!(attribute_tokens("name", "@\\u0070A"), pairs(&[("@\\u0070A", "@pA")]));
    assert_eq!(attribute_tokens("name", "&#x61;b"), pairs(&[("&#x61;b", "ab")]));
    assert_eq!(attribute_tokens("name", "&#111;perator"), pairs(&[("&#111;perator", "operator")]));
    assert_eq!(attribute_tokens("name", "&#114;e&#102;"), pairs(&[("&#114;e&#102;", "ref")]));
}

/// LexicalAndXml/PreprocessorTests.cs:194-218 (`#define`'s and `#undef`'s `Name.ValueText`, `#load`'s and `#r`'s
/// `File.ValueText`): a directive's identifier and string have their values, a string literal elsewhere has none
/// (the port keeps no literal's value).
#[test]
fn directive_values_and_the_literals_without_one() {
    let file = "#define F\\u004F\n#line 1 \"a.cs\"\n#pragma warning disable CS0168\nclass C { string s = \"x\"; int n = 0x10; char c = 'c'; }\n";
    let values = tokens(file);
    let value = |text: &str| values.iter().find(|(t, _)| t == text).map(|(_, v)| v.clone()).expect(text);
    assert_eq!(value("F\\u004F"), Some("FO".to_string()));
    assert_eq!(value("\"a.cs\""), Some("a.cs".to_string()));
    assert_eq!(value("CS0168"), Some("CS0168".to_string()));
    assert_eq!(value("\"x\""), None);
    assert_eq!(value("0x10"), None);
    assert_eq!(value("'c'"), None);
    assert_eq!(value("class"), Some("class".to_string()));
    // a directive's number has none either (Roslyn's `ValueText` of `010` is `10`, its value)
    let values = tokens("#line 010 \"x.cs\"\n#pragma warning disable 0168\nclass C { }\n");
    let value = |text: &str| values.iter().find(|(t, _)| t == text).map(|(_, v)| v.clone()).expect(text);
    assert_eq!(value("010"), None);
    assert_eq!(value("0168"), None);
    assert_eq!(value("\"x.cs\""), Some("x.cs".to_string()));
}

/// A directive in the trailing trivia of a token with a value (not first on its line) has values of its own: the
/// token keeps its value (a lexer fuzz input gave `@c` the value of `#r "a"`'s string before this was fixed).
#[test]
fn a_directive_in_trailing_trivia_leaves_the_token_value() {
    let values = tokens("123@c#r \"a\"\nclass L { v\u{FEFF}oid #line 1 \"f\"\n }\n");
    assert!(values.contains(&("@c".to_string(), Some("c".to_string()))), "{values:?}");
    assert!(values.contains(&("v\u{FEFF}oid".to_string(), Some("void".to_string()))), "{values:?}");
}

/// Nodes, trivia and missing tokens: `None` for the first two, the empty text for the last, a missing literal's
/// included (`MissingTokenWithTrivia.Text`; Roslyn's `DirectiveParser` makes missing strings and numbers:
/// Parser/DirectiveParser.cs:384, 442, 513, 533, 615, 643-654).
#[test]
fn nodes_trivia_and_missing_tokens() {
    let text = "#pragma checksum\n#line (1,1)-(2,2) 3\n#line\nclass C { int x = ; // c\n}";
    let tree = Tree::parse(text, FileKind::default());
    assert_eq!(tree.value_text(tree.root()), None);
    let mut missing = Vec::new();
    for id in tree.descendants(tree.root()) {
        match tree.token_kind(id) {
            None => assert_eq!(tree.value_text(id), None),
            Some(kind) if dump::is_trivia(kind) => assert_eq!(tree.value_text(id), None),
            Some(kind) if tree.is_missing(id) => {
                assert_eq!(tree.value_text(id), Some(""), "{kind:?}");
                missing.push(kind);
            }
            Some(TokenKind::NumericLiteralToken) => assert_eq!(tree.value_text(id), None),
            Some(_) => assert_eq!(tree.value_text(id).map(str::as_bytes), Some(tree.text(id))),
        }
    }
    for kind in [TokenKind::StringLiteralToken, TokenKind::NumericLiteralToken, TokenKind::IdentifierToken] {
        assert!(missing.contains(&kind), "a missing {kind:?} in {missing:?}");
    }
}

/// `values/NAME.cs` against `NAME.values.txt`, what `tools/csharp-oracle/oracle.sh values --kept` printed for it:
/// the lines of the tokens whose value the port keeps and differs from the text, exactly `dump::value_lines`'
/// (`ksdump --values`), a directive's strings included.
#[test]
fn value_lines_equal_the_oracle() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/values");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("tests/values") {
        let path = entry.expect("an entry").path();
        if path.extension().is_none_or(|e| e != "cs") {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).expect("a name").to_string();
        let text = std::fs::read(&path).expect("the input");
        let oracle = std::fs::read_to_string(path.with_extension("values.txt")).expect("the oracle's lines");
        let ours = dump::value_lines(&name, &Tree::parse(&text, FileKind::default()));
        assert_eq!(ours, oracle, "{name}");
        checked += oracle.lines().filter(|l| l.contains("\tStringLiteralToken\t")).count();
    }
    assert!(checked >= 4, "the directive strings of values.cs: {checked}");
}

/// The goldens of `testdata/csharp` (its `manifest.tsv`, each at its version, entry, documentation mode and preprocessor
/// symbols) against `values/goldens.values.txt`, what `tools/csharp-oracle/oracle.sh values --kept` printed for them
/// (the manifest's lines with `.cs` added to the path).
#[test]
fn golden_value_lines_equal_the_oracle() {
    let Some(ours) = common::golden_lines(dump::value_lines) else {
        println!("no testdata/csharp/manifest.tsv, nothing to compare");
        return;
    };
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/values");
    let oracle = std::fs::read_to_string(dir.join("goldens.values.txt")).expect("the oracle's lines");
    common::assert_same_lines(&ours, &oracle);
    assert!(oracle.lines().count() > 500);
}
