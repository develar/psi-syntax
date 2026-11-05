//! `java-oracle mutate`: one-place mutations of the Java goldens for the PSI differential, after psi-tools'
//! `tools/psi-gate/mutate.js`.
//!
//! Each operator applies to every input, not to hand-picked cases, so a gap shows wherever the operator's construct
//! is. A mutant changes one place of its original (`--per-file N` places per operator and file, picked by a generator
//! seeded with the seed, the file and the operator), so a gap is not hidden behind an error the file has elsewhere.
//! The inputs are the PSI-clean goldens of `manifest.tsv` (no `PsiErrorElement` in `NAME.txt`), whole files and
//! parser-partial fragments alike, each copied under `original/` as the test parses it (trimmed where the test trims).
//! `OUT/origins.json` maps every mutant to its original and `OUT/list` is an oracle list:
//! `PATH<TAB>LEVEL<TAB>ENTRY<TAB>0`, the golden's level and entry, no trimming. Every text is valid UTF-8.
//!
//! Texts are code points and every draw is [`PyRandom`]'s, as the Python script that first wrote the corpus drew
//! them, so its cached oracle dumps stay the dumps of these files. Its token classes are Python's: `\w` is read as
//! Rust's alphanumeric, `\d` as numeric, `isspace` as White_Space and U+001C..U+001F; the three differ from Python's on
//! a few non-ASCII marks, symbols and numbers, of which the goldens hold none (their one non-ASCII character is
//! U+2019).
use anyhow::{Context, Result};
use ide_oracle::fuzz::Edit;
use ide_oracle::random::PyRandom;
use std::collections::BTreeMap;
use std::path::Path;

const KEYWORDS: [&str; 53] = [
    "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char", "class", "const", "continue", "default", "do",
    "double", "else", "enum", "extends", "final", "finally", "float", "for", "goto", "if", "implements", "import",
    "instanceof", "int", "interface", "long", "native", "new", "package", "private", "protected", "public", "return",
    "short", "static", "strictfp", "super", "switch", "synchronized", "this", "throw", "throws", "transient", "try", "void",
    "volatile", "while", "true", "false", "null",
];
/// Contextual words of java-syntax (`JavaKeywords.kt` and the module/record/sealed/yield/when parsing).
const SOFT: [&str; 19] = [
    "var", "yield", "record", "sealed", "permits", "non-sealed", "when", "module", "open", "opens", "requires", "exports",
    "uses", "provides", "transitive", "to", "with", "value", "_",
];
const OPERATORS: [&str; 23] =
    [">>>=", "<<=", ">>=", "...", "->", "::", "++", "--", "&&", "||", "==", "!=", "<=", ">=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<"];
