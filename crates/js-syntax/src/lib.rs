//! TypeScript, TSX and JavaScript read by oxc's parser, the parse each consumer of psi-syntax shares: accio's extraction
//! and Branch Deck's summaries, highlighting and navigation read one arena AST of one parse of one text, and each walks
//! it its own way (decision 2026-11-08-rust-and-js-parse-entries of accio). What is the language's is oxc's typed AST;
//! this crate holds what is not: the dialect of a file (`Dialect`), the options of every parse (`options`), the text
//! the parser reads (`parse_text`), the recovery of a fatal parse (`recover`), the stack a parse and a walk of its tree
//! take (`stack_needed`) and the thread's arena (`with_program`). `tsconfig` reads a project's configuration, the
//! path aliases its imports resolve through, a file format of the language every consumer reads alike.
//!
//! The `oxc_*` crates are re-exported, so a consumer names the AST's types through this crate and builds with its pins.

pub mod recover;
pub mod tsconfig;

pub use oxc_allocator;
pub use oxc_ast;
pub use oxc_ast_visit;
pub use oxc_diagnostics;
pub use oxc_parser;
pub use oxc_span;
pub use oxc_syntax;

use std::borrow::Cow;
use std::cell::RefCell;

use oxc_allocator::Allocator;
use oxc_ast::ast::Program;
use oxc_diagnostics::OxcDiagnostic;
use oxc_parser::config::TokensParserConfig;
use oxc_parser::{ParseOptions, Parser, ParserReturn};
use oxc_span::SourceType;

thread_local! {
    static ALLOCATOR: RefCell<Allocator> = RefCell::new(Allocator::default());
}

/// How oxc reads a file: TypeScript or JavaScript, with JSX or without. Module or script is decided by the file
/// (`Unambiguous`: an `import` or an `export` makes it a module); what the extension adds beside the dialect (`.mts`,
/// `.cts`, `.cjs`) is nothing a walk reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dialect {
    /// `.ts`, `.mts`, `.cts`: without JSX, where `<T>x` is a type assertion
    TypeScript,
    /// `.tsx`
    Tsx,
    /// `.js`, `.mjs`, `.cjs`, `.jsx`: JSX in `.js` too, as tree-sitter-javascript read it (oxc's `SourceType::from_path`
    /// turns it off for `.js`)
    JavaScript,
}

impl Dialect {
    /// The dialect of a file by its extension, without its dot; None for a file oxc does not read.
    pub fn of_extension(extension: &str) -> Option<Self> {
        Some(match extension {
            "ts" | "mts" | "cts" => Self::TypeScript,
            "tsx" => Self::Tsx,
            "js" | "mjs" | "cjs" | "jsx" => Self::JavaScript,
            _ => return None,
        })
    }

    pub fn source_type(self) -> SourceType {
        let (typescript, jsx) = match self {
            Self::TypeScript => (true, false),
            Self::Tsx => (true, true),
            Self::JavaScript => (false, true),
        };
        SourceType::unambiguous().with_typescript(typescript).with_jsx(jsx)
    }
}

/// The options of every parse: parentheses kept (`ParenthesizedExpression`, as tree-sitter's
/// `parenthesized_expression`: a chain that opens with one is no chain, `extract::chain_of`), a top-level `return`
/// accepted (CommonJS bodies and scripts tree-sitter parses without an error), no identifier hashes (no semantic
/// analysis runs on the tree).
pub fn options() -> ParseOptions {
    ParseOptions {
        preserve_parens: true,
        allow_return_outside_function: true,
        enable_ident_hashes: false,
        ..ParseOptions::default()
    }
}

/// `src` as the text oxc reads: itself when it is UTF-8, else a copy whose invalid bytes are spaces, one for one, so
/// every span of the tree is an offset into `src` as well (tree-sitter parsed such a file by its bytes; a Latin-1
/// comment or string must not cost the file its declarations). Names are cut from `src` itself, where such a byte
/// makes the name no UTF-8 and no name (`Source::text`), as tree-sitter's `text` did.
pub fn parse_text(src: &[u8]) -> Cow<'_, str> {
    match std::str::from_utf8(src) {
        Ok(text) => Cow::Borrowed(text),
        Err(_) => {
            let mut bytes = src.to_vec();
            let mut at = 0;
            while let Err(e) = std::str::from_utf8(&bytes[at..]) {
                let bad = at + e.valid_up_to();
                let len = e.error_len().unwrap_or(bytes.len() - bad);
                bytes[bad..bad + len].fill(b' ');
                at = bad + len;
            }
            Cow::Owned(String::from_utf8(bytes).expect("every invalid sequence was blanked"))
        }
    }
}

