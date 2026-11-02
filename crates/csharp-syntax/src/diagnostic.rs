//! The text of a diagnostic: `DiagnosticInfo.GetMessage(CultureInfo.InvariantCulture)` (and `XmlSyntaxDiagnosticInfo`'s),
//! `string.Format` of the English message format with the arguments as .NET formats the objects Roslyn passes.

use std::fmt::Write;

use crate::green::{DiagArg, DiagnosticId, GreenArena};

/// How deep embedded diagnostics (`WRN_ErrorOverride`'s argument) are formatted; Roslyn nests one.
const MAX_EMBEDDED: u32 = 8;

/// `GetMessage(CultureInfo.InvariantCulture)` of the diagnostic `id` of `arena`, whose tokens index `source`.
pub fn message(arena: &GreenArena, source: &[u8], id: DiagnosticId) -> String {
    message_at(arena, source, id, 0)
}

fn message_at(arena: &GreenArena, source: &[u8], id: DiagnosticId, depth: u32) -> String {
    let info = arena.diagnostic(id);
    let args = arena.diagnostic_args(id);
    let format = info.code.message_format();
    let arg_text = |depth: u32| -> Vec<String> { args.iter().map(|a| arg(arena, source, a, depth)).collect() };
    match info.xml {
        // `XmlSyntaxDiagnosticInfo.GetMessage`: the XML code's message inside WRN_XMLParseError's, then the arguments
        Some(xml) => {
            let inner = composite_format(format, &[xml.message_format().to_string()]);
            if args.is_empty() { inner } else { composite_format(&inner, &arg_text(depth)) }
        }
        None if format.is_empty() => String::new(),
        // a message without arguments is not formatted: `{{` stays
        None if args.is_empty() => format.to_string(),
        None => composite_format(format, &arg_text(depth)),
    }
}

/// An argument as `string.Format` with the invariant culture prints the object Roslyn passes.
fn arg(arena: &GreenArena, source: &[u8], a: &DiagArg, depth: u32) -> String {
    match *a {
        DiagArg::Str(s) => s.to_string(),
        DiagArg::TokenText(token) => String::from_utf8_lossy(arena.token_text(token, source)).into_owned(),
        DiagArg::Owned { start, len } => arena.interned(start, len).to_string(),
        DiagArg::Int(n) => n.to_string(),
        DiagArg::Feature(id) => id.message().to_string(),
        DiagArg::Version(v) => v.to_display_string().to_string(),
        DiagArg::Kind(k) => k.name().to_string(),
        DiagArg::Info(id) if depth < MAX_EMBEDDED => message_at(arena, source, id, depth + 1),
        DiagArg::Info(_) => String::new(),
    }
}

/// .NET composite formatting of `format` with `args`: `{index[,alignment][:format]}` items, `{{` and `}}` escapes. An
/// item .NET would reject (a bad index, an unclosed brace) is written as it is rather than failing.
pub fn composite_format(format: &str, args: &[String]) -> String {
    let mut out = String::with_capacity(format.len() + 16);
    let bytes = format.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' if bytes.get(i + 1) == Some(&b'{') => {
                out.push('{');
                i += 2;
            }
            b'}' if bytes.get(i + 1) == Some(&b'}') => {
                out.push('}');
                i += 2;
            }
            b'{' => match format[i + 1..].find('}') {
                Some(close) => {
                    let item = &format[i + 1..i + 1 + close];
                    match format_item(item, args) {
                        Some(text) => out.push_str(&text),
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
fn format_item(item: &str, args: &[String]) -> Option<String> {
    let (spec, fmt) = match item.find(':') {
        Some(c) => (&item[..c], Some(&item[c + 1..])),
        None => (item, None),
    };
    let (index, alignment) = match spec.find(',') {
        Some(c) => (&spec[..c], Some(spec[c + 1..].trim().parse::<i32>().ok()?)),
        None => (spec, None),
    };
    let value = args.get(index.trim().parse::<usize>().ok()?)?;
    let mut text = match (fmt, value.parse::<i64>()) {
        (Some(f), Ok(n)) if f.starts_with(['X', 'x']) => {
            let width = f[1..].parse::<usize>().unwrap_or(0);
            let mut s = String::new();
            if f.starts_with('X') {
                let _ = write!(s, "{n:0width$X}");
            } else {
                let _ = write!(s, "{n:0width$x}");
            }
            s
        }
        (Some(f), Ok(n)) if f.starts_with(['D', 'd']) => {
            let width = f[1..].parse::<usize>().unwrap_or(0);
            format!("{n:0width$}")
        }
        _ => value.clone(),
    };
    if let Some(a) = alignment {
        let pad = (a.unsigned_abs() as usize).saturating_sub(text.chars().count());
        if a > 0 {
            text = " ".repeat(pad) + &text;
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
        let args = ["a".to_string(), "255".to_string()];
        assert_eq!(composite_format("x {0} y {1}", &args), "x a y 255");
        assert_eq!(composite_format("{{0}} {0}", &args), "{0} a");
        assert_eq!(composite_format("{1:X4}|{0,3}|{0,-3}|", &args), "00FF|  a|a  |");
        assert_eq!(composite_format("{2} {", &args), "{2} {");
    }

    #[test]
    fn messages_of_codes_features_and_xml() {
        let mut arena = GreenArena::new();
        let syntax = arena.make_diagnostic(0, 1, ErrorCode::ERR_SyntaxError, &[DiagArg::Str(";")]);
        assert_eq!(message(&arena, b"", syntax), "Syntax error, ';' expected");
        let feature = arena.make_diagnostic(
            0,
            1,
            ErrorCode::ERR_FeatureNotAvailableInVersion7,
            &[DiagArg::Feature(MessageID::IDS_FeatureRecords), DiagArg::Version(LanguageVersion::CSharp9)],
        );
        assert_eq!(
            message(&arena, b"", feature),
            "Feature 'records' is not available in C# 7.0. Please use language version 9.0 or greater."
        );
        let warning = arena.make_diagnostic(
            0,
            0,
            ErrorCode::WRN_ErrorOverride,
            &[DiagArg::Info(syntax), DiagArg::Int(ErrorCode::ERR_SyntaxError.number() as i64)],
        );
        assert_eq!(message(&arena, b"", warning), "Syntax error, ';' expected. See also error CS1003.");
        let xml = arena.make_xml_diagnostic(0, 0, XmlParseErrorCode::XML_RefUndefinedEntity_1, &[DiagArg::Str("nbsp")]);
        assert_eq!(
            message(&arena, b"", xml),
            "XML comment has badly formed XML -- 'Reference to undefined entity 'nbsp'.'"
        );
    }
}
