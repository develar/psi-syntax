//! A string literal's value: `StarlarkStringLiteralExpression.getStringContents` (the text inside its quotes, after an
//! allowed prefix, `StarlarkQuote.unwrap`) decoded as `StarlarkStringLiteralEscaper.decodeEscapes` decodes it, a raw
//! string (`r"…"`) left as written. What every consumer of a literal reads, so none decodes one of its own.

use std::ops::Range;

/// `ALLOWED_STRING_PREFIXES`: capital letters are not allowed here in Starlark.
const ALLOWED_PREFIXES: [&str; 5] = ["", "r", "b", "rb", "br"];

/// `StarlarkQuote`, longest first as `ofString` tries them.
const QUOTES: [&str; 4] = ["'''", "\"\"\"", "'", "\""];

/// `getRawPrefixLength`: the prefix before the first quote when it is an allowed one, else none.
fn prefix_len(literal: &str) -> usize {
    let prefix = literal.find(['\'', '"']).map_or(literal, |at| &literal[..at]);
    if ALLOWED_PREFIXES.contains(&prefix) { prefix.len() } else { 0 }
}

/// `StarlarkQuote.ofString` of the text after the prefix: "" (`UNQUOTED`) for none.
fn quote_of(text: &str) -> &'static str {
    QUOTES.into_iter().find(|q| text.starts_with(q)).unwrap_or("")
}

/// `isRaw`: the prefix holds an `r`.
pub fn is_raw(literal: &str) -> bool {
    literal[..prefix_len(literal)].contains(['r', 'R'])
}

/// `getStringContents`: the text inside the quotes (`String.removeSurrounding`, which removes both quotes or none).
pub fn contents(literal: &str) -> &str {
    let text = &literal[prefix_len(literal)..];
    let quote = quote_of(text);
    match text.strip_prefix(quote).and_then(|inner| inner.strip_suffix(quote)) {
        Some(inner) if text.len() >= quote.len() * 2 => inner,
        _ => text,
    }
}

/// `getStringContentsOffset`: where the contents lie in the literal (`StarlarkQuote.rangeWithinQuotes` past the
/// prefix): an unterminated literal's run to its end, a closing quote escaped by an odd run of backslashes being none.
pub fn contents_range(literal: &str) -> Range<usize> {
    let prefix = prefix_len(literal);
    let text = &literal[prefix..];
    let quote = quote_of(text);
    let closed = text.len() >= quote.len() * 2
        && text.ends_with(quote)
        && text[..text.len() - quote.len()].bytes().rev().take_while(|&b| b == b'\\').count() % 2 == 0;
    let end = if closed { text.len() - quote.len() } else { text.len() };
    prefix + quote.len()..prefix + end.max(quote.len())
}

/// The literal's value: its contents decoded (`decode`), a raw literal's as written.
pub fn value(literal: &str) -> String {
    let contents = contents(literal);
    if is_raw(literal) { contents.to_owned() } else { decode(contents) }
}

/// `StarlarkStringLiteralEscaper.decodeEscapes`: `\n`, `\t`, `\r`, `\a`, `\b`, `\f`, `\v`, `\\`, `\"`, `\'`, a line
/// continuation (`\` and its line break dropped), `\ooo`, `\xhh`, `\uhhhh` and `\Uhhhhhhhh`; any other escape, a hex
/// escape that is short or names no code point (a surrogate too, which a Rust string cannot hold), keeps its
/// backslash.
pub fn decode(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut index = 0;
    while index < bytes.len() {
        let rest = &source[index..];
        let ch = rest.chars().next().expect("index on a char boundary");
        if ch != '\\' || index + 1 >= bytes.len() {
            out.push(ch);
            index += ch.len_utf8();
            continue;
        }
        let next = bytes[index + 1];
        let simple = match next {
            b'n' => Some('\n'),
            b't' => Some('\t'),
            b'r' => Some('\r'),
            b'a' => Some('\u{7}'),
            b'b' => Some('\u{8}'),
            b'f' => Some('\u{c}'),
            b'v' => Some('\u{b}'),
            b'\\' | b'"' | b'\'' => Some(next as char),
            _ => None,
        };
        if let Some(decoded) = simple {
            out.push(decoded);
            index += 2;
            continue;
        }
        match next {
            b'\n' => index += 2,
            b'x' | b'u' | b'U' => {
                let digits = match next {
                    b'x' => 2,
                    b'u' => 4,
                    _ => 8,
                };
                let hex =
                    source.get(index + 2..index + 2 + digits).filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()));
                match hex.and_then(|h| u32::from_str_radix(h, 16).ok()).and_then(char::from_u32) {
                    Some(decoded) => {
                        out.push(decoded);
                        index += 2 + digits;
                    }
                    None => {
                        out.push('\\');
                        index += 1;
                    }
                }
            }
            b'0'..=b'7' => {
                let mut end = index + 2;
                while end < bytes.len() && end < index + 4 && (b'0'..=b'7').contains(&bytes[end]) {
                    end += 1;
                }
                let code = u32::from_str_radix(&source[index + 1..end], 8).expect("octal digits");
                out.push(char::from_u32(code).expect("an octal escape is below 0o777"));
                index = end;
            }
            _ => {
                out.push('\\');
                index += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_literal_is_its_contents_decoded_unless_raw() {
        assert_eq!(value(r#""//pkg:x""#), "//pkg:x");
        assert_eq!(value(r#"'a\tb\'c'"#), "a\tb'c");
        assert_eq!(value(r#"r"a\tb""#), r"a\tb");
        assert_eq!(value(r#"b"\x41\101B""#), "AAB");
        assert_eq!(value("\"\"\"one\\\ntwo\"\"\""), "onetwo");
        assert_eq!(value(r#""\q\x4""#), r"\q\x4");
        assert_eq!(value(r#""\ud800""#), r"\ud800");
        // An unterminated literal keeps its text after the opening quote, as `removeSurrounding` removes none.
        assert_eq!(contents("\"open"), "\"open");
        assert_eq!(contents_range("\"open"), 1..5);
        assert_eq!(contents_range(r#"rb"x""#), 3..4);
        assert_eq!(contents_range(r#""a\""#), 1..4);
        // A prefix Starlark does not allow is no prefix: the text before the quote stays.
        assert_eq!(contents("R\"x\""), "R\"x\"");
    }
}