/// The messages of oxc's syntax errors (`oxc_parser::diagnostics`), whole or as a prefix (`…*`): an unexpected or
/// a missing token, an unterminated literal or comment, a character or number no token takes, a merge marker, JSX
/// whose tags do not close, a file oxc reads as Flow or binary.
const SYNTAX: &[&str] = &[
    "Unexpected token",
    "Unexpected token.*",
    "Unexpected end of file",
    "Unexpected JSX expression",
    "Expected `*",
    "Expected a semicolon or an implicit semicolon after a statement, but found none",
    "Expected function name",
    "Expected function body",
    "Expected switch clause",
    "Expected corresponding*",
    "Expected 'with' in import type options",
    "Identifier expected.",
    "Identifier expected after '?.'",
    "Missing catch or finally clause",
    "Adjacent JSX elements must be wrapped in an enclosing tag.",
    "Unterminated*",
    "Invalid Character*",
    "Invalid characters after number",
    "Invalid Number*",
    "Line terminator not permitted before arrow",
    "Empty parenthesized expression",
    "Encountered diff marker",
    "Flow is not supported",
    "File appears to be binary.",
    "String literal expected.",
    "'extends' list cannot be empty.",
    "Type parameter list cannot be empty.",
    "Type argument list cannot be empty.",
    "Trailing comma not allowed.",
];

/// Whether `d` says the text does not follow the grammar (`SYNTAX`), not an early error: oxc's parser also reports a
/// valid parse that breaks a rule of the language beyond its grammar (strict mode, a modifier where it may not stand, a
/// duplicate export, an escape in a keyword), after which the tree is whole. Every consumer reads a file's errors by
/// this: accio's syntax errors, Branch Deck's error bytes and `in_error`.
pub fn is_syntax_error(d: &OxcDiagnostic) -> bool {
    let m = d.message.as_ref();
    SYNTAX.iter().any(|s| match s.strip_suffix('*') {
        Some(prefix) => m.starts_with(prefix),
        None => m == *s,
    })
}

/// What one `with_program` asks of the parse.
#[derive(Debug, Clone, Copy)]
pub struct Read {
    /// A fatal parse is parsed again with the text at its error blanked (`recover::blank`): the declarations around an
    /// error of a file being edited stay. Off for the syntax errors of a file, the first parse's diagnostics.
    pub recover: bool,
    /// The tokens of the parse, in source order (`ParserReturn::tokens`), for a reader that colours a file or writes a
    /// declaration's text by its tokens; empty after a fatal parse.
    pub tokens: bool,
    /// The clock the time of the parses is read on, returned beside the answer: a consumer's tallies, which a test
    /// may replace.
    pub clock: fn() -> u64,
}

impl Default for Read {
    fn default() -> Self {
        Self { recover: true, tokens: false, clock: || 0 }
    }
}

/// One parse as a walk reads it: oxc's result, and whether it is a recovery's (`recover`), the first parse having
/// been fatal. A recovered tree's spans are offsets into the file as every other's.
pub struct Parsed<'a> {
    ret: ParserReturn<'a>,
    /// The first parse was fatal and this tree is a later one's (or, after the last round, still fatal).
    pub recovered: bool,
}

impl<'a> std::ops::Deref for Parsed<'a> {
    type Target = ParserReturn<'a>;

    fn deref(&self) -> &ParserReturn<'a> {
        &self.ret
    }
}

impl<'a> Parsed<'a> {
    /// The program when it came out of the parse whole: what a walk reads, empty after a fatal error.
    pub fn program(&self) -> Option<&Program<'a>> {
        (!self.ret.fatal_error).then_some(&self.ret.program)
    }
}

