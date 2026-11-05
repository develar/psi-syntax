//! `java-oracle vendor-goldens`: IntelliJ's Java parsing goldens into `testdata/java/`.
//!
//! Reads `java/java-tests/testData/psi/parser-{full,partial}` and the test classes that use them
//! (`java/java-frontback-tests/testSrc/com/intellij/java/parser`) at the revision, never from the working tree. For
//! every test method that checks a golden it writes, under `OUT/<parser-full|parser-partial>/<dir>/`:
//!
//! - `NAME.java`: the parsed text, the test data file for parser-full (verbatim; the test trims it), the string the
//!   test passes for parser-partial (the exact argument, not trimmed);
//! - `NAME.txt`: `DebugUtil.psiToString(file, true, false)`, the golden as committed;
//! - `NAME_node.txt`: `DebugUtil.nodeTreeAsElementTypeToString(file.getNode(), true)`, as committed;
//!
//! and one manifest line `PATH<TAB>LEVEL<TAB>ENTRY<TAB>TRIM<TAB>TEST` in `OUT/manifest.tsv`: LEVEL the `LanguageLevel`
//! the test parses at (its `setLanguageLevel(...)`, else `JavaTestUtil.getMaxRegisteredLanguageLevel()`, the last
//! `LanguageLevel` constant), ENTRY `file` or the parser entry of a partial test (`JavaPsiOracle.java`), TRIM `1` when
//! the test trims the text (`ParsingTestFixture.loadFile`). JShell goldens (`parser-full/jshell`) are not vendored.
//!
//! The test sources are read as their Java text: the method heads, the calls of the helpers and their arguments
//! (string literals, text blocks with JLS 3.10.6's indentation, concatenations, booleans).
use anyhow::{Context, Result, bail};
use ide_oracle::git::Revision;
use ide_oracle::goldens::{Goldens, Order};
use std::collections::BTreeSet;
use std::path::Path;

pub const REV: &str = "decc943674edfe7ecc620d2d60b61be38f876994";
const DATA: &str = "java/java-tests/testData/psi/";
const ABSTRACT: &str = "java/java-frontback-tests/testSrc/com/intellij/java/parser/";
const LEVELS: &str = "java/java-syntax/src/com/intellij/pom/java/LanguageLevel.kt";
const CALLS: [&str; 6] = ["doTest", "doParserTest", "doRefParserTest", "doTypeParserTest", "doTypeParamsParserTest", "doBlockParserTest"];

#[derive(Debug, Clone, PartialEq)]
enum Arg {
    Str(String),
    Bool(bool),
}

impl Arg {
    fn truthy(&self) -> bool {
        match self {
            Arg::Str(s) => !s.is_empty(),
            Arg::Bool(b) => *b,
        }
    }
}

/// The concrete partial tests (`java/java-tests/testSrc/.../parser/partial/*Test.java`) implement each abstract
/// helper with one parser call; the entry names that call.
fn partial_entry(class: &str, call: &str, args: &[Arg]) -> Result<&'static str> {
    Ok(match (class, call, args.len()) {
        ("AbstractBasicAnnotationParserTest", "doParserTest", 1) => "annotations",
        ("AbstractBasicDeclarationParserTest", "doParserTest", 1) => "class_body",
        ("AbstractBasicDeclarationParserTest", "doParserTest", 3) => match (&args[1], &args[2]) {
            (Arg::Bool(false), Arg::Bool(false)) => "class_body",
            (Arg::Bool(true), Arg::Bool(false)) => "class_body_annotation",
            (Arg::Bool(false), Arg::Bool(true)) => "class_body_enum",
            other => bail!("{class}.{call}: no entry for the flags {other:?}"),
        },
        ("AbstractBasicExpressionParserTest", "doParserTest", 0 | 1) => "expression",
        ("AbstractBasicFileParserTest" | "AbstractBasicModuleParserTest", "doParserTest", 1) => "file_parser",
        ("AbstractBasicPatternParserTest", "doParserTest", 1) => "pattern",
        ("AbstractBasicReferenceParserTest", "doRefParserTest", 2) => {
            if args[1].truthy() {
                "reference_incomplete"
            } else {
                "reference"
            }
        }
        ("AbstractBasicReferenceParserTest", "doTypeParserTest", 1) => "type",
        ("AbstractBasicReferenceParserTest", "doTypeParamsParserTest", 1) => "type_params",
        ("AbstractBasicStatementParserTest", "doBlockParserTest", 1) => "code_block_deep",
        ("AbstractBasicStatementParserTest", "doParserTest", 1) => "statements",
        _ => bail!("{class}.{call} with {} arguments: no parser entry", args.len()),
    })
}

