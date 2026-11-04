//! `preprocessor::activity` against the parser itself: the flags it replays from a tree parsed with no symbol are the
//! flags of a parse with the symbols, wherever both trees hold the same directives.

use csharp_syntax::preprocessor::activity;
use csharp_syntax::{DirectiveFlags, FileKind, NodeKind, Parser, PreprocessorSymbols, TokenKind, Tree};

/// Each directive of `tree`: its kind, its range and its flags.
fn directives(tree: &Tree) -> Vec<(Option<NodeKind>, std::ops::Range<usize>, DirectiveFlags)> {
    tree.directives().map(|(id, f)| (tree.node_kind(id), tree.range(id), f)).collect()
}

/// Every name of an `#if` or `#elif` condition of `tree`, sorted, once each.
fn condition_names(tree: &Tree) -> Vec<String> {
    let mut names: Vec<String> = tree
        .directives()
        .filter(|&(id, _)| matches!(tree.node_kind(id), Some(NodeKind::IfDirectiveTrivia | NodeKind::ElifDirectiveTrivia)))
        .flat_map(|(id, _)| tree.descendants(id).filter(|&l| tree.token_kind(l) == Some(TokenKind::IdentifierToken)).filter_map(|l| tree.value_text(l)).map(str::to_string).collect::<Vec<_>>())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// The outcome of one file under one set of symbols.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// The replay is the parse's.
    Same,
    /// The parse holds other directives (a branch taken whose text spans a `#` line inside a string or a comment).
    OtherDirectives,
    /// The replay differs from the parse though both hold the same directives: a bug.
    Differs(String),
}

fn check(parser: &mut Parser, text: &[u8], none: &Tree, symbols: &[String]) -> Outcome {
    let with = parser.parse_with_symbols(text, FileKind::default(), &PreprocessorSymbols::new(symbols));
    let (replayed, parsed) = (activity(none, &PreprocessorSymbols::new(symbols)), directives(&with));
    let ours = directives(none);
    if ours.len() != parsed.len() || ours.iter().zip(&parsed).any(|(a, b)| a.0 != b.0 && !(a.0 == Some(NodeKind::BadDirectiveTrivia) || b.0 == Some(NodeKind::BadDirectiveTrivia)) || a.1 != b.1) {
        return Outcome::OtherDirectives;
    }
    for (i, (flags, (kind, range, expected))) in replayed.iter().zip(&parsed).enumerate() {
        if flags != expected {
            return Outcome::Differs(format!("directive {i} {kind:?} {range:?}: replayed {flags:?}, parsed {expected:?} with {symbols:?}"));
        }
    }
    Outcome::Same
}

#[test]
fn a_replay_is_the_parse_with_the_symbols() {
    let cases = [
        "#if A\nclass X {}\n#elif B\nclass Y {}\n#else\nclass Z {}\n#endif\n",
        "#if A && !B\n#define C\n#endif\n#if C || (A == B)\nclass X {}\n#endif\n",
        "#undef A\n#if A\nclass X {}\n#endif\n#region r\n#if B\n#region s\n#endregion\n#endif\n#endregion\n",
        "#if A\n#if B\n#elif C\n#else\n#endif\n#elif D\n#if E\n#endif\n#endif\n",
        "#elif X\n#else\n#endif\n#endregion\n#if true\n#elif false\n#endif\n#if A != false\n#endif\n",
        "#if A\n#region open\n#else\n#region other\n#endif\n#endregion\n",
    ];
    let mut parser = Parser::new();
    for text in cases {
        let none = parser.parse(text, FileKind::default());
        let names = condition_names(&none);
        for mask in 0..(1u32 << names.len()) {
            let symbols: Vec<String> = names.iter().enumerate().filter(|(i, _)| mask >> i & 1 == 1).map(|(_, n)| n.clone()).collect();
            assert_eq!(check(&mut parser, text.as_bytes(), &none, &symbols), Outcome::Same, "{text}");
        }
    }
}

/// Every `.cs` file with an `#if` under the directories `CSHARP_PREPROCESSOR_CORPUS` names (`:`-separated, the
/// corpora of `tools/csharp-oracle`): with every name of its conditions defined and with a few subsets of them.
#[test]
#[ignore = "a corpus run: CSHARP_PREPROCESSOR_CORPUS=DIR[:DIR…]"]
fn a_replay_is_the_parse_on_the_corpora() {
    let roots = std::env::var("CSHARP_PREPROCESSOR_CORPUS").expect("CSHARP_PREPROCESSOR_CORPUS");
    let mut stack: Vec<std::path::PathBuf> = roots.split(':').map(Into::into).collect();
    let mut parser = Parser::new();
    let (mut runs, mut other, mut differing) = (0usize, 0usize, Vec::new());
    while let Some(path) = stack.pop() {
        if path.is_dir() {
            stack.extend(std::fs::read_dir(&path).into_iter().flatten().flatten().map(|e| e.path()));
            continue;
        }
        if path.extension().is_none_or(|e| e != "cs") {
            continue;
        }
        let Ok(text) = std::fs::read(&path) else { continue };
        if !text.windows(3).any(|w| w == b"#if") {
            continue;
        }
        let none = parser.parse(&text, FileKind::default());
        let names = condition_names(&none);
        if names.is_empty() {
            continue;
        }
        let mut sets = vec![names.clone()];
        let mut seed = text.len() as u64 | 1;
        for _ in 0..3 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            sets.push(names.iter().enumerate().filter(|(i, _)| seed >> (i % 64) & 1 == 1).map(|(_, n)| n.clone()).collect());
        }
        for symbols in sets {
            runs += 1;
            match check(&mut parser, &text, &none, &symbols) {
                Outcome::Same => {}
                Outcome::OtherDirectives => other += 1,
                Outcome::Differs(why) => differing.push(format!("{}: {why}", path.display())),
            }
        }
    }
    assert!(runs > 0 && differing.is_empty(), "{runs} runs, {other} with other directives, {} differing: {:#?}", differing.len(), &differing[..differing.len().min(20)]);
    eprintln!("{runs} runs, {other} with other directives");
}
