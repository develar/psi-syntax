//! How deep rust-analyzer's parser recurses into a file, bounded before it does. Its grammar is recursive descent
//! without a depth limit: each nested block, parenthesis, prefix operator, closure, `else if` or generic argument is a
//! few frames more (about a kilobyte of stack per level in a release build), and a stack overflow aborts the process,
//! which no `catch_unwind` recovers. A worker thread has 2 MiB (accio's build workers, Branch Deck's blocking threads), so one file
//! nested two thousand deep, generated or from a fuzz corpus, ended every build of its repository. `levels` over-approximates the depth from the tokens alone;
//! `run` parses a shallow file on the caller's stack, a deeper one on a thread with a stack sized for it, and gives up
//! on a file deeper than any real code (`MAX_LEVELS`).

use ra_ap_rustc_lexer::{FrontmatterAllowed, TokenKind as T, tokenize};

/// The stack a level may take: about four times the most a release build measured, 1.16 KiB a level of nested blocks
/// (parentheses 0.9, generic types 0.65, prefix operators 0.4, `else if` 0.26: `levels` counts several tokens of some
/// shapes as levels), and more for a debug build's larger frames.
#[cfg(not(debug_assertions))]
const BYTES_PER_LEVEL: usize = 4 * 1024;
#[cfg(debug_assertions)]
const BYTES_PER_LEVEL: usize = 32 * 1024;

/// The levels a file may have and still be parsed on the caller's stack: those of `psi_builder::stack::STACK`, half a
/// worker's 2 MiB, the caller having used some before it parses (256 in a release build; 0.15% of 90 000 files
/// of published crates count more, a table or a long chain of calls).
pub const INLINE_LEVELS: usize = psi_builder::stack::STACK / BYTES_PER_LEVEL;

/// The deepest file parsed at all: past it no code is written by hand, and its thread would reserve 512 MiB of stack
/// in a release build (the deepest of the published crates counts 4 133, unicode-normalization's tables).
pub const MAX_LEVELS: usize = 128 * 1024;

/// Runs `work` (a parse of the file of `text` and what reads its tree) where the stack holds its recursion: here when
/// the file is shallow, else on a thread with a stack for its `levels` (`psi_builder::stack::on_own_stack`). None when the file
/// is deeper than `MAX_LEVELS` or the thread cannot be made: no answer, which the caller reports as a file it cannot
/// parse.
pub fn run<R: Send>(text: &str, work: impl FnOnce() -> R + Send) -> Option<R> {
    let levels = levels(text);
    if levels <= INLINE_LEVELS {
        return Some(work());
    }
    if levels > MAX_LEVELS {
        return None;
    }
    psi_builder::stack::on_own_stack((levels * BYTES_PER_LEVEL).next_multiple_of(1 << 20) + (1 << 20), work)
}

/// The state of one bracket level of `levels`: what came before it at the level around it, and what its own tokens
/// have run to.
#[derive(Default)]
struct Level {
    /// The run of the level around this one when this one opened, its bracket included.
    outer: usize,
    /// The tokens of this level since the last that ends a nesting (`;`, `,` outside the regions below).
    run: usize,
    /// `<` opened at this level and not closed: a generic argument list (`HashMap<K, HashMap<K, V>>`), whose commas
    /// do not end the nesting; a comparison's `<` is counted too, which only overcounts.
    angles: usize,
    /// Inside a closure's parameter list (`|a, b|`), whose commas do not end the nesting of closures in closures.
    params: bool,
}