/// `\w` of a Python `str` pattern.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `str.isspace()`: Unicode's whitespace and the four separators U+001C..U+001F.
fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

fn char_at(src: &str, i: usize) -> Result<char> {
    src.get(i..).and_then(|s| s.chars().next()).with_context(|| format!("the test source ends at byte {i}"))
}

/// The end of the run of `\w` characters from `i`.
fn word_end(src: &str, i: usize) -> usize {
    i + src[i..].chars().take_while(|&c| is_word(c)).map(char::len_utf8).sum::<usize>()
}

/// The string literal or text block at `src[i]`, unescaped, and the offset after it.
fn java_string(src: &str, i: usize) -> Result<(String, usize)> {
    if src[i..].starts_with("\"\"\"") {
        let j = i + 3 + src[i + 3..].find('\n').context("a text block without a line end")? + 1;
        let mut end = j;
        loop {
            end += src[end..].find("\"\"\"").context("an unclosed text block")?;
            let backslashes = src[..end].bytes().rev().take_while(|&b| b == b'\\').count();
            if backslashes % 2 == 0 {
                break;
            }
            end += 1;
        }
        let lines: Vec<&str> = src[j..end].split('\n').collect();
        let (last, body) = lines.split_last().expect("split yields one line at least");
        // JLS 3.10.6: the closing delimiter's line counts for the indentation when it is blank.
        let indent = body
            .iter()
            .filter(|l| l.chars().any(|c| !is_space(c)))
            .chain(std::iter::once(last))
            .map(|l| l.len() - l.trim_start_matches([' ', '\t']).len())
            .min()
            .unwrap_or(0);
        let skip = |l: &str| -> String { l.chars().skip(indent).collect() };
        let mut stripped: Vec<String> = body.iter().map(|l| skip(l).trim_end_matches([' ', '\t']).to_owned()).collect();
        stripped.push(skip(last));
        return Ok((unescape(&stripped.join("\n"), true)?, end + 3));
    }
    if char_at(src, i)? != '"' {
        bail!("a string expected at {:?}", &src[i..(i + 40).min(src.len())]);
    }
    let mut j = i + 1;
    loop {
        let c = char_at(src, j)?;
        if c == '"' {
            break;
        }
        j += c.len_utf8();
        if c == '\\' {
            j += char_at(src, j)?.len_utf8();
        }
    }
    Ok((unescape(&src[i + 1..j], false)?, j + 1))
}

fn unescape(s: &str, text_block: bool) -> Result<String> {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let n = chars.next().context("an escape at the end of a string")?;
        let simple = match n {
            'n' => Some('\n'),
            't' => Some('\t'),
            'b' => Some('\u{8}'),
            'r' => Some('\r'),
            'f' => Some('\u{c}'),
            's' => Some(' '),
            '"' | '\'' | '\\' => Some(n),
            _ => None,
        };
        if let Some(v) = simple {
            out.push(v);
        } else if n == '\n' && text_block {
        } else if ('0'..='7').contains(&n) {
            // `[0-3][0-7]{0,2}|[0-7]{1,2}`
            let max = if n <= '3' { 2 } else { 1 };
            let mut value = n.to_digit(8).expect("an octal digit");
            for _ in 0..max {
                match chars.peek().and_then(|c| c.to_digit(8)) {
                    Some(d) => {
                        value = value * 8 + d;
                        chars.next();
                    }
                    None => break,
                }
            }
            out.push(char::from_u32(value).expect("at most \\377"));
        } else {
            bail!("escape \\{n}");
        }
    }
    Ok(out)
}

