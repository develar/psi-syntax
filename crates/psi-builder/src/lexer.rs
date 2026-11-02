//! What lexers share: the token list of a text (`Lexed`, what `TokenSequence.performLexing` makes), the decoding of a
//! byte-offset text in which an invalid UTF-8 byte is one character (`char_at`), the lookup of a code point in a
//! table of Unicode ranges (`in_ranges`), keyword tables compared a word at a time (`Keyword`) and byte scans eight
//! bytes at a time (`find_either`, `find_any3`).

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

    /// The heap bytes of the list's buffers, by capacity.
    pub fn heap_bytes(&self) -> usize {
        use crate::buffers::vec_bytes;
        vec_bytes(&self.types) + vec_bytes(&self.starts) + vec_bytes(&self.plain)
    }

    /// Empties the list, each buffer shrunk to its share of `shrink`.
    pub fn shrink(&mut self, shrink: crate::buffers::Shrink) {
        shrink.vec(&mut self.types);
        shrink.vec(&mut self.starts);
        shrink.vec(&mut self.plain);
    }
}

impl<T: Copy> Lexed<T> {
    /// `begin(len)`, then a writer that appends the tokens in batches (`fill` initializes its batch).
    #[inline]
    pub fn writer(&mut self, len: usize, fill: T) -> LexedWriter<'_, T> {
        self.begin(len);
        LexedWriter { lexed: self, kinds: [fill; BATCH], starts: [0; BATCH], plain: 0, n: 0 }
    }
}

/// The tokens a `LexedWriter` holds before it appends them: one word of `Lexed::plain` bits.
const BATCH: usize = 64;

/// Appends tokens to an emptied `Lexed` (`Lexed::writer`) as `Lexed::push` does, through a batch of 64 on the stack:
/// a token is two stores and a bit, and the batch is appended (two copies and one plain word) when it is full and at
/// `end`.
pub struct LexedWriter<'a, T: Copy> {
    lexed: &'a mut Lexed<T>,
    kinds: [T; BATCH],
    starts: [u32; BATCH],
    plain: u64,
    n: usize,
}

impl<T: Copy> LexedWriter<'_, T> {
    /// Appends a token starting at `start`; `plain` as `Lexed::is_plain` says.
    #[inline(always)]
    pub fn push(&mut self, kind: T, start: u32, plain: bool) {
        let i = self.n % BATCH;
        self.kinds[i] = kind;
        self.starts[i] = start;
        self.plain |= (plain as u64) << i;
        self.n = i + 1;
        if self.n == BATCH {
            self.flush();
        }
    }

    #[inline(never)]
    fn flush(&mut self) {
        let n = self.n;
        self.lexed.types.extend_from_slice(&self.kinds[..n]);
        self.lexed.starts.extend_from_slice(&self.starts[..n]);
        self.lexed.plain.push(self.plain);
        self.n = 0;
        self.plain = 0;
    }

    /// Ends the list at the text's length `len` (`Lexed::end`).
    #[inline]
    pub fn end(mut self, len: u32) {
        if self.n != 0 {
            self.flush();
        }
        self.lexed.starts.push(len);
    }
}

/// The decoded character at `pos`: its code point and its length in bytes. A byte that does not start a valid UTF-8
/// sequence is U+FFFD of length 1, as a decoder replaces it.
#[inline(always)]
pub fn char_at(text: &[u8], pos: usize) -> (u32, usize) {
    let b0 = text[pos];
    if b0 < 0x80 {
        return (b0 as u32, 1);
    }
    char_at_multibyte(text, pos, b0)
}

