//! `SlidingTextWindow.cs`: the lexer's view of the text, a position and the characters around it. Roslyn's window is a
//! chunk of UTF-16 code units copied out of a `SourceText`; here it is the file's UTF-8 bytes, read as the UTF-16 code
//! units Roslyn's char-based lexer sees:
//!
//! - a character below U+0080 is 1 byte, below U+0800 2, any other BMP character 3: each one code unit;
//! - a supplementary character (4 bytes) is two code units, a high and a low surrogate (category `Surrogate`, in no
//!   class of `character_info`), each 2 of its bytes wide: the position between them is the character's offset + 2, as
//!   the oracle maps Roslyn's positions (`tools/csharp-oracle/README.md`, "Offsets");
//! - a byte that does not start valid UTF-8 (only fuzz and robustness inputs: the oracle dumps valid UTF-8 only) is one
//!   code unit U+FFFD, 1 byte wide, so any bytes are lexed losslessly.
//!
//! Positions are byte offsets into the whole file (a nested lexer's window is a range of it), widths are bytes, and
//! `peek_char_at(n)` counts code units as `PeekChar(n)` does. The window's chunking (`ReadChunkAt`, `DefaultWindowLength`)
//! is not ported: it decides only where Roslyn reads from, never what it reads.

/// `InvalidCharacter`: what `peek_char` answers at the end (and for a real U+FFFF, `IsReallyAtEnd` tells them apart).
pub const INVALID_CHARACTER: u16 = 0xFFFF;

/// The code unit of a byte that starts no valid UTF-8 sequence.
const REPLACEMENT_CHARACTER: u16 = 0xFFFD;

/// `SlidingTextWindow`.
#[derive(Clone, Copy)]
pub struct SlidingTextWindow<'t> {
    /// `Text`: the whole file (the window is `start..text_end` of it).
    text: &'t [u8],
    /// The start of the window's text (`Text` starts at 0 in Roslyn; a nested lexer's text is a range of the file).
    start: u32,
    /// `_textEnd`.
    text_end: u32,
    /// `_positionInText`.
    position_in_text: u32,
}

impl<'t> SlidingTextWindow<'t> {
    /// `SlidingTextWindow(text)` over `text[start..end]`.
    pub fn new(text: &'t [u8], start: u32, end: u32) -> SlidingTextWindow<'t> {
        let end = end.min(text.len() as u32);
        let start = start.min(end);
        SlidingTextWindow { text, start, text_end: end, position_in_text: start }
    }

    /// `Text`: the bytes positions index.
    #[inline]
    pub fn text(&self) -> &'t [u8] {
        self.text
    }

    /// The start of the window's text (`0` of Roslyn's `Text`).
    #[inline]
    pub fn text_start(&self) -> u32 {
        self.start
    }

    /// `Text.Length` as an offset: the end of the window's text.
    #[inline]
    pub fn text_end(&self) -> u32 {
        self.text_end
    }

    /// `Position`.
    #[inline(always)]
    pub fn position(&self) -> u32 {
        self.position_in_text
    }

    /// `Reset(position)`.
    #[inline]
    pub fn reset(&mut self, position: u32) {
        self.position_in_text = position.min(self.text_end);
    }

    /// `IsReallyAtEnd()`.
    #[inline(always)]
    pub fn is_really_at_end(&self) -> bool {
        self.position_in_text >= self.text_end
    }

    /// `AdvanceChar()`: past one code unit.
    #[inline(always)]
    pub fn advance_char(&mut self) {
        let p = self.position_in_text as usize;
        if p < self.text_end as usize && self.text[p] < 0x80 {
            self.position_in_text += 1;
        } else {
            self.position_in_text = self.next_position(self.position_in_text);
        }
    }

    /// `AdvanceChar(n)`: past `n` code units.
    #[inline]
    pub fn advance_char_n(&mut self, n: u32) {
        for _ in 0..n {
            self.advance_char();
        }
    }

    /// `TryAdvance(c)`.
    #[inline(always)]
    pub fn try_advance(&mut self, c: u8) -> bool {
        debug_assert!(c < 0x80);
        let p = self.position_in_text as usize;
        if p < self.text_end as usize && self.text[p] == c {
            self.position_in_text += 1;
            return true;
        }
        false
    }

    /// `AdvancePastNewLine()`.
    pub fn advance_past_new_line(&mut self) {
        let width = self.get_new_line_width();
        self.advance_char_n(width);
    }