const PIECES: [&str; 85] = [
    "(", ")", "{", "}", "[", "]", "<", ">", ",", ":", ";", "=", "->", ".", "::", "?", "@", "@A ", "*", "&&", "!", "...",
    "instanceof ", "new ", "class ", "interface ", "enum ", "record ", "@interface ", "if ", "else ", "for ", "while ",
    "do ", "try ", "catch ", "finally ", "switch ", "case ", "default ", "yield ", "return ", "throw ", "break ",
    "continue ", "assert ", "synchronized ", "this", "super", "null", "var ", "final ", "static ", "public ", "sealed ",
    "non-sealed ", "permits ", "extends ", "implements ", "throws ", "import ", "package ", "module ", "when ", "x", "1",
    "1L", "0x", "1e", "\"", "\"\"\"", "'", "<T>", "<?>", "(int a)", "() -> {}", "int ", "void ", "int[] ", "#", "\\", "/**",
    "/*", "//", "\n",
];
const CHAR_CONTENTS: [&str; 9] = ["\\q", "\\u0000", "ab", "", "a\nb", "\\", "'", "\\u000a", "\\777"];
const DECLARATION_KEYWORDS: [&str; 8] = ["class", "interface", "enum", "record", "void", "int", "boolean", "long"];
const MODIFIER_RUNS: [(&str, &str); 3] = [
    ("private-twice", "private private "),
    ("annotations", "@A @B(x) @C.D({1, 2}) "),
    (
        "all",
        "public protected private abstract static final transient volatile synchronized native strictfp default sealed non-sealed ",
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Word,
    Number,
    Str,
    Char,
    Punct,
}

struct Token {
    kind: Kind,
    start: usize,
    end: usize,
    text: Vec<char>,
    line: usize,
}

impl Token {
    fn is(&self, s: &str) -> bool {
        self.text.iter().copied().eq(s.chars())
    }

    fn in_set(&self, set: &[&str]) -> bool {
        set.iter().any(|s| self.is(s))
    }

    fn is_name(&self) -> bool {
        self.kind == Kind::Word && !self.in_set(&KEYWORDS)
    }

    fn opaque(&self) -> bool {
        matches!(self.kind, Kind::Str | Kind::Char)
    }
}

fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn is_decimal(c: char) -> bool {
    c.is_ascii_digit() || (!c.is_ascii() && c.is_numeric())
}

fn starts_with(text: &[char], i: usize, s: &str) -> bool {
    (i..).zip(s.chars()).all(|(k, c)| text.get(k) == Some(&c))
}

fn find(text: &[char], from: usize, s: &str) -> Option<usize> {
    (from..text.len()).find(|&i| starts_with(text, i, s))
}

/// `[^\W\d]\w*|[$_][\w$]*` at `i`: the end of the word.
fn word_at(text: &[char], i: usize) -> Option<usize> {
    let run = |from: usize, dollar: bool| from + text[from..].iter().take_while(|&&c| is_word(c) || (dollar && c == '$')).count();
    let c = text[i];
    if is_word(c) && !is_decimal(c) {
        Some(run(i + 1, false))
    } else if c == '$' || c == '_' {
        Some(run(i + 1, true))
    } else {
        None
    }
}

/// `[0-9](?:[0-9a-zA-Z_]|\.(?=[0-9]))*|\.[0-9][0-9a-zA-Z_]*` at `i`: the end of the number.
fn number_at(text: &[char], i: usize) -> Option<usize> {
    let part = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let digit = |k: usize| text.get(k).is_some_and(char::is_ascii_digit);
    if digit(i) {
        let mut j = i + 1;
        while j < text.len() && (part(text[j]) || (text[j] == '.' && digit(j + 1))) {
            j += 1;
        }
        Some(j)
    } else if text[i] == '.' && digit(i + 1) {
        Some(i + 2 + text[i + 2..].iter().take_while(|&&c| part(c)).count())
    } else {
        None
    }
}

/// Tokens with offsets and lines; strings, text blocks, characters and comments are opaque.
fn tokenize(text: &[char]) -> Vec<Token> {
    let n = text.len();
    let mut tokens = Vec::new();
    let (mut i, mut line, mut counted) = (0, 1, 0);
    let mut push = |kind: Kind, start: usize, end: usize| {
        line += text[counted..start].iter().filter(|&&c| c == '\n').count();
        counted = start;
        tokens.push(Token { kind, start, end, text: text[start..end].to_vec(), line });
    };
    while i < n {
        let c = text[i];
        if is_space(c) {
            i += 1;
            continue;
        }
        if starts_with(text, i, "//") {
            while i < n && text[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if starts_with(text, i, "/*") {
            i = find(text, i + 2, "*/").map_or(n, |end| end + 2);
            continue;
        }
        let start = i;
        if starts_with(text, i, "\"\"\"") {
            i += 3;
            while i < n && !starts_with(text, i, "\"\"\"") {
                i += if text[i] == '\\' { 2 } else { 1 };
            }
            i = n.min(i + 3);
            push(Kind::Str, start, i);
            continue;
        }
        if c == '"' || c == '\'' {
            i += 1;
            while i < n && text[i] != c && text[i] != '\n' {
                i += if text[i] == '\\' { 2 } else { 1 };
            }
            i = n.min(i + 1);
            push(if c == '"' { Kind::Str } else { Kind::Char }, start, i);
            continue;
        }
        if let Some(end) = word_at(text, i) {
            i = end;
            push(Kind::Word, start, i);
            continue;
        }
        if let Some(end) = number_at(text, i) {
            i = end;
            push(Kind::Number, start, i);
            continue;
        }
        i += OPERATORS.iter().find(|o| starts_with(text, i, o)).map_or(1, |o| o.len());
        push(Kind::Punct, start, i);
    }
    tokens
}

fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

fn insert(text: &[char], at: usize, piece: &str) -> Vec<char> {
    Edit::Insert { at, piece: &chars(piece) }.apply(text)
}

fn replace(text: &[char], at: usize, end: usize, piece: &[char]) -> Vec<char> {
    Edit::Replace { at, end, piece }.apply(text)
}

/// The operators in their order: a name and its sites (token indices).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    CommentBetweenTokens,
    DeleteToken,
    DuplicateToken,
    SwapTokens,
    Truncate,
    KeywordSwap,
    NameToKeyword,
    DoubleSemi,
    ModifierRun,
    BadName,
    CharContent,
    StringContent,
    DropBody,
    InsertPiece,
}

const OPS: [Op; 14] = [
    Op::CommentBetweenTokens,
    Op::DeleteToken,
    Op::DuplicateToken,
    Op::SwapTokens,
    Op::Truncate,
    Op::KeywordSwap,
    Op::NameToKeyword,
    Op::DoubleSemi,
    Op::ModifierRun,
    Op::BadName,
    Op::CharContent,
    Op::StringContent,
    Op::DropBody,
    Op::InsertPiece,
];

impl Op {
    fn name(self) -> &'static str {
        match self {
            Op::CommentBetweenTokens => "comment-between-tokens",
            Op::DeleteToken => "delete-token",
            Op::DuplicateToken => "duplicate-token",
            Op::SwapTokens => "swap-tokens",
            Op::Truncate => "truncate",
            Op::KeywordSwap => "keyword-swap",
            Op::NameToKeyword => "name-to-keyword",
            Op::DoubleSemi => "double-semi",
            Op::ModifierRun => "modifier-run",
            Op::BadName => "bad-name",
            Op::CharContent => "char-content",
            Op::StringContent => "string-content",
            Op::DropBody => "drop-body",
            Op::InsertPiece => "insert-piece",
        }
    }

    fn applies(self, tokens: &[Token], k: usize) -> bool {
        let t = &tokens[k];
        let next = tokens.get(k + 1);
        match self {
            Op::CommentBetweenTokens => next.is_some() && !t.opaque(),
            Op::DeleteToken | Op::DuplicateToken | Op::InsertPiece => true,
            Op::SwapTokens => next.is_some_and(|n| n.text != t.text),
            Op::Truncate => next.is_some(),
            Op::KeywordSwap => t.in_set(&KEYWORDS),
            Op::NameToKeyword | Op::BadName => t.is_name(),
            Op::DoubleSemi => t.is(";"),
            Op::ModifierRun => t.in_set(&DECLARATION_KEYWORDS) && (k == 0 || tokens[k - 1].line != t.line),
            Op::CharContent => t.kind == Kind::Char,
            Op::StringContent => t.kind == Kind::Str && t.text.len() >= 2,
            Op::DropBody => method_body(tokens, k).is_some(),
        }
    }

    /// The mutants of site `k`: a variant name (empty for the operator's only one) and the mutated text.
    fn apply(self, text: &[char], tokens: &[Token], k: usize, rng: &mut PyRandom) -> Vec<(String, Vec<char>)> {
        let t = &tokens[k];
        let one = |mutated: Vec<char>| vec![(String::new(), mutated)];
        match self {
            Op::CommentBetweenTokens => one(insert(text, t.end, "/*c*/")),
            Op::DeleteToken => one(Edit::Delete { at: t.start, end: t.end }.apply(text)),
            Op::DuplicateToken => {
                let mut piece = vec![' '];
                piece.extend_from_slice(&t.text);
                one(Edit::Insert { at: t.end, piece: &piece }.apply(text))
            }
            Op::SwapTokens => {
                let n = &tokens[k + 1];
                let mut out = text[..t.start].to_vec();
                out.extend_from_slice(&n.text);
                out.extend_from_slice(&text[t.end..n.start]);
                out.extend_from_slice(&t.text);
                out.extend_from_slice(&text[n.end..]);
                one(out)
            }
            Op::Truncate => one(Edit::<char>::Truncate { at: t.end }.apply(text)),
            Op::KeywordSwap => one(replace(text, t.start, t.end, &chars(rng.choice(&KEYWORDS)))),
            Op::NameToKeyword => {
                let mut out: Vec<(String, Vec<char>)> = rng
                    .sample(SOFT.len(), 4)
                    .into_iter()
                    .map(|i| (SOFT[i].to_owned(), replace(text, t.start, t.end, &chars(SOFT[i]))))
                    .collect();
                out.push(("hard".into(), replace(text, t.start, t.end, &chars(rng.choice(&KEYWORDS)))));
                out
            }
            Op::DoubleSemi => [("", ";"), ("comment", "/*c*/;"), ("space", " ;")]
                .iter()
                .map(|(v, more)| (v.to_string(), insert(text, t.end, more)))
                .collect(),
            Op::ModifierRun => MODIFIER_RUNS.iter().map(|(v, run)| (v.to_string(), insert(text, t.start, run))).collect(),
            Op::BadName => {
                let name = &t.text;
                let mut emoji = name.clone();
                emoji.push('\u{1F4A9}');
                let mut digit = vec!['\u{661}'];
                digit.extend_from_slice(name);
                let mut escape = chars(&format!("\\u{:04x}", u32::from(name[0])));
                escape.extend_from_slice(&name[1..]);
                [("emoji-suffix", emoji), ("digit-start", digit), ("unicode-escape", escape)]
                    .into_iter()
                    .map(|(v, n)| (v.to_owned(), replace(text, t.start, t.end, &n)))
                    .collect()
            }
            Op::CharContent => CHAR_CONTENTS
                .iter()
                .enumerate()
                .map(|(i, c)| (i.to_string(), replace(text, t.start, t.end, &chars(&format!("'{c}'")))))
                .collect(),
            Op::StringContent => {
                let s = &t.text;
                let (last, init) = s.split_last().expect("two characters at least");
                let half = s.len() / 2;
                let unclosed = init.to_vec();
                let broken = [&s[..half], &['\n'], &s[half..]].concat();
                let bad_escape = [init, &['\\', 'q'], &[*last]].concat();
                let block_open = [&['"', '"'], &s[..], &['"', '"']].concat();
                [("unclosed", unclosed), ("break", broken), ("bad-escape", bad_escape), ("block-open", block_open)]
                    .into_iter()
                    .map(|(v, n)| (v.to_owned(), replace(text, t.start, t.end, &n)))
                    .collect()
            }
            Op::DropBody => {
                let (open, close) = method_body(tokens, k).expect("a site of drop-body");
                [("semicolon", ";"), ("none", "")]
                    .iter()
                    .map(|(v, r)| (v.to_string(), replace(text, tokens[open].start, tokens[close].end, &chars(r))))
                    .collect()
            }
            Op::InsertPiece => {
                let piece = rng.choice(&PIECES);
                let piece = if piece.ends_with([' ', '\n']) { piece.to_string() } else { format!("{piece} ") };
                one(insert(text, t.start, &piece))
            }
        }
    }
}

fn matching(tokens: &[Token], k: usize, open: &str, close: &str) -> Option<usize> {
    let mut depth = 0i64;
    for (i, t) in tokens.iter().enumerate().skip(k) {
        if t.is(open) {
            depth += 1;
        } else if t.is(close) {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// For a `)` at `k` followed by `{`, the token range of that block.
fn method_body(tokens: &[Token], k: usize) -> Option<(usize, usize)> {
    if !tokens[k].is(")") || !tokens.get(k + 1).is_some_and(|t| t.is("{")) {
        return None;
    }
    matching(tokens, k + 1, "{", "}").map(|end| (k + 1, end))
}

fn rename(text: &[char], tokens: &[Token], suffix: char) -> Vec<char> {
    let mut out = Vec::with_capacity(text.len() + tokens.len());
    let mut last = 0;
    for t in tokens {
        if t.is_name() && !t.in_set(&SOFT) && t.text[0].is_ascii_alphabetic() {
            out.extend_from_slice(&text[last..t.end]);
            out.push(suffix);
            last = t.end;
        }
    }
    out.extend_from_slice(&text[last..]);
    out
}

/// Every mutant of `text`: its directory (`operator` or `operator/variant`), its site index and its text.
fn mutants_of(text: &[char], per_file: usize, seed: &str, rel: &str) -> Vec<(String, usize, Vec<char>)> {
    let tokens = tokenize(text);
    let mut out = Vec::new();
    for op in OPS {
        let mut rng = PyRandom::from_str_seed(&format!("{seed}:{rel}:{}", op.name()));
        let sites: Vec<usize> = (0..tokens.len()).filter(|&k| op.applies(&tokens, k)).collect();
        let picked = if sites.len() > per_file {
            let mut picked: Vec<usize> = rng.sample(sites.len(), per_file).into_iter().map(|i| sites[i]).collect();
            picked.sort_unstable();
            picked
        } else {
            sites
        };
        for (index, site) in picked.into_iter().enumerate() {
            for (variant, mutated) in op.apply(text, &tokens, site, &mut rng) {
                if mutated != text {
                    let dir = if variant.is_empty() { op.name().to_owned() } else { format!("{}/{variant}", op.name()) };
                    out.push((dir, index, mutated));
                }
            }
        }
    }
    for (name, suffix) in [("unicode-latin", 'é'), ("unicode-cjk", '名')] {
        let mutated = rename(text, &tokens, suffix);
        if mutated != text {
            out.push((name.to_owned(), 0, mutated));
        }
    }
    out
}

/// A JSON string as Python's `json.dump` writes it (`ensure_ascii`): serde_json's escapes of the ASCII controls,
/// quote and backslash (the same as Python's, lowercase hex), then DEL and every non-ASCII character as its UTF-16
/// units.
fn json_string(s: &str) -> String {
    let escaped = serde_json::to_string(s).expect("a string serializes");
    let mut out = String::with_capacity(escaped.len());
    let mut units = [0u16; 2];
    for c in escaped.chars() {
        if c.is_ascii() && c != '\u{7f}' {
            out.push(c);
        } else {
            c.encode_utf16(&mut units).iter().for_each(|u| out.push_str(&format!("\\u{u:04x}")));
        }
    }
    out
}

pub fn mutate(goldens: &Path, out: &Path, per_file: usize, seed: &str) -> Result<()> {
    let manifest = std::fs::read_to_string(goldens.join("manifest.tsv")).context("reading manifest.tsv")?;
    let write = |rel: &str, text: &[char]| -> Result<()> {
        let dest = out.join(rel);
        std::fs::create_dir_all(dest.parent().expect("a file under OUT"))?;
        std::fs::write(&dest, text.iter().collect::<String>()).with_context(|| format!("writing {}", dest.display()))
    };
    let (mut lines, mut origins) = (Vec::new(), BTreeMap::new());
    let (mut inputs, mut skipped) = (0, 0);
    for row in manifest.lines() {
        let [path, level, entry, trimmed, _] = row.split('\t').collect::<Vec<_>>()[..] else {
            anyhow::bail!("manifest.tsv: `{row}` is not five columns");
        };
        let stem = path.strip_suffix(".java").with_context(|| format!("{path}: not a .java file"))?;
        let golden = std::fs::read_to_string(goldens.join(format!("{stem}.txt"))).with_context(|| format!("reading {stem}.txt"))?;
        if golden.contains("PsiErrorElement") {
            skipped += 1;
            continue;
        }
        inputs += 1;
        let raw = std::fs::read_to_string(goldens.join(path)).with_context(|| format!("reading {path}"))?;
        let mut text = raw.replace("\r\n", "\n").replace('\r', "\n");
        if trimmed == "1" {
            text = text.trim_matches(|c: char| c <= ' ').to_owned();
        }
        let text: Vec<char> = text.chars().collect();
        let original = format!("original/{path}");
        write(&original, &text)?;
        lines.push(format!("{original}\t{level}\t{entry}\t0"));
        for (dir, index, mutated) in mutants_of(&text, per_file, seed, path) {
            let mutant = format!("{dir}/{stem}~{index}.java");
            write(&mutant, &mutated)?;
            lines.push(format!("{mutant}\t{level}\t{entry}\t0"));
            origins.insert(mutant, original.clone());
        }
    }
    let json: Vec<String> = origins.iter().map(|(k, v)| format!("{}: {}", json_string(k), json_string(v))).collect();
    std::fs::write(out.join("origins.json"), format!("{{{}}}\n", json.join(", "))).context("writing origins.json")?;
    std::fs::write(out.join("list"), lines.iter().map(|l| format!("{l}\n")).collect::<String>()).context("writing list")?;
    println!(
        "mutate: {inputs} PSI-clean inputs ({skipped} with a PSI error left out), {} mutants, {} files in {}/list",
        lines.len() - inputs,
        lines.len(),
        out.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_pythons() {
        let text: Vec<char> = "a$b $c _d 1.5x .5 x>>>=y // c\n'q\n\"s\\\"\" /* */ \"\"\"t\"\"\"".chars().collect();
        let tokens: Vec<(Kind, String, usize)> = tokenize(&text).iter().map(|t| (t.kind, t.text.iter().collect(), t.line)).collect();
        let want = [
            // `[^\W\d]\w*` stops at `$`, where `[$_][\w$]*` starts.
            (Kind::Word, "a", 1),
            (Kind::Word, "$b", 1),
            (Kind::Word, "$c", 1),
            (Kind::Word, "_d", 1),
            (Kind::Number, "1.5x", 1),
            (Kind::Number, ".5", 1),
            (Kind::Word, "x", 1),
            (Kind::Punct, ">>>=", 1),
            (Kind::Word, "y", 1),
            (Kind::Char, "'q\n", 2),
            (Kind::Str, "\"s\\\"\"", 3),
            (Kind::Str, "\"\"\"t\"\"\"", 3),
        ];
        assert_eq!(tokens, want.map(|(k, s, l)| (k, s.to_owned(), l)));
    }

    #[test]
    fn origins_are_written_as_python_json() {
        // Non-ASCII as lowercase UTF-16 escapes, a backslash and `u` apart here.
        let u = |hex: &str| format!("\\{}{hex}", 'u');
        assert_eq!(json_string("a\"\\\n\u{e9}\u{1F4A9}"), format!("\"a\\\"\\\\\\n{}{}{}\"", u("00e9"), u("d83d"), u("dca9")));
        assert_eq!(json_string("\u{1}\u{7f}\u{8}"), format!("\"{}{}\\b\"", u("0001"), u("007f")));
    }
}
