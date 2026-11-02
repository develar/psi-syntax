//! `CharacterInfo.cs` (`SyntaxFacts`' character classes) and the `UnicodeCharacterUtilities.cs` it calls, in their
//! order, over UTF-16 code units (`u16`): what `CharUnicodeInfo.GetUnicodeCategory(char)` answers comes from the
//! generated `unicode` tables (a surrogate is in none of them), ASCII from the hand rules.

use crate::unicode;

/// `IsHexDigit`.
#[inline]
pub fn is_hex_digit(c: u16) -> bool {
    (c >= b'0' as u16 && c <= b'9' as u16)
        || (c >= b'A' as u16 && c <= b'F' as u16)
        || (c >= b'a' as u16 && c <= b'f' as u16)
}

/// `IsBinaryDigit`.
#[inline]
pub fn is_binary_digit(c: u16) -> bool {
    c == b'0' as u16 || c == b'1' as u16
}

/// `IsDecDigit`.
#[inline]
pub fn is_dec_digit(c: u16) -> bool {
    c >= b'0' as u16 && c <= b'9' as u16
}

/// `HexValue`.
#[inline]
pub fn hex_value(c: u16) -> u32 {
    debug_assert!(is_hex_digit(c));
    if is_dec_digit(c) { (c - b'0' as u16) as u32 } else { ((c & 0xdf) as u32).wrapping_sub(b'A' as u32) + 10 }
}

/// `BinaryValue`.
#[inline]
pub fn binary_value(c: u16) -> u32 {
    debug_assert!(is_binary_digit(c));
    (c - b'0' as u16) as u32
}

/// `DecValue`.
#[inline]
pub fn dec_value(c: u16) -> u32 {
    debug_assert!(is_dec_digit(c));
    (c - b'0' as u16) as u32
}

/// `IsWhitespace`: space, tab, vertical tab, form feed, U+00A0, U+FEFF, U+001A and the space separators above 255.
#[inline]
pub fn is_whitespace(ch: u16) -> bool {
    ch == b' ' as u16
        || ch == b'\t' as u16
        || ch == 0x0B
        || ch == 0x0C
        || ch == 0x00A0 // NO-BREAK SPACE
        // The native compiler, in ScanToken, recognized both the byte-order marker '﻿' as well as ^Z '\u001A' as
        // whitespace, although this is not to spec since neither of these are in Zs. For the sake of compatibility, we
        // recognize them both here.
        || ch == 0xFEFF
        || ch == 0x001A
        || (ch > 255 && in_ranges(&unicode::SPACE_SEPARATOR, ch))
}

/// `IsNewLine`: CR, LF, U+0085, U+2028, U+2029.
#[inline]
pub fn is_new_line(ch: u16) -> bool {
    ch == b'\r' as u16 || ch == b'\n' as u16 || ch == 0x0085 || ch == 0x2028 || ch == 0x2029
}

/// `IsIdentifierStartCharacter` (`UnicodeCharacterUtilities.IsIdentifierStartCharacter`).
#[inline]
pub fn is_identifier_start_character(ch: u16) -> bool {
    if ch < 0x80 {
        return unicode::ASCII_IDENTIFIER_START[ch as usize];
    }
    in_ranges(&unicode::IDENTIFIER_START, ch)
}

/// `IsIdentifierPartCharacter` (`UnicodeCharacterUtilities.IsIdentifierPartCharacter`).
#[inline]
pub fn is_identifier_part_character(ch: u16) -> bool {
    if ch < 0x80 {
        return unicode::ASCII_IDENTIFIER_PART[ch as usize];
    }
    in_ranges(&unicode::IDENTIFIER_PART, ch)
}

/// `UnicodeCharacterUtilities.IsFormattingChar(char)`: Cf (none in ASCII).
#[inline]
pub fn is_formatting_char(ch: u16) -> bool {
    ch > 127 && in_ranges(&unicode::FORMATTING, ch)
}

/// `IsNonAsciiQuotationMark`: U+2018, U+2019, U+201C, U+201D.
pub fn is_non_ascii_quotation_mark(ch: u16) -> bool {
    matches!(ch, 0x2018 | 0x2019 | 0x201C | 0x201D)
}

/// `ObjectDisplay.NeedsEscaping(GetUnicodeCategory(...))` of a code point that is not a surrogate code unit.
pub fn needs_escaping(cp: u32) -> bool {
    in_ranges_u32(&unicode::NEEDS_ESCAPING, cp)
}

/// Whether `ch` is in the sorted disjoint inclusive `ranges`.
#[inline]
fn in_ranges(ranges: &[(u16, u16)], ch: u16) -> bool {
    let i = ranges.partition_point(|&(_, end)| end < ch);
    i < ranges.len() && ranges[i].0 <= ch
}

fn in_ranges_u32(ranges: &[(u32, u32)], cp: u32) -> bool {
    let i = ranges.partition_point(|&(_, end)| end < cp);
    i < ranges.len() && ranges[i].0 <= cp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes() {
        assert!(is_identifier_start_character(b'a' as u16) && !is_identifier_start_character(b'1' as u16));
        assert!(is_identifier_start_character(0x0416)); // Ж
        assert!(!is_identifier_start_character(0xD83D)); // a surrogate
        assert!(is_identifier_part_character(0x200B) && is_formatting_char(0x200B));
        assert!(is_whitespace(0xFEFF) && is_whitespace(0x3000) && !is_whitespace(0x0085));
        assert!(is_new_line(0x2028) && !is_new_line(0x000B));
        assert!(needs_escaping(0x01) && needs_escaping(0x2028) && !needs_escaping(b'a' as u32));
        assert!(!needs_escaping(0x1F600) && needs_escaping(0xE0080));
        assert_eq!(hex_value(b'f' as u16), 15);
    }
}