    /// `GetNewLineWidth()`: 2 for `\r\n`, else 1 (code units).
    pub fn get_new_line_width(&self) -> u32 {
        get_new_line_width(self.peek_char(), self.peek_char_at(1))
    }

    /// `NextChar()`.
    #[inline]
    pub fn next_char(&mut self) -> u16 {
        let c = self.peek_char();
        if c != INVALID_CHARACTER {
            self.advance_char();
        }
        c
    }

    /// `PeekChar()`: the code unit at the position, `INVALID_CHARACTER` at the end.
    #[inline(always)]
    pub fn peek_char(&self) -> u16 {
        let p = self.position_in_text as usize;
        if p < self.text_end as usize {
            let b = self.text[p];
            if b < 0x80 {
                return b as u16;
            }
            return self.unit_at(p).0;
        }
        INVALID_CHARACTER
    }

    /// `PeekChar(delta)`: the code unit `delta` units after the position.
    #[inline]
    pub fn peek_char_at(&self, delta: u32) -> u16 {
        let mut p = self.position_in_text;
        for _ in 0..delta {
            if p >= self.text_end {
                return INVALID_CHARACTER;
            }
            p = self.next_position(p);
        }
        self.char_at(p)
    }

    /// `PreviousChar()` (`PeekChar(-1)`): the code unit before the position (`INVALID_CHARACTER` at the window's
    /// start).
    pub fn previous_char(&self) -> u16 {
        match self.previous_position(self.position_in_text) {
            Some(p) => self.char_at(p),
            None => INVALID_CHARACTER,
        }
    }

    /// `AdvanceIfMatches(desired)`: an ASCII `desired`.
    pub fn advance_if_matches(&mut self, desired: &[u8]) -> bool {
        let p = self.position_in_text as usize;
        let end = p + desired.len();
        if end <= self.text_end as usize && &self.text[p..end] == desired {
            self.position_in_text = end as u32;
            return true;
        }
        false
    }

