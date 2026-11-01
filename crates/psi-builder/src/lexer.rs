//! What lexers share: the token list of a text (`Lexed`, what `TokenSequence.performLexing` makes), the decoding of a
//! byte-offset text in which an invalid UTF-8 byte is one character (`char_at`), and the lookup of a code point in a
//! table of Unicode ranges (`in_ranges`).

/// The tokens of a text: types, start offsets (one more than tokens: the last is the text length) and, per token,
/// whether it started in the lexer's initial state with an empty state stack ("plain"), so that the text from it lexes
/// alone into the same tokens (a chameleon may then reuse them).
#[derive(Clone, Debug)]
pub struct Lexed<T> {
    pub types: Vec<T>,
    pub starts: Vec<u32>,
    plain: Vec<u64>,
}

impl<T> Default for Lexed<T> {
    fn default() -> Self {
        Lexed { types: Vec::new(), starts: Vec::new(), plain: Vec::new() }
    }
}

impl<T> Lexed<T> {
    /// The number of tokens.
    pub fn len(&self) -> usize {
        self.types.len()
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }

    /// Whether token `i` started in the lexer's initial state with an empty stack, so that the text from it lexes
    /// alone into the same tokens.
    pub fn is_plain(&self, i: usize) -> bool {
        self.plain.get(i / 64).is_some_and(|w| w & (1 << (i % 64)) != 0)
    }

    /// Clears the list for a text of `len` bytes and reserves room for its tokens (an estimate).
    #[inline]
    pub fn begin(&mut self, len: usize) {
        let estimate = len / 4 + 2;
        self.types.clear();
        self.starts.clear();
        self.plain.clear();
        self.types.reserve(estimate);
        self.starts.reserve(estimate + 1);
    }

    /// Appends a token starting at `start`; `plain` as `is_plain` says.
    #[inline(always)]
    pub fn push(&mut self, kind: T, start: u32, plain: bool) {
        let i = self.types.len();
        if i.is_multiple_of(64) {
            self.plain.push(0);
        }
        if plain && let Some(w) = self.plain.last_mut() {
            *w |= 1 << (i % 64);
        }
        self.types.push(kind);
        self.starts.push(start);
    }

    /// Ends the list at the text's length `len`.
    #[inline]
    pub fn end(&mut self, len: u32) {
        self.starts.push(len);
    }
}

/// The decoded character at `pos`: its code point and its length in bytes. A byte that does not start a valid UTF-8
/// sequence is U+FFFD of length 1, as a decoder replaces it.
#[inline]
pub fn char_at(text: &[u8], pos: usize) -> (u32, usize) {
    let b0 = text[pos];
    if b0 < 0x80 {
        return (b0 as u32, 1);
    }
    let cont = |i: usize| -> Option<u32> {
        let b = *text.get(pos + i)?;
        if b & 0xC0 == 0x80 { Some((b & 0x3F) as u32) } else { None }
    };
    let decoded = match b0 {
        0xC2..=0xDF => cont(1).map(|c1| ((((b0 & 0x1F) as u32) << 6) | c1, 2)),
        0xE0..=0xEF => match (cont(1), cont(2)) {
            (Some(c1), Some(c2)) => {
                let cp = (((b0 & 0x0F) as u32) << 12) | (c1 << 6) | c2;
                if cp >= 0x800 && !(0xD800..=0xDFFF).contains(&cp) { Some((cp, 3)) } else { None }
            }
            _ => None,
        },
        0xF0..=0xF4 => match (cont(1), cont(2), cont(3)) {
            (Some(c1), Some(c2), Some(c3)) => {
                let cp = (((b0 & 0x07) as u32) << 18) | (c1 << 12) | (c2 << 6) | c3;
                if (0x10000..=0x10FFFF).contains(&cp) { Some((cp, 4)) } else { None }
            }
            _ => None,
        },
        _ => None,
    };
    decoded.unwrap_or((0xFFFD, 1))
}

/// Whether `cp` lies in one of the sorted, disjoint `ranges`.
#[inline]
pub fn in_ranges(ranges: &[(u32, u32)], cp: u32) -> bool {
    match ranges.binary_search_by(|&(start, _)| start.cmp(&cp)) {
        Ok(_) => true,
        Err(0) => false,
        Err(i) => cp <= ranges[i - 1].1,
    }
}
