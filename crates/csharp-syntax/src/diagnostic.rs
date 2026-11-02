//! The text of a diagnostic: `DiagnosticInfo.GetMessage(CultureInfo.InvariantCulture)` (and `XmlSyntaxDiagnosticInfo`'s),
//! `string.Format` of the English message format with the arguments as .NET formats the objects Roslyn passes.

use std::fmt::Write;

use crate::dump::SourceHalves;
use crate::green::{DiagArg, DiagnosticId, GreenArena};

/// How deep embedded diagnostics (`WRN_ErrorOverride`'s argument) are formatted; Roslyn nests one.
const MAX_EMBEDDED: u32 = 8;

/// A message as .NET's string holds it: `text` has each lone surrogate (half of a supplementary character, which a
/// Rust string cannot hold) as U+FFFD, `lone_surrogates` their byte offsets in `text` and code units, in order.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Message {
    pub text: String,
    pub lone_surrogates: Vec<(u32, u16)>,
}

impl Message {
    fn push_str(&mut self, s: &str) {
        self.text.push_str(s);
    }

    fn push_lone_surrogate(&mut self, unit: u16) {
        self.lone_surrogates.push((self.text.len() as u32, unit));
        self.text.push('\u{FFFD}');
    }

    fn push_message(&mut self, other: &Message) {
        let base = self.text.len() as u32;
        self.lone_surrogates.extend(other.lone_surrogates.iter().map(|&(offset, unit)| (base + offset, unit)));
        self.text.push_str(&other.text);
    }
}

impl From<String> for Message {
    fn from(text: String) -> Message {
        Message { text, lone_surrogates: Vec::new() }
    }
}

/// `GetMessage(CultureInfo.InvariantCulture)` of the diagnostic `id` of `arena`, whose tokens index `source`.
pub fn message(arena: &GreenArena, source: &[u8], id: DiagnosticId) -> Message {
    message_at(arena, source, id, 0)
}

fn message_at(arena: &GreenArena, source: &[u8], id: DiagnosticId, depth: u32) -> Message {
    let info = arena.diagnostic(id);
    let args = arena.diagnostic_args(id);
    let format = info.code.message_format();
    let arg_text = |depth: u32| -> Vec<Message> { args.iter().map(|a| arg(arena, source, a, depth)).collect() };
    match info.xml {
        // `XmlSyntaxDiagnosticInfo.GetMessage`: the XML code's message inside WRN_XMLParseError's, then the arguments
        Some(xml) => {
            let inner = composite_format(format, &[Message::from(xml.message_format().to_string())]);
            if args.is_empty() { inner } else { composite_format(&inner.text, &arg_text(depth)) }
        }
        None if format.is_empty() => Message::default(),
        // a message without arguments is not formatted: `{{` stays
        None if args.is_empty() => Message::from(format.to_string()),
        None => composite_format(format, &arg_text(depth)),
    }
}

/// An argument as `string.Format` with the invariant culture prints the object Roslyn passes.
fn arg(arena: &GreenArena, source: &[u8], a: &DiagArg, depth: u32) -> Message {
    let text = match *a {
        DiagArg::Str(s) => s.to_string(),
        DiagArg::TokenText(token) => match arena.token_source_range(token) {
            Some((start, width)) => {
                let halves = SourceHalves::of(source, start as usize, (start + width) as usize);
                let mut message = Message::default();
                if let Some(low) = halves.low {
                    message.push_lone_surrogate(low);
                }
                message.push_str(&String::from_utf8_lossy(&source[halves.middle]));
                if let Some(high) = halves.high {
                    message.push_lone_surrogate(high);
                }
                return message;
            }
            None => String::from_utf8_lossy(arena.token_text(token, source)).into_owned(),
        },
        DiagArg::Owned { start, len } => arena.interned(start, len).to_string(),
        DiagArg::Char(unit) => match char::from_u32(unit as u32) {
            Some(ch) => ch.to_string(),
            None => {
                let mut message = Message::default();
                message.push_lone_surrogate(unit);
                return message;
            }
        },
        DiagArg::Int(n) => n.to_string(),
        DiagArg::Feature(id) => id.message().to_string(),
        DiagArg::Version(v) => v.to_display_string().to_string(),
        DiagArg::Kind(k) => k.name().to_string(),
        DiagArg::Info(id) if depth < MAX_EMBEDDED => return message_at(arena, source, id, depth + 1),
        DiagArg::Info(_) => String::new(),
    };
    Message::from(text)
}