    /// `GetText(startPosition, intern)`: the bytes from `start` to the position.
    #[inline]
    pub fn get_text(&self, start: u32) -> &'t [u8] {
        self.text.get(start as usize..self.position_in_text as usize).unwrap_or(&[])
    }

    /// `Text[position]`: the code unit at `position` (`INVALID_CHARACTER` outside the window).
    #[inline]
    pub fn char_at(&self, position: u32) -> u16 {
        let p = position as usize;
        if p < self.text_end as usize && p >= self.start as usize {
            let b = self.text[p];
            if b < 0x80 {
                return b as u16;
            }
            return self.unit_at(p).0;
        }
        INVALID_CHARACTER
    }

    /// The position after the code unit at `position`.
    #[inline]
    pub fn next_position(&self, position: u32) -> u32 {
        let p = position as usize;
        if p >= self.text_end as usize {
            return position;
        }
        let b = self.text[p];
        if b < 0x80 {
            return position + 1;
        }
        position + self.unit_at(p).1
    }

    /// The position of the code unit before `position`, `None` at the window's start.
    pub fn previous_position(&self, position: u32) -> Option<u32> {
        let start = self.start as usize;
        let p = (position as usize).min(self.text_end as usize);
        if p <= start {
            return None;
        }
        // the unit ending at p starts 1 to 3 bytes before it (a surrogate half is 2); the farthest first, so a
        // continuation byte inside a character is never taken for a unit of its own
        for back in (1..=3usize).rev() {
            if p < start + back {
                continue;
            }
            let q = p - back;
            if self.is_unit_start(q) && q as u32 + self.unit_at_or_ascii(q).1 == p as u32 {
                return Some(q as u32);
            }
        }
        Some(p as u32 - 1)
    }

    fn unit_at_or_ascii(&self, p: usize) -> (u16, u32) {
        if self.text[p] < 0x80 { (self.text[p] as u16, 1) } else { self.unit_at(p) }
    }

    /// Whether a code unit can start at `p`: a byte that starts a character, or the low half of a supplementary one.
    fn is_unit_start(&self, p: usize) -> bool {
        let b = self.text[p];
        !(0x80..0xC0).contains(&b) || self.low_surrogate_at(p).is_some() || self.sequence_at(p).is_none()
    }

    /// The code unit at `p` (a byte at or above 0x80, `p` inside the window) and its width in bytes.
    #[cold]
    fn unit_at(&self, p: usize) -> (u16, u32) {
        if let Some(low) = self.low_surrogate_at(p) {
            return (low, 2);
        }
        match self.sequence_at(p) {
            Some((cp, 4)) => {
                let high = (((cp - 0x10000) >> 10) + 0xD800) as u16;
                (high, 2)
            }
            Some((cp, len)) => (cp as u16, len),
            None => (REPLACEMENT_CHARACTER, 1),
        }
    }

    /// The low surrogate at `p` when `p` is 2 bytes into a valid 4-byte sequence of the window.
    fn low_surrogate_at(&self, p: usize) -> Option<u16> {
        let b = self.text[p];
        if b & 0xC0 != 0x80 || p < self.start as usize + 2 {
            return None;
        }
        match self.sequence_at(p - 2) {
            Some((cp, 4)) => Some((((cp - 0x10000) & 0x3FF) + 0xDC00) as u16),
            _ => None,
        }
    }

    /// The valid UTF-8 sequence starting at `p` inside the window: its code point and length.
    fn sequence_at(&self, p: usize) -> Option<(u32, u32)> {
        let end = self.text_end as usize;
        let t = self.text;
        let b0 = t[p] as u32;
        let cont = |i: usize| -> Option<u32> {
            let b = *t.get(p + i).filter(|_| p + i < end)?;
            (b & 0xC0 == 0x80).then_some(b as u32 & 0x3F)
        };
        match b0 {
            0x00..=0x7F => Some((b0, 1)),
            0xC2..=0xDF => Some((((b0 & 0x1F) << 6) | cont(1)?, 2)),
            0xE0..=0xEF => {
                let c1 = cont(1)?;
                if (b0 == 0xE0 && c1 < 0x20) || (b0 == 0xED && c1 >= 0x20) {
                    return None;
                }
                Some((((b0 & 0x0F) << 12) | (c1 << 6) | cont(2)?, 3))
            }
            0xF0..=0xF4 => {
                let c1 = cont(1)?;
                if (b0 == 0xF0 && c1 < 0x10) || (b0 == 0xF4 && c1 >= 0x10) {
                    return None;
                }
                Some((((b0 & 0x07) << 18) | (c1 << 12) | (cont(2)? << 6) | cont(3)?, 4))
            }
            _ => None,
        }
    }

    /// The code point at `position` (a surrogate pair as one, from its high half) and its width in bytes, for the
    /// messages that print a character (`ObjectDisplay.FormatLiteral`).
    pub fn code_point_at(&self, position: u32) -> (u32, u32) {
        let p = position as usize;
        if p >= self.text_end as usize {
            return (INVALID_CHARACTER as u32, 0);
        }
        if self.low_surrogate_at(p).is_some() {
            return (self.unit_at(p).0 as u32, 2);
        }
        match self.sequence_at(p) {
            Some((cp, len)) => (cp, len),
            None => (REPLACEMENT_CHARACTER as u32, 1),
        }
    }

    /// The length in UTF-16 code units of `start..end`.
    pub fn utf16_len(&self, start: u32, end: u32) -> u32 {
        let mut p = start;
        let mut n = 0;
        while p < end && p < self.text_end {
            p = self.next_position(p);
            n += 1;
        }
        n
    }
}

/// `GetNewLineWidth(currentChar, nextChar)`.
pub fn get_new_line_width(current_char: u16, next_char: u16) -> u32 {
    if current_char == b'\r' as u16 && next_char == b'\n' as u16 { 2 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_units_of_utf8() {
        // a, é (2 bytes), € (3 bytes), 😀 (4 bytes: two surrogates), an invalid byte
        let text = "aé€😀".as_bytes().iter().copied().chain([0xFF, b'z']).collect::<Vec<u8>>();
        let mut w = SlidingTextWindow::new(&text, 0, text.len() as u32);
        let mut units = Vec::new();
        let mut positions = Vec::new();
        while !w.is_really_at_end() {
            positions.push(w.position());
            units.push(w.peek_char());
            w.advance_char();
        }
        assert_eq!(units, [0x61, 0xE9, 0x20AC, 0xD83D, 0xDE00, 0xFFFD, 0x7A]);
        assert_eq!(positions, [0, 1, 3, 6, 8, 10, 11]);
        w.reset(0);
        assert_eq!(w.peek_char_at(3), 0xD83D);
        assert_eq!(w.peek_char_at(4), 0xDE00);
        assert_eq!(w.peek_char_at(7), INVALID_CHARACTER);
        for (i, &p) in positions.iter().enumerate().skip(1) {
            assert_eq!(w.previous_position(p), Some(positions[i - 1]));
        }
        assert_eq!(w.utf16_len(0, text.len() as u32), 7);
    }
}