/// An upper bound on the levels the parser recurses through over `text`, from its tokens: the brackets open around a
/// token plus, at each level, the tokens since the last `;` or `,` there (a prefix operator, a closure, an `else if`,
/// a right-associative operator is a level more than the token before it), a `}` ending the run unless an `else`, an
/// operator or a cast continues the expression after it. Commas inside generic arguments and closure parameters do
/// not end a run. It overcounts long expressions (a chain of a thousand calls is a thousand levels, which a thread
/// of their stack parses); it never undercounts what the grammar nests.
pub fn levels(text: &str) -> usize {
    let mut stack: Vec<Level> = Vec::new();
    let mut cur = Level::default();
    // the sum of `outer` over `stack`: the levels the open brackets and what led to them hold
    let mut held = 0usize;
    let mut max = 0usize;
    let mut at = 0usize;
    // the previous significant token, with the offset after it, and whether a `}` just closed
    let mut prev: Option<(T, usize, usize)> = None;
    let mut after_brace = false;
    for token in tokenize(text, FrontmatterAllowed::No) {
        let (start, end) = (at, at + token.len as usize);
        at = end;
        let kind = token.kind;
        if matches!(kind, T::Whitespace | T::LineComment { .. } | T::BlockComment { .. } | T::Frontmatter { .. }) {
            continue;
        }
        let word = &text[start..end];
        if after_brace {
            after_brace = false;
            // a new statement or item after a block ends what the block was part of; `else`, `as` and an operator
            // continue it (`if a {} else if b {}`, `{a} = {b} = …`)
            let continues = match kind {
                T::Ident => matches!(word, "else" | "as"),
                T::RawIdent | T::Literal { .. } | T::Lifetime { .. } | T::Pound => false,
                _ => true,
            };
            if !continues {
                cur.run = 0;
                cur.angles = 0;
                cur.params = false;
            }
        }
        match kind {
            T::OpenParen | T::OpenBrace | T::OpenBracket => {
                let outer = cur.run + 1;
                held += outer;
                stack.push(std::mem::replace(&mut cur, Level { outer, ..Level::default() }));
            }
            T::CloseParen | T::CloseBracket | T::CloseBrace => {
                // back to the level around, whose run now holds this bracket's group
                let inner_outer = cur.outer;
                cur = stack.pop().unwrap_or_default();
                held = held.saturating_sub(inner_outer);
                cur.run = inner_outer;
                after_brace = kind == T::CloseBrace;
            }
            T::Semi => {
                cur.run = 0;
                cur.angles = 0;
                cur.params = false;
            }
            T::Comma if cur.angles == 0 && !cur.params => cur.run = 0,
            _ => {
                cur.run += 1;
                match kind {
                    T::Lt => cur.angles += 1,
                    // the `>` of `->` and `=>` closes no generic argument list
                    T::Gt
                        if cur.angles > 0
                            && !prev.is_some_and(|(k, _, e)| e == start && matches!(k, T::Minus | T::Eq)) =>
                    {
                        cur.angles -= 1
                    }
                    T::Or if cur.params => cur.params = false,
                    // a `|` where an operand is expected opens a closure's parameters; `||` has none
                    T::Or
                        if !prev.is_some_and(|(k, _, e)| k == T::Or && e == start)
                            && expects_operand(prev, text)
                            && !text[end..].starts_with('|') =>
                    {
                        cur.params = true
                    }
                    _ => {}
                }
            }
        }
        max = max.max(held + cur.run);
        prev = Some((kind, start, end));
    }
    max
}

/// Does an operand come next after `prev`: at the start, after an operator, an opening bracket or a separator, after a
/// keyword that takes an expression (`return |x| x`, `move |x|`)? Not after a name, a literal or a closing bracket,
/// where a `|` is a bitwise or or an or-pattern.
fn expects_operand(prev: Option<(T, usize, usize)>, text: &str) -> bool {
    match prev {
        None => true,
        Some((T::Ident, start, end)) => matches!(
            &text[start..end],
            "return" | "move" | "break" | "yield" | "in" | "async" | "static" | "else" | "become"
        ),
        Some((
            T::RawIdent
            | T::Literal { .. }
            | T::Lifetime { .. }
            | T::CloseParen
            | T::CloseBracket
            | T::CloseBrace
            | T::Question,
            _,
            _,
        )) => false,
        Some(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every shape the grammar recurses through counts a level per nesting, whatever separates them; a flat file of
    /// many items, statements, arguments and arms stays shallow.
    #[test]
    fn levels_bound_every_nesting() {
        let deep = |prefix: &str, suffix: &str| {
            levels(&format!("fn f() {{ let x = {}1{}; }}", prefix.repeat(1000), suffix.repeat(1000)))
        };
        for (prefix, suffix) in [
            ("{ ", " }"),
            ("(", ")"),
            ("- ", ""),
            ("|a, b| ", ""),
            ("|| ", ""),
            ("return ", ""),
            ("a = ", ""),
            ("g(1, ", ")"),
            ("S { a: 1, b: ", " }"),
        ] {
            assert!(deep(prefix, suffix) >= 1000, "{prefix:?}: {}", deep(prefix, suffix));
        }
        let chain = format!("fn f() {{ if a {{ 1 }}{} }}", " else if a { 1 }".repeat(1000));
        assert!(levels(&chain) >= 1000);
        let generic = format!("fn f(x: {}u8{}) {{}}", "HashMap<K, ".repeat(1000), ">".repeat(1000));
        assert!(levels(&generic) >= 1000);
        let flat = format!(
            "{}\nfn g() {{ {} }}\nconst T: [u8; 3000] = [{}];\nfn h(x: u8) -> u8 {{ match x {{ {} _ => 0 }} }}\n",
            "fn a(x: u8, y: Vec<u8>) -> bool { x < 1 }\nstruct S { a: u8 }\n".repeat(1000),
            "f(x, y);\n".repeat(1000),
            "0, ".repeat(3000),
            "1 => 1,\n".repeat(1000)
        );
        assert!(levels(&flat) < 32, "{}", levels(&flat));
    }

    /// A file deeper than any code is not parsed at all; one deeper than the stack in place holds is parsed, walked
    /// and dropped on a thread of its own (20 000 blocks aborted a 2 MiB worker in the drop of its tree).
    #[test]
    fn a_deep_file_is_parsed_on_a_stack_for_it() {
        let blocks = format!("fn f() {}1{}\nfn g() {{}}\n", "{ ".repeat(20_000), " }".repeat(20_000));
        let items = std::thread::Builder::new()
            .stack_size(2 << 20)
            .spawn(move || crate::guarded(blocks.as_bytes(), || crate::parse(blocks.as_bytes()).children().count()))
            .unwrap()
            .join()
            .unwrap();
        assert_eq!(items, Some(2));
        let too_deep = "{".repeat(MAX_LEVELS + 1);
        assert_eq!(run(&too_deep, || ()), None);
    }
}