/// Parse `src` as `dialect` and hand the result to `read`, with the time the parses took on `how.clock`; None when
/// the file nests deeper than `MAX_STACK` holds. The tree lives as long as `read` runs.
///
/// oxc's parser and the walks over its tree recurse once per level of nesting, about 2 KiB of stack each (`{"k":`
/// nested 2 000 times overflowed a 2 MiB thread): a file whose nesting `stack_needed` bounds within
/// `psi_builder::stack::STACK` parses on the calling thread and its arena (`ALLOCATOR`), reset per file; a deeper one
/// on a thread of its own with the stack it needs (`psi_builder::stack::on_own_stack`).
pub fn with_program<R: Send>(
    dialect: Dialect,
    src: &[u8],
    how: Read,
    read: impl FnOnce(&Parsed<'_>) -> R + Send,
) -> Option<(R, u64)> {
    let source_type = dialect.source_type();
    let text = parse_text(src);
    let needed = stack_needed(src);
    if needed <= psi_builder::stack::STACK {
        Some(ALLOCATOR.with(|cell| {
            let mut allocator = cell.borrow_mut();
            allocator.reset();
            let (parsed, took) = parse(&allocator, &text, source_type, how);
            (read(&parsed), took)
        }))
    } else if needed <= MAX_STACK {
        let text = &*text;
        psi_builder::stack::on_own_stack(needed, move || {
            let allocator = Allocator::default();
            let (parsed, took) = parse(&allocator, text, source_type, how);
            (read(&parsed), took)
        })
    } else {
        None
    }
}

/// The parses after a fatal one: each blanks one more region of the text (`recover::blank`), so a file with that many
/// separate errors still has a tree; a file that is no source of the language stops after them.
const RECOVERY_ROUNDS: usize = 16;

/// Parse `text`, and when `how.recover` and the parse is fatal, again: a JavaScript file oxc reads as Flow (a `@flow`
/// pragma and its type syntax) as TypeScript with JSX, whose syntax Flow's annotations mostly are; any other with the
/// region of its fatal error blanked (`recover::blank`), up to `RECOVERY_ROUNDS` times. With the time the parses took
/// on `how.clock`.
fn parse<'a>(allocator: &'a Allocator, text: &'a str, source_type: SourceType, how: Read) -> (Parsed<'a>, u64) {
    let (mut ret, mut took) = oxc_parse(allocator, text, source_type, how);
    if !how.recover || !ret.fatal_error {
        return (Parsed { ret, recovered: false }, took);
    }
    let mut source_type = source_type;
    let mut blanked: Option<Vec<u8>> = None;
    for _ in 0..RECOVERY_ROUNDS {
        if ret.is_flow_language && !source_type.is_typescript() {
            source_type = source_type.with_typescript(true).with_jsx(true);
        } else {
            let bytes = blanked.get_or_insert_with(|| text.as_bytes().to_vec());
            let at = fatal_offset(&ret, bytes.len());
            if !recover::blank(bytes, at) {
                break;
            }
        }
        let text = match &blanked {
            Some(b) => allocator.alloc_str(std::str::from_utf8(b).expect("blanking replaces whole characters")),
            None => text,
        };
        let (next, t) = oxc_parse(allocator, text, source_type, how);
        took += t;
        ret = next;
        if !ret.fatal_error {
            break;
        }
    }
    (Parsed { ret, recovered: true }, took)
}

/// Where a fatal parse stopped: the furthest offset a diagnostic names (the fatal error is the parse's last, a lexer
/// error at most one token past it), the end of the text when none names one.
fn fatal_offset(ret: &ParserReturn, len: usize) -> usize {
    ret.diagnostics
        .iter()
        .filter_map(|d| d.labels.iter().map(|l| l.offset()).max())
        .max()
        .map_or(len, |at| (at as usize).min(len))
}

/// The deepest file parsed: nesting over 500 000 levels is no source a reader writes.
const MAX_STACK: usize = 1 << 30;