/// .NET composite formatting of `format` with `args`: `{index[,alignment][:format]}` items, `{{` and `}}` escapes. An
/// item .NET would reject (a bad index, an unclosed brace) is written as it is rather than failing.
pub fn composite_format(format: &str, args: &[Message]) -> Message {
    let mut out = Message { text: String::with_capacity(format.len() + 16), lone_surrogates: Vec::new() };
    let bytes = format.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' if bytes.get(i + 1) == Some(&b'{') => {
                out.push_str("{");
                i += 2;
            }
            b'}' if bytes.get(i + 1) == Some(&b'}') => {
                out.push_str("}");
                i += 2;
            }
            b'{' => match format[i + 1..].find('}') {
                Some(close) => {
                    let item = &format[i + 1..i + 1 + close];
                    match format_item(item, args) {
                        Some(text) => out.push_message(&text),
                        None => out.push_str(&format[i..i + 2 + close]),
                    }
                    i += close + 2;
                }
                None => {
                    out.push_str(&format[i..]);
                    break;
                }
            },
            _ => {
                let next = format[i..].find(['{', '}']).map_or(bytes.len(), |n| if n == 0 { i + 1 } else { i + n });
                out.push_str(&format[i..next]);
                i = next;
            }
        }
    }
    out
}

/// One item: `index[,alignment][:format]`; a numeric format (`X8`, `D`) applies to an integer argument.
fn format_item(item: &str, args: &[Message]) -> Option<Message> {
    let (spec, fmt) = match item.find(':') {
        Some(c) => (&item[..c], Some(&item[c + 1..])),
        None => (item, None),
    };
    let (index, alignment) = match spec.find(',') {
        Some(c) => (&spec[..c], Some(spec[c + 1..].trim().parse::<i32>().ok()?)),
        None => (spec, None),
    };
    let value = args.get(index.trim().parse::<usize>().ok()?)?;
    let mut text = match (fmt, value.text.parse::<i64>()) {
        (Some(f), Ok(n)) if f.starts_with(['X', 'x']) => {
            let width = f[1..].parse::<usize>().unwrap_or(0);
            let mut s = String::new();
            if f.starts_with('X') {
                let _ = write!(s, "{n:0width$X}");
            } else {
                let _ = write!(s, "{n:0width$x}");
            }
            Message::from(s)
        }
        (Some(f), Ok(n)) if f.starts_with(['D', 'd']) => {
            let width = f[1..].parse::<usize>().unwrap_or(0);
            Message::from(format!("{n:0width$}"))
        }
        _ => value.clone(),
    };
    if let Some(a) = alignment {
        let pad = (a.unsigned_abs() as usize).saturating_sub(text.text.chars().count());
        if a > 0 {
            let mut padded = Message::from(" ".repeat(pad));
            padded.push_message(&text);
            text = padded;
        } else {
            text.push_str(&" ".repeat(pad));
        }
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error_code::{ErrorCode, MessageID, XmlParseErrorCode};
    use crate::language_version::LanguageVersion;

    #[test]
    fn formats_as_dotnet_does() {
        let args = [Message::from("a".to_string()), Message::from("255".to_string())];
        assert_eq!(composite_format("x {0} y {1}", &args).text, "x a y 255");
        assert_eq!(composite_format("{{0}} {0}", &args).text, "{0} a");
        assert_eq!(composite_format("{1:X4}|{0,3}|{0,-3}|", &args).text, "00FF|  a|a  |");
        assert_eq!(composite_format("{2} {", &args).text, "{2} {");
    }

    #[test]
    fn messages_of_codes_features_and_xml() {
        let mut arena = GreenArena::new();
        let syntax = arena.make_diagnostic(0, 1, ErrorCode::ERR_SyntaxError, &[DiagArg::Str(";")]);
        assert_eq!(message(&arena, b"", syntax).text, "Syntax error, ';' expected");
        let feature = arena.make_diagnostic(
            0,
            1,
            ErrorCode::ERR_FeatureNotAvailableInVersion7,
            &[DiagArg::Feature(MessageID::IDS_FeatureRecords), DiagArg::Version(LanguageVersion::CSharp9)],
        );
        assert_eq!(
            message(&arena, b"", feature).text,
            "Feature 'records' is not available in C# 7.0. Please use language version 9.0 or greater."
        );
        let warning = arena.make_diagnostic(
            0,
            0,
            ErrorCode::WRN_ErrorOverride,
            &[DiagArg::Info(syntax), DiagArg::Int(ErrorCode::ERR_SyntaxError.number() as i64)],
        );
        assert_eq!(message(&arena, b"", warning).text, "Syntax error, ';' expected. See also error CS1003.");
        let xml = arena.make_xml_diagnostic(0, 0, XmlParseErrorCode::XML_RefUndefinedEntity_1, &[DiagArg::Str("nbsp")]);
        assert_eq!(
            message(&arena, b"", xml).text,
            "XML comment has badly formed XML -- 'Reference to undefined entity 'nbsp'.'"
        );
    }

    #[test]
    fn lone_surrogates_are_kept_beside_the_text() {
        let mut arena = GreenArena::new();
        // a code unit of the lexer, inside an XML message and embedded in another
        let half = arena.make_xml_diagnostic(0, 0, XmlParseErrorCode::XML_InvalidToken, &[DiagArg::Char(0xD83D)]);
        let prefix = "XML comment has badly formed XML -- 'The character(s) '";
        let text = message(&arena, b"", half);
        assert_eq!(text.text, format!("{prefix}\u{FFFD}' cannot be used at this location.'"));
        assert_eq!(text.lone_surrogates, [(prefix.len() as u32, 0xD83D)]);
        let embedded =
            arena.make_diagnostic(0, 0, ErrorCode::WRN_ErrorOverride, &[DiagArg::Info(half), DiagArg::Int(1570)]);
        assert_eq!(message(&arena, b"", embedded).lone_surrogates, [(prefix.len() as u32, 0xD83D)]);
        // tokens of the halves of a supplementary character
        let source = "a😀".as_bytes();
        let empty = crate::green::TriviaList::EMPTY;
        let bad = crate::syntax_kind::SyntaxKind::BadToken;
        let high = arena.create_token(empty, bad, bad, crate::green::TokenText::Source { start: 1, width: 2 }, empty);
        let low = arena.create_token(empty, bad, bad, crate::green::TokenText::Source { start: 3, width: 2 }, empty);
        let both = arena.make_diagnostic(
            0,
            0,
            ErrorCode::ERR_SyntaxError,
            &[DiagArg::TokenText(high), DiagArg::TokenText(low)],
        );
        let text = message(&arena, source, both);
        assert_eq!(text.text, "Syntax error, '\u{FFFD}' expected");
        assert_eq!(text.lone_surrogates, [(15, 0xD83D)]);
        let arg = |token| super::arg(&arena, source, &DiagArg::TokenText(token), 0);
        assert_eq!(arg(low).lone_surrogates, [(0, 0xDE00)]);
        assert_eq!(arg(high).lone_surrogates, [(0, 0xD83D)]);
    }
}
