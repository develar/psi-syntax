//! The lexers against the goldens: for every golden, the leaf texts of the expected dump must be the texts of our
//! tokens, where a doc comment is its KDoc lexer tokens and a markdown link inside it its Kotlin lexer tokens (the
//! granularity of the PSI leaves). A leaf may join two adjacent tokens: `?.`, `?:` and `!!`, which the parser
//! collapses. Prints the match count and the first mismatch of each failure; fails with `KS_STRICT=1`.

mod common;

use kotlin_syntax::kdoc::lexer::KDocLexer;
use kotlin_syntax::lexer::KotlinLexer;
use kotlin_syntax::tokens::TokenKind;

/// The leaf texts of a dump, escaped as the dump escapes them.
fn leaf_texts(dump: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in dump.lines() {
        let content = line.trim_start_matches(' ');
        let after_name = if let Some(rest) = content.strip_prefix("PsiWhiteSpace") {
            rest
        } else if content.starts_with("PsiElement(") || content.starts_with("PsiComment(") {
            match content.find(')') {
                Some(i) => &content[i + 1..],
                None => continue,
            }
        } else {
            continue;
        };
        if let Some(text) = after_name.strip_prefix("('").and_then(|t| t.strip_suffix("')")) {
            out.push(text.to_owned());
        }
    }
    out
}

/// Our tokens at the granularity of PSI leaves, escaped; empty tokens are not leaves.
fn token_texts(text: &[u8]) -> Vec<(TokenKind, String)> {
    let mut out = Vec::new();
    kotlin_tokens(text, &mut out);
    out
}

fn kotlin_tokens(text: &[u8], out: &mut Vec<(TokenKind, String)>) {
    let mut lexer = KotlinLexer::new(text);
    while let Some(t) = lexer.advance() {
        let token_text = &text[t.start..t.end];
        if t.kind == TokenKind::DOC_COMMENT {
            let mut kdoc = KDocLexer::new(token_text);
            while let Some(k) = kdoc.advance() {
                let k_text = &token_text[k.start..k.end];
                if k.kind == TokenKind::KDOC_MARKDOWN_LINK {
                    kotlin_tokens(k_text, out);
                } else if !k_text.is_empty() {
                    out.push((k.kind, common::fix_white_spaces(k_text)));
                }
            }
        } else if !token_text.is_empty() {
            out.push((t.kind, common::fix_white_spaces(token_text)));
        }
    }
}

fn compare(expected: &[String], ours: &[(TokenKind, String)]) -> Result<(), String> {
    let (mut i, mut j) = (0, 0);
    while i < expected.len() && j < ours.len() {
        if expected[i] == ours[j].1 {
            i += 1;
            j += 1;
            continue;
        }
        let joinable = matches!(
            (ours[j].0, ours.get(j + 1).map(|t| t.0)),
            (TokenKind::QUEST, Some(TokenKind::DOT | TokenKind::COLON)) | (TokenKind::EXCL, Some(TokenKind::EXCL))
        );
        if joinable && expected[i] == format!("{}{}", ours[j].1, ours[j + 1].1) {
            i += 1;
            j += 2;
            continue;
        }
        break;
    }
    psi_check::leaves_compared(expected, ours, i, j)
}

#[test]
fn lexer_leaf_texts() {
    let goldens = common::goldens();
    let mut matched = 0;
    let mut stale = 0;
    for golden in &goldens {
        let text = common::source_text(golden);
        let expected = leaf_texts(&common::expected(golden));
        let ours = token_texts(text.as_bytes());
        match compare(&expected, &ours) {
            Ok(()) => matched += 1,
            Err(e) => {
                stale += common::is_known_stale(golden) as usize;
                println!("MISMATCH {}: {e}", golden.name);
            }
        }
    }
    println!("lexer: {matched}/{} goldens have their leaf texts ({stale} failing known stale)", goldens.len());
    if std::env::var_os("KS_STRICT").is_some_and(|v| v == "1") {
        assert_eq!(matched + stale, goldens.len());
    }
}

/// A collapsed block or lambda is reparsed from its text; the tree reuses the file's tokens for it when its `{` was
/// lexed in the plain state. Checks that lexing every such balanced `{...}` alone gives exactly those tokens, over
/// the goldens and over the files of `KS_CORPUS_LIST` (paths relative to `KS_CORPUS_ROOT`) when set.
#[test]
fn plain_blocks_lex_alone_into_the_same_tokens() {
    let mut texts: Vec<Vec<u8>> = common::goldens().iter().map(|g| common::source_text(g).into_bytes()).collect();
    if let (Some(list), Some(root)) = (std::env::var_os("KS_CORPUS_LIST"), std::env::var_os("KS_CORPUS_ROOT")) {
        let list = std::fs::read_to_string(list).expect("read KS_CORPUS_LIST");
        for path in list.lines().filter(|l| !l.is_empty()) {
            if let Ok(text) = std::fs::read(std::path::Path::new(&root).join(path)) {
                texts.push(text);
            }
        }
    }
    let mut blocks = 0;
    for text in &texts {
        let lexed = kotlin_syntax::lexer::tokenize(text);
        for first in 0..lexed.len() {
            if lexed.types[first] != TokenKind::LBRACE || !lexed.is_plain(first) {
                continue;
            }
            // advanceBalancedBlock from after the `{`
            let mut brace_count = 1;
            let mut last = first + 1;
            while last < lexed.len() {
                match lexed.types[last] {
                    TokenKind::LBRACE => brace_count += 1,
                    TokenKind::RBRACE => brace_count -= 1,
                    _ => {}
                }
                last += 1;
                if brace_count == 0 {
                    break;
                }
            }
            let (start, end) = (lexed.starts[first] as usize, lexed.starts[last] as usize);
            let alone = kotlin_syntax::lexer::tokenize(&text[start..end]);
            assert_eq!(alone.types, lexed.types[first..last], "{}", String::from_utf8_lossy(&text[start..end]));
            let shifted: Vec<u32> = lexed.starts[first..=last].iter().map(|s| s - start as u32).collect();
            assert_eq!(alone.starts, shifted);
            blocks += 1;
        }
    }
    println!("{blocks} plain blocks lex alone into the same tokens");
}