fn skip_space(src: &str, mut i: usize) -> Result<usize> {
    while matches!(char_at(src, i)?, ' ' | '\t' | '\r' | '\n') {
        i += 1;
    }
    Ok(i)
}

/// The arguments of a call whose `(` is at `src[i]`: concatenated strings and booleans.
fn parse_args(src: &str, i: usize) -> Result<Vec<Arg>> {
    let mut args = Vec::new();
    let mut i = skip_space(src, i + 1)?;
    if char_at(src, i)? == ')' {
        return Ok(args);
    }
    loop {
        i = skip_space(src, i)?;
        if char_at(src, i)? == '"' {
            let mut value = String::new();
            loop {
                let (part, next) = java_string(src, i)?;
                value.push_str(&part);
                i = skip_space(src, next)?;
                if char_at(src, i)? != '+' {
                    break;
                }
                i = skip_space(src, i + 1)?;
            }
            args.push(Arg::Str(value));
        } else if src[i..].starts_with("true") {
            args.push(Arg::Bool(true));
            i += 4;
        } else if src[i..].starts_with("false") {
            args.push(Arg::Bool(false));
            i += 5;
        } else {
            bail!("argument at {:?}", src[i..].chars().take(40).collect::<String>());
        }
        i = skip_space(src, i)?;
        match char_at(src, i)? {
            ')' => return Ok(args),
            ',' => i += 1,
            _ => bail!("`,` expected at {:?}", src[i..].chars().take(40).collect::<String>()),
        }
    }
}