/// An upper bound of the stack the parse of `src` and a walk over its tree take, over a base of 256 KiB: the most
/// that the open brackets and the tokens of the statements open around one point cost. A bracket (outside strings,
/// comments and regular expressions, a template's `${` one) costs 4 KiB; a token that may nest the parse or a walk
/// without a bracket about twice what a level of it was measured to take (a file nesting it alone, every fact read,
/// on the release build: `function(){ return ` 2.9 KiB, `{"k":` 1.8, `` `${ `` 1.8, `B<` 1.6, `x => ` 0.95, `keyof `
/// 0.58, `a = ` 0.56, `l: ` 0.48, `if (x) ` 0.4, `new ` 0.37, `!`, `-`, `typeof `, `await ` 0.27): `<` 3 KiB, `=>`
/// 2 KiB, `=` 1.25 KiB, `?`, `:` and a word that nests (`NESTING_WORDS`) 1 KiB, `!`, `~`, `-`, `+` 640 bytes, `.`
/// 320. A `;` or a `,` ends what its bracket level's tokens cost (a statement, an element: the parse returned from
/// them), a closing bracket the level; a statement that holds a statement (`HOLDING_WORDS`, 1 KiB) costs until its
/// level closes. Per `.` of the longest run of them no `;`, `,`, `=` or brace ends, 2 KiB more: a call chain
/// (`x.f(a).g(b)…`) nests a call and a member per link, which the walks enter one frame each (a 3 000-link chain
/// overflowed the 1.2 MiB the dots alone gave). A binary chain (`a + b + …`) nests nothing: oxc parses it in a loop
/// and the walk visits it so.
///
/// Overcounting costs a thread spawned for the parse (13 of idea-1's 54 579 files); undercounting overflows the
/// stack, which aborts the process and with it the build of the whole index. Where the scan cannot tell code from
/// text it reads code: a quote that does not close on its line opens no string, a `/` after an operand divides.
pub fn stack_needed(src: &[u8]) -> usize {
    const BRACKET: usize = 4096;
    // What each open bracket level's tokens cost since its last `;` or `,` (the top level first), and their sum.
    let mut levels: Vec<usize> = vec![0];
    // What each level's statements that hold a statement cost (`if`, `else`, a loop): a `;` ends the statement they
    // hold, not them (`if (a) b; else if (c) d; else …` nests an `if` per `else`).
    let mut sticky: Vec<usize> = vec![0];
    let mut open_cost = 0usize;
    let mut most = 0usize;
    // Dots since the last `;`, `,`, `=` or brace, and the most of them: the longest chain the file may write.
    let (mut run, mut longest_run) = (0usize, 0usize);
    // Template literals open around `${ … }`: the bracket depth their `${` raised, innermost last.
    let mut templates: Vec<usize> = Vec::new();
    // Whether the last token ends an operand: a `/` after one divides, else it opens a regular expression.
    let mut after_operand = false;
    let mut i = 0;
    let open = |levels: &mut Vec<usize>, sticky: &mut Vec<usize>, most: &mut usize, open_cost: usize| {
        levels.push(0);
        sticky.push(0);
        *most = (*most).max(open_cost + levels.len() * BRACKET);
    };
    let close = |levels: &mut Vec<usize>, sticky: &mut Vec<usize>, open_cost: &mut usize| {
        if levels.len() > 1 {
            *open_cost -= levels.pop().unwrap_or(0) + sticky.pop().unwrap_or(0);
        }
    };
    while i < src.len() {
        let b = src[i];
        let mut cost = 0;
        let mut operand = false;
        match b {
            b'"' | b'\'' => {
                i = recover::string_end(src, i);
                operand = true;
            }
            b'/' if src.get(i + 1) == Some(&b'/') => i = memchr::memchr(b'\n', &src[i..]).map_or(src.len(), |n| i + n),
            b'/' if src.get(i + 1) == Some(&b'*') => {
                i = memchr::memmem::find(&src[i + 2..], b"*/").map_or(src.len(), |n| i + 2 + n + 1)
            }
            b'/' if !after_operand => {
                i = recover::regex_end(src, i);
                operand = true;
            }
            b'`' => {
                (i, operand) = (template_text(src, i + 1, &mut templates, &mut levels, &mut sticky), true);
                most = most.max(open_cost + levels.len() * BRACKET);
            }
            b'}' if templates.last() == Some(&levels.len()) => {
                templates.pop();
                close(&mut levels, &mut sticky, &mut open_cost);
                (i, operand) = (template_text(src, i + 1, &mut templates, &mut levels, &mut sticky), true);
                most = most.max(open_cost + levels.len() * BRACKET);
            }
            b'(' | b'[' | b'{' => {
                open(&mut levels, &mut sticky, &mut most, open_cost);
                if b == b'{' {
                    run = 0;
                }
            }
            b')' | b']' | b'}' => {
                close(&mut levels, &mut sticky, &mut open_cost);
                operand = b != b'}';
                if b == b'}' {
                    run = 0;
                }
            }
            b';' | b',' => {
                open_cost -= std::mem::take(levels.last_mut().expect("the top level stays"));
                run = 0;
            }
            b'<' => cost = 3072,
            b'?' | b':' => cost = 1024,
            b'!' | b'~' | b'-' | b'+' => cost = 640,
            b'=' if src.get(i + 1) == Some(&b'>') => {
                cost = 2048;
                i += 1;
                run = 0;
            }
            b'=' => {
                cost = 1280;
                run = 0;
            }
            b'.' => {
                cost = 320;
                run += 1;
                longest_run = longest_run.max(run);
            }
            c if recover::is_word(c) => {
                let end = i + src[i..].iter().take_while(|&&c| recover::is_word(c)).count();
                let word = &src[i..end];
                if HOLDING_WORDS.contains(&word) {
                    *sticky.last_mut().expect("the top level stays") += 1024;
                    open_cost += 1024;
                    most = most.max(open_cost + levels.len() * BRACKET);
                } else if NESTING_WORDS.contains(&word) {
                    cost = 1024;
                } else {
                    operand = !REGEX_AFTER.contains(&word);
                }
                i = end - 1;
            }
            _ => operand = after_operand && b.is_ascii_whitespace(),
        }
        if cost > 0 {
            *levels.last_mut().expect("the top level stays") += cost;
            open_cost += cost;
            most = most.max(open_cost + levels.len() * BRACKET);
        }
        after_operand = operand;
        i += 1;
    }
    (256 << 10) + most + longest_run * 2048
}