/// `char_at` of a byte above ASCII, out of line: source text is mostly ASCII.
#[cold]
#[inline(never)]
fn char_at_multibyte(text: &[u8], pos: usize, b0: u8) -> (u32, usize) {
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

/// A keyword of a language's perfect-hash keyword table: its token, length (at most 16 bytes) and text as two
/// little-endian words, the first 8 bytes zero padded and the last 8 (which overlap the first below 16 bytes, and are
/// 0 below 8), so that a candidate word of the text compares in one or two loads.
#[derive(Copy, Clone, Debug)]
pub struct Keyword<T> {
    pub kind: T,
    pub len: u8,
    head: u64,
    tail: u64,
}

impl<T: Copy> Keyword<T> {
    /// An empty slot (length 0): it matches no word.
    pub const fn empty(kind: T) -> Self {
        Keyword { kind, len: 0, head: 0, tail: 0 }
    }

    /// The keyword `text` (2 to 16 bytes) of token `kind`.
    pub const fn new(text: &[u8], kind: T) -> Self {
        let len = text.len();
        assert!(len >= 1 && len <= 16);
        let (mut head, mut tail) = (0u64, 0u64);
        let mut j = 0;
        while j < 8 && j < len {
            head |= (text[j] as u64) << (8 * j);
            j += 1;
        }
        if len >= 8 {
            j = 0;
            while j < 8 {
                tail |= (text[len - 8 + j] as u64) << (8 * j);
                j += 1;
            }
        }
        Keyword { kind, len: len as u8, head, tail }
    }

    /// Whether `text[start..end]` is this keyword's text.
    #[inline(always)]
    pub fn matches(&self, text: &[u8], start: usize, end: usize) -> bool {
        let len = end - start;
        if len != self.len as usize {
            return false;
        }
        if len >= 8 {
            return match (text.get(start..start + 8), text.get(end - 8..end)) {
                (Some(head), Some(tail)) => word(head) == self.head && word(tail) == self.tail,
                _ => false,
            };
        }
        match text.get(start..start + 8) {
            Some(head) => word(head) & ((1u64 << (len * 8)) - 1) == self.head,
            None => text[start..end].iter().zip(self.head.to_le_bytes()).all(|(&a, b)| a == b),
        }
    }
}

/// The little-endian word of the first eight bytes of `bytes` (at least eight).
#[inline(always)]
pub fn word(bytes: &[u8]) -> u64 {
    u64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]])
}

/// The high bit of each byte of `word` that equals `b`: exact for the lowest such byte (a borrow can mark a byte
/// above it, never one below).
#[inline(always)]
pub fn has_byte(word: u64, b: u8) -> u64 {
    const LO: u64 = 0x0101_0101_0101_0101;
    const HI: u64 = 0x8080_8080_8080_8080;
    let x = word ^ (LO * b as u64);
    x.wrapping_sub(LO) & !x & HI
}

/// The position of the first `a` or `b` in `text` from `pos`, else `text.len()`; eight bytes at a time.
#[inline]
pub fn find_either(text: &[u8], mut pos: usize, a: u8, b: u8) -> usize {
    while let Some(chunk) = text.get(pos..pos + 8) {
        let w = word(chunk);
        let found = has_byte(w, a) | has_byte(w, b);
        if found != 0 {
            return pos + (found.trailing_zeros() / 8) as usize;
        }
        pos += 8;
    }
    while text.get(pos).is_some_and(|&c| c != a && c != b) {
        pos += 1;
    }
    pos.min(text.len())
}

/// The position of the first `a`, `b` or `c` in `text` from `pos`, else `text.len()`; eight bytes at a time.
#[inline]
pub fn find_any3(text: &[u8], mut pos: usize, a: u8, b: u8, c: u8) -> usize {
    while let Some(chunk) = text.get(pos..pos + 8) {
        let w = word(chunk);
        let found = has_byte(w, a) | has_byte(w, b) | has_byte(w, c);
        if found != 0 {
            return pos + (found.trailing_zeros() / 8) as usize;
        }
        pos += 8;
    }
    while text.get(pos).is_some_and(|&x| x != a && x != b && x != c) {
        pos += 1;
    }
    pos.min(text.len())
}