struct TestMethod {
    name: String,
    level: Option<String>,
    /// The helper it calls and its arguments; `None` for a method that compares no dump.
    call: Option<(&'static str, Vec<Arg>)>,
}

/// Each `public void testX() {` method: the first helper call before the next test method, and a
/// `setLanguageLevel(LanguageLevel.L)` before that call.
fn test_methods(src: &str) -> Result<Vec<TestMethod>> {
    const HEAD: &str = "public void test";
    let mut methods = Vec::new();
    let mut from = 0;
    while let Some(found) = src[from..].find(HEAD) {
        let at = from + found;
        // `public void (test\w+)\(\)\s*\{`
        let name_end = word_end(src, at + HEAD.len());
        let mut body_start = None;
        if name_end > at + HEAD.len() && src[name_end..].starts_with("()") {
            let mut k = name_end + 2;
            while let Some(c) = src[k..].chars().next().filter(|&c| is_space(c)) {
                k += c.len_utf8();
            }
            if src[k..].starts_with('{') {
                body_start = Some(k + 1);
            }
        }
        let Some(body_start) = body_start else {
            from = at + 1;
            continue;
        };
        let name = src[at + "public void ".len()..name_end].to_owned();
        let limit = src[body_start..].find(HEAD).map_or(src.len(), |n| body_start + n);
        let Some((call, call_start, open)) = find_call(src, body_start, limit) else {
            methods.push(TestMethod { name, level: None, call: None });
            from = body_start;
            continue;
        };
        let level = find_level(src, body_start, call_start);
        methods.push(TestMethod { name, level, call: Some((call, parse_args(src, open)?)) });
        from = body_start;
    }
    Ok(methods)
}

/// The first `\b(doTest|...)\(` in `src[from..limit]`: the helper, where it starts and the offset of its `(`.
fn find_call(src: &str, from: usize, limit: usize) -> Option<(&'static str, usize, usize)> {
    let region = &src[..limit];
    let mut at = from;
    while let Some(found) = region[at..].find("do") {
        let q = at + found;
        let boundary = src[..q].chars().next_back().is_none_or(|c| !is_word(c));
        if boundary {
            for call in CALLS {
                if region[q..].starts_with(call) && region[q + call.len()..].starts_with('(') {
                    return Some((call, q, q + call.len()));
                }
            }
        }
        at = q + 1;
    }
    None
}

/// `setLanguageLevel\(LanguageLevel\.(\w+)\)` in `src[from..limit]`.
fn find_level(src: &str, from: usize, limit: usize) -> Option<String> {
    const CALL: &str = "setLanguageLevel(LanguageLevel.";
    let region = &src[..limit];
    let mut at = from;
    while let Some(found) = region[at..].find(CALL) {
        let start = at + found + CALL.len();
        let end = word_end(src, start);
        if end > start && end < limit && src[end..].starts_with(')') {
            return Some(src[start..end].to_owned());
        }
        at = at + found + 1;
    }
    None
}

/// The data directory of a test class: `super("parser-...",`.
fn data_dir(src: &str) -> Option<&str> {
    const SUPER: &str = "super(\"parser-";
    let mut at = 0;
    while let Some(found) = src[at..].find(SUPER) {
        let start = at + found + "super(\"".len();
        let value_start = at + found + SUPER.len();
        if let Some(len) = src[value_start..].find('"').filter(|&n| n > 0) {
            return Some(&src[start..value_start + len]);
        }
        at = at + found + 1;
    }
    None
}

/// The last `JDK_*` constant of `enum class LanguageLevel`: `JavaTestUtil.getMaxRegisteredLanguageLevel()`.
fn max_level(levels: &str) -> Result<String> {
    let body = &levels[levels.find("enum class LanguageLevel {").context("no enum class LanguageLevel")?..];
    let constants = &body[..body.find(';').context("no end of LanguageLevel's constants")?];
    constants
        .split('\n')
        .filter_map(|line| {
            let line = line.trim_start_matches(is_space);
            let end = word_end(line, 0);
            (line.starts_with("JDK_") && end > 4 && line[end..].starts_with('(')).then(|| line[..end].to_owned())
        })
        .next_back()
        .context("no JDK_ constant in LanguageLevel")
}

pub fn vendor(repo: &Path, rev: &str, out: &Path) -> Result<()> {
    let mut git = Revision::open(repo, rev)?;
    let max_level = max_level(&String::from_utf8(git.show(LEVELS)?).context("LanguageLevel.kt is not UTF-8")?)?;
    let data_files: BTreeSet<String> =
        git.ls(&format!("{DATA}parser-full"))?.into_iter().chain(git.ls(&format!("{DATA}parser-partial"))?).collect();
    let mut goldens = Goldens::create(out, &["parser-full", "parser-partial"])?;
    let mut used = BTreeSet::new();
    let mut unmatched = Vec::new();
    for path in git.ls(ABSTRACT)? {
        let file = path.rsplit('/').next().unwrap_or(&path);
        let Some(class) = file.strip_suffix(".java").filter(|c| c.starts_with("AbstractBasic") && path.ends_with("Test.java")) else {
            continue;
        };
        let src = String::from_utf8(git.show(&path)?).with_context(|| format!("{path} is not UTF-8"))?;
        let data_dir = data_dir(&src).with_context(|| format!("{path}: no super(\"parser-...\")"))?;
        let partial = data_dir.starts_with("parser-partial");
        for TestMethod { name: method, level, call } in test_methods(&src).with_context(|| path.clone())? {
            // PlatformTestUtil.getTestName(name, lowercaseFirstLetter = false)
            let name = method.strip_prefix("test").unwrap_or(&method);
            let Some((call, args)) = call else {
                unmatched.push(format!("{class}.{method}: compares no dump"));
                continue;
            };
            let base = format!("{DATA}{data_dir}/{name}");
            if !data_files.contains(&format!("{base}.txt")) {
                unmatched.push(format!("{class}.{method}: no {name}.txt"));
                continue;
            }
            if call == "doTest" && args == [Arg::Bool(false)] {
                unmatched.push(format!("{class}.{method}: doTest(false) does not compare {name}.txt"));
                continue;
            }
            let (entry, text, trim) = if partial {
                let entry = partial_entry(class, call, &args)?;
                match args.first() {
                    None => (entry, git.show(&format!("{base}.java"))?, 1),
                    Some(Arg::Str(text)) => (entry, text.clone().into_bytes(), 0),
                    Some(Arg::Bool(_)) => bail!("{class}.{method}: the text argument is a boolean"),
                }
            } else {
                if call != "doTest" || args != [Arg::Bool(true)] {
                    bail!("{class}.{method}: a parser-full test calls {call}{args:?}, not doTest(true)");
                }
                ("file", git.show(&format!("{base}.java"))?, 1)
            };
            let rel = format!("{data_dir}/{name}");
            goldens.write(&format!("{rel}.java"), &text)?;
            used.insert(format!("{base}.java"));
            for suffix in [".txt", "_node.txt"] {
                let golden = format!("{base}{suffix}");
                if data_files.contains(&golden) {
                    goldens.write(&format!("{rel}{suffix}"), &git.show(&golden)?)?;
                    used.insert(golden);
                }
            }
            goldens.record(format!("{rel}.java\t{}\t{entry}\t{trim}\t{class}.{method}", level.as_deref().unwrap_or(&max_level)));
        }
    }
    let full = goldens.recorded().iter().filter(|l| l.split('\t').nth(2) == Some("file")).count();
    let total = goldens.recorded().len();
    goldens.finish("manifest.tsv", Order::Sorted)?;
    let left: Vec<&String> = data_files.iter().filter(|p| !used.contains(*p) && !p.contains("/jshell/")).collect();
    println!("vendored {total} tests ({full} full, {} partial); default level {max_level}", total - full);
    println!("test methods without a golden: {}", unmatched.len());
    for line in &unmatched {
        println!("  {line}");
    }
    println!("data files no test reads (jshell excluded): {}", left.len());
    for p in left {
        println!("  {}", &p[DATA.len()..]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_and_text_blocks_read_as_java_reads_them() {
        assert_eq!(java_string(r#""a\tb\"c\101\0" x"#, 0).unwrap(), ("a\tb\"cA\0".to_owned(), 15));
        let block = "\"\"\"\n    class A {\n      int a;  \n\n    }\n    \"\"\"";
        assert_eq!(java_string(block, 0).unwrap().0, "class A {\n  int a;\n\n}\n");
        assert_eq!(java_string("\"\"\"\n  a \\\n  b\"\"\"", 0).unwrap().0, "a b");
        assert!(unescape("\\u0041", false).is_err());
    }

    #[test]
    fn test_methods_name_their_call_level_and_arguments() {
        let src = r#"
  public void testA() { setLanguageLevel(LanguageLevel.JDK_21); doParserTest("x" +
     "y", false, true); }
  public void testNone() { }
  public void testB()
  {
    undoTest(true); doTest(true);
  }"#;
        let methods = test_methods(src).unwrap();
        assert_eq!(methods.len(), 3);
        assert_eq!(methods[0].name, "testA");
        assert_eq!(methods[0].level.as_deref(), Some("JDK_21"));
        assert_eq!(methods[0].call, Some(("doParserTest", vec![Arg::Str("xy".into()), Arg::Bool(false), Arg::Bool(true)])));
        assert!(methods[1].call.is_none());
        assert_eq!(methods[2].call, Some(("doTest", vec![Arg::Bool(true)])));
        assert_eq!(partial_entry("AbstractBasicDeclarationParserTest", "doParserTest", &methods[0].call.as_ref().unwrap().1).unwrap(), "class_body_enum");
    }

    #[test]
    fn the_default_level_is_the_last_constant() {
        let kt = "enum class LanguageLevel {\n  JDK_1_3(1),\n  JDK_X(2),\n  ;\n  JDK_Y(3)";
        assert_eq!(max_level(kt).unwrap(), "JDK_X");
        assert_eq!(data_dir(r#"super("parser-partial/annotations", x)"#), Some("parser-partial/annotations"));
    }
}