/// The words that may nest the parse or a walk one level each without a bracket: prefix operators (`typeof`, `void`,
/// `delete`, `await`, `new`, `yield`), a nested function and its `return`; TypeScript's type operators (`keyof`, `infer`, `readonly`, `unique`, a
/// conditional type's `extends`).
const NESTING_WORDS: &[&[u8]] = &[
    b"typeof",
    b"void",
    b"delete",
    b"await",
    b"new",
    b"yield",
    b"keyof",
    b"infer",
    b"readonly",
    b"unique",
    b"extends",
    b"function",
    b"return",
];

/// The words of a statement that holds a statement, nesting it one level: a `;` ends the statement held, not them.
const HOLDING_WORDS: &[&[u8]] = &[b"if", b"else", b"do", b"while", b"for", b"with"];

/// The words after which a `/` opens a regular expression, not a division (`return /x/`).
const REGEX_AFTER: &[&[u8]] = &[
    b"return",
    b"typeof",
    b"void",
    b"delete",
    b"await",
    b"yield",
    b"new",
    b"in",
    b"of",
    b"case",
    b"do",
    b"else",
    b"instanceof",
];

/// Skip a template literal's text from `i` to its closing backtick (the returned offset) or into a `${`, which opens
/// a bracket level until its `}` (`stack_needed`).
fn template_text(
    src: &[u8],
    mut i: usize,
    templates: &mut Vec<usize>,
    levels: &mut Vec<usize>,
    sticky: &mut Vec<usize>,
) -> usize {
    while i < src.len() {
        match src[i] {
            b'\\' => i += 1,
            b'`' => return i,
            b'$' if src.get(i + 1) == Some(&b'{') => {
                levels.push(0);
                sticky.push(0);
                templates.push(levels.len());
                return i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    i
}

/// The one oxc parse call, with the time it took on `how.clock`: a consumer counts a file's parses as one.
#[allow(clippy::disallowed_methods)] // the one oxc parse call (clippy.toml)
fn oxc_parse<'a>(
    allocator: &'a Allocator,
    text: &'a str,
    source_type: SourceType,
    how: Read,
) -> (ParserReturn<'a>, u64) {
    let started = (how.clock)();
    let parser = Parser::new(allocator, text, source_type).with_options(options());
    let parsed = if how.tokens { parser.with_config(TokensParserConfig).parse() } else { parser.parse() };
    (parsed, (how.clock)().saturating_sub(started))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn javascript_reads_jsx_and_the_arena_is_reused() {
        let read = Read::default();
        let jsx = b"export const App = () => <div className=\"a\">{x}</div>;\n";
        let ((fatal, errors, statements), _) = with_program(Dialect::JavaScript, jsx, read, |p| {
            (p.fatal_error, p.diagnostics.has_errors(), p.program.body.len())
        })
        .unwrap();
        assert!(!fatal && !errors && statements == 1);
        // A CommonJS module's top-level `return` is no error, as in tree-sitter.
        assert!(
            with_program(Dialect::JavaScript, b"if (done) return;\nmodule.exports = f;\n", read, |p| !p
                .diagnostics
                .has_errors())
            .unwrap()
            .0
        );
        assert!(with_program(Dialect::TypeScript, b"let x = <T>y;\n", read, |p| p.program().is_some()).unwrap().0);
        // A byte that is no UTF-8 is a space to the parser, at its own offset.
        let latin1 = b"const s = \"\xe9t\xe9\"; function f() {}";
        assert_eq!(
            with_program(Dialect::TypeScript, latin1, read, |p| (p.diagnostics.has_errors(), p.program.body.len()))
                .unwrap()
                .0,
            (false, 2)
        );
    }

    /// Tokens come only when asked for, in source order; a fatal parse is parsed again around its error.
    #[test]
    fn tokens_are_read_when_asked_and_a_fatal_parse_recovers() {
        let src = b"const a = 1\nlet b = a\n";
        let tokens = Read { tokens: true, ..Read::default() };
        assert_eq!(with_program(Dialect::TypeScript, src, Read::default(), |p| p.tokens.len()).unwrap().0, 0);
        let kinds =
            with_program(Dialect::TypeScript, src, tokens, |p| p.tokens.iter().map(|it| it.kind()).collect::<Vec<_>>())
                .unwrap()
                .0;
        assert_eq!(kinds.first(), Some(&oxc_parser::Kind::Const));
        let broken = b"function f() {}\nclass {{{\nfunction g() {}\n";
        let (recovered, functions) = with_program(Dialect::TypeScript, broken, Read::default(), |p| {
            (p.recovered, p.program().map_or(0, |it| it.body.len()))
        })
        .unwrap()
        .0;
        assert!(recovered && functions >= 2, "{functions}");
        let first = Read { recover: false, ..Read::default() };
        assert!(with_program(Dialect::TypeScript, broken, first, |p| p.program().is_none()).unwrap().0);
    }

    /// A syntax error is one the grammar rejects; an early error leaves a whole tree.
    #[test]
    fn an_early_error_is_no_syntax_error() {
        let kinds = |src: &[u8]| {
            with_program(Dialect::TypeScript, src, Read { recover: false, ..Read::default() }, |p| {
                p.diagnostics.iter().map(is_syntax_error).collect::<Vec<_>>()
            })
            .unwrap()
            .0
        };
        assert_eq!(kinds(b"let a = (;\n"), [true]);
        // a modifier where it may not stand, an implementation in an ambient context
        let early = kinds(b"public function f() {}\nclass A { declare m() {} }\n");
        assert!(!early.is_empty() && early.iter().all(|it| !it), "{early:?}");
    }

    #[test]
    fn the_dialect_is_the_extension() {
        assert_eq!(
            ["ts", "mts", "cts", "tsx", "js", "mjs", "cjs", "jsx", "rs"].map(Dialect::of_extension),
            [
                Some(Dialect::TypeScript),
                Some(Dialect::TypeScript),
                Some(Dialect::TypeScript),
                Some(Dialect::Tsx),
                Some(Dialect::JavaScript),
                Some(Dialect::JavaScript),
                Some(Dialect::JavaScript),
                Some(Dialect::JavaScript),
                None
            ]
        );
    }
}
