//! Values and source ranges of Starlark string literals.

use std::ops::Range;

/// The byte range of a string literal's contents, excluding its prefix and quotes.
pub fn contents_range(raw: &str) -> Range<usize> {
    let bytes = raw.as_bytes();
    let quote_at = if bytes.get(..2).is_some_and(|prefix| prefix == b"br" || prefix == b"rb")
        && matches!(bytes.get(2), Some(b'\'' | b'"'))
    {
        2
    } else if matches!(bytes.first(), Some(b'b' | b'r')) && matches!(bytes.get(1), Some(b'\'' | b'"')) {
        1
    } else {
        0
    };
    let Some(&quote) = bytes.get(quote_at).filter(|&&byte| matches!(byte, b'\'' | b'"')) else {
        return 0..raw.len();
    };
    let triple = bytes.get(quote_at..quote_at + 3).is_some_and(|delimiter| delimiter == [quote; 3]);
    let delimiter_len = if triple { 3 } else { 1 };
    let start = quote_at + delimiter_len;
    let end = raw
        .len()
        .checked_sub(delimiter_len)
        .filter(|&end| end >= start && bytes[end..].iter().all(|&byte| byte == quote) && !is_escaped(bytes, end));
    start..end.unwrap_or(raw.len())
}

/// The decoded value of a Starlark string literal.
pub fn value(raw: &str) -> String {
    let range = contents_range(raw);
    let contents = &raw[range];
    let prefix = &raw[..raw.find(['\'', '"']).unwrap_or(0)];
    if prefix.contains('r') {
        return contents.to_owned();
    }
    decode_escapes(contents)
}

fn is_escaped(bytes: &[u8], at: usize) -> bool {
    let mut backslashes = 0;
    let mut index = at;
    while index > 0 && bytes[index - 1] == b'\\' {
        backslashes += 1;
        index -= 1;
    }
    backslashes % 2 != 0
}

fn decode_escapes(contents: &str) -> String {
    let bytes = contents.as_bytes();
    let mut value = String::with_capacity(contents.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'\\' {
            let character = contents[index..].chars().next().expect("index is within the string");
            value.push(character);
            index += character.len_utf8();
            continue;
        }

        index += 1;
        let Some(&escaped) = bytes.get(index) else {
            value.push('\\');
            break;
        };
        index += 1;
        match escaped {
            b'a' => value.push('\u{7}'),
            b'b' => value.push('\u{8}'),
            b'f' => value.push('\u{c}'),
            b'n' => value.push('\n'),
            b'r' => value.push('\r'),
            b't' => value.push('\t'),
            b'v' => value.push('\u{b}'),
            b'\\' => value.push('\\'),
            b'\'' => value.push('\''),
            b'"' => value.push('"'),
            b'\n' => {}
            b'\r' => {
                if bytes.get(index) == Some(&b'\n') {
                    index += 1;
                }
            }
            b'x' if push_numeric_escape(&mut value, contents, &mut index, 2) => {}
            b'u' if push_numeric_escape(&mut value, contents, &mut index, 4) => {}
            b'U' if push_numeric_escape(&mut value, contents, &mut index, 8) => {}
            b'0'..=b'7' => {
                let mut codepoint = u32::from(escaped - b'0');
                let mut digits = 1;
                while digits < 3 {
                    let Some(&next @ b'0'..=b'7') = bytes.get(index) else { break };
                    codepoint = codepoint * 8 + u32::from(next - b'0');
                    index += 1;
                    digits += 1;
                }
                value.push(char::from_u32(codepoint).unwrap_or(char::REPLACEMENT_CHARACTER));
            }
            _ => {
                value.push('\\');
                if escaped.is_ascii() {
                    value.push(char::from(escaped));
                } else {
                    let character = contents[index - 1..].chars().next().expect("index is within the string");
                    value.push(character);
                    index += character.len_utf8() - 1;
                }
            }
        }
    }
    value
}

fn push_numeric_escape(value: &mut String, contents: &str, index: &mut usize, digits: usize) -> bool {
    let start = *index;
    let Some(end) = start.checked_add(digits) else { return false };
    let Some(hex) = contents.as_bytes().get(start..end) else { return false };
    let Ok(hex) = std::str::from_utf8(hex) else { return false };
    let Ok(codepoint) = u32::from_str_radix(hex, 16) else { return false };
    let Some(character) = char::from_u32(codepoint) else { return false };
    value.push(character);
    *index = end;
    true
}

#[cfg(test)]
mod tests {
    use super::{contents_range, value};

    #[test]
    fn decodes_starlark_escapes_and_preserves_raw_strings() {
        assert_eq!(value(r#""\x41\u03b1\101""#), "AαA");
        assert_eq!(value(r#"r"\n\"""#), r#"\n\""#);
        assert_eq!(value("'line\\nnext'"), "line\nnext");
    }

    #[test]
    fn returns_content_ranges_for_prefixed_and_triple_quoted_strings() {
        for raw in ["\"hello\"", "r'hello'", "br\"\"\"hello\"\"\""] {
            let range = contents_range(raw);
            assert_eq!(&raw[range], "hello");
        }
        assert_eq!(contents_range("\"unfinished"), 1..11);
    }
}
