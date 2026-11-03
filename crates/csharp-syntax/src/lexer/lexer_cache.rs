//! `LexerCache.cs`: the token map of `LookupToken` (Roslyn's `TextKeyedCache<SyntaxToken>`), which `QuickScanner`
//! consults. Roslyn's map holds the green token itself, shared by every place the same text recurs; a token here
//! records its source offset, so the map holds what the token is made of instead (`QuickToken`: its kind, contextual
//! kind and widths, and the kinds and widths of its trivia), and a hit makes a new token of it at the position
//! (`GreenArena::quick_token`). A token is the same function of its text wherever the quick scanner accepts it (that
//! is what Roslyn's sharing relies on), so the map changes no tree; it only skips `LexSyntaxToken`. Only tokens
//! without diagnostics or values whose trivia are whitespace and ends of line are kept.
//!
//! The map is direct mapped (a new text replaces the one in its slot, as `TextKeyedCache`'s local table does) and
//! belongs to the `Parser`'s lexer buffers, so it stays warm from one file to the next: 4096 slots of 72 bytes (288 KB),
//! allocated by the first token added. The lexers of interpolated string holes have their own (`LexerBuffers` of
//! each nesting level).

use crate::syntax_kind::SyntaxKind;

/// `MaxCachedTokenSize`: the longest text the quick scanner looks at.
pub(crate) const MAX_CACHED_TOKEN_SIZE: usize = 42;

/// The most trivia of a cached token (leading and trailing together).
pub(crate) const MAX_TRIVIA: usize = 8;

/// The slots of the map (a power of two).
const SLOTS_BITS: u32 = 12;
const SLOTS: usize = 1 << SLOTS_BITS;

/// A trivia of a cached token: `EOL_BIT` for `EndOfLineTrivia` (else `WhitespaceTrivia`) and its width.
pub(crate) const EOL_BIT: u8 = 0x80;

/// What a cached token is made of.
#[derive(Copy, Clone, Debug)]
pub(crate) struct QuickToken {
    pub(crate) kind: SyntaxKind,
    pub(crate) contextual: SyntaxKind,
    /// The token's width (its text after the leading trivia).
    pub(crate) width: u8,
    pub(crate) leading: u8,
    pub(crate) trailing: u8,
    /// `leading` then `trailing` trivia, each `EOL_BIT | width` or a whitespace's width.
    pub(crate) trivia: [u8; MAX_TRIVIA],
}

/// The 8-byte words of a text of at most `MAX_CACHED_TOKEN_SIZE` bytes, zero after its end.
const WORDS: usize = MAX_CACHED_TOKEN_SIZE.div_ceil(8);

/// A text the map is searched with: its words, length and hash (`textBuffer`, `hashCode`).
#[derive(Copy, Clone)]
pub(crate) struct Key {
    words: [u64; WORDS],
    len: u32,
    hash: u64,
}

impl Key {
    /// The key of `text[start..start + len]` (`len` at most `MAX_CACHED_TOKEN_SIZE`).
    #[inline]
    pub(crate) fn new(text: &[u8], start: usize, len: usize) -> Key {
        let mut words = [0u64; WORDS];
        let mut hash = len as u64;
        let n = len.div_ceil(8).min(WORDS);
        for (k, word) in words.iter_mut().enumerate().take(n) {
            let at = start + 8 * k;
            let rem = len - 8 * k;
            let mut w = match text.get(at..at + 8) {
                Some(b) => u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
                None => {
                    let mut b = [0u8; 8];
                    for (i, x) in b.iter_mut().enumerate().take(rem.min(8)) {
                        *x = text.get(at + i).copied().unwrap_or(0);
                    }
                    u64::from_le_bytes(b)
                }
            };
            if rem < 8 {
                w &= (1u64 << (8 * rem)) - 1;
            }
            *word = w;
            hash = (hash ^ w).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
        Key { words, len: len as u32, hash }
    }
}

#[derive(Copy, Clone)]
struct Entry {
    words: [u64; WORDS],
    len: u32,
    token: QuickToken,
}

const EMPTY: Entry = Entry {
    words: [0; WORDS],
    len: 0,
    token: QuickToken {
        kind: SyntaxKind::None,
        contextual: SyntaxKind::None,
        width: 0,
        leading: 0,
        trailing: 0,
        trivia: [0; MAX_TRIVIA],
    },
};

/// `LexerCache` (its token map).
#[derive(Default)]
pub struct LexerCache {
    /// Empty until the first token is added.
    entries: Vec<Entry>,
    /// The lookups so far (tests: which modes scan).
    #[cfg(test)]
    pub(crate) lookups: std::cell::Cell<usize>,
    /// The lookups that found the text (tests: that hits happen).
    #[cfg(test)]
    pub(crate) hits: std::cell::Cell<usize>,
}

impl LexerCache {
    #[inline]
    fn slot(key: &Key) -> usize {
        (key.hash >> (64 - SLOTS_BITS)) as usize
    }

    /// `TokenMap.FindItem(textBuffer, hashCode)`.
    #[inline]
    pub(crate) fn find_item(&self, key: &Key) -> Option<QuickToken> {
        #[cfg(test)]
        self.lookups.set(self.lookups.get() + 1);
        let e = self.entries.get(Self::slot(key))?;
        let found = (e.len == key.len && e.words == key.words).then_some(e.token);
        #[cfg(test)]
        self.hits.set(self.hits.get() + found.is_some() as usize);
        found
    }

    /// `TokenMap.AddItem(textBuffer, hashCode, value)`.
    pub(crate) fn add_item(&mut self, key: &Key, token: QuickToken) {
        if key.len == 0 {
            return;
        }
        if self.entries.is_empty() {
            self.entries = vec![EMPTY; SLOTS];
        }
        let slot = Self::slot(key);
        if let Some(e) = self.entries.get_mut(slot) {
            *e = Entry { words: key.words, len: key.len, token };
        }
    }

    /// The heap bytes of the map.
    pub(crate) fn heap_bytes(&self) -> usize {
        self.entries.capacity() * size_of::<Entry>()
    }

    /// Frees the map when the buffers are shrunk (it is all or nothing; the next token added allocates it again).
    pub(crate) fn shrink(&mut self, shrink: psi_builder::buffers::Shrink) {
        if shrink.elements(self.entries.capacity()) < self.entries.capacity() {
            self.entries = Vec::new();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_is_72_bytes() {
        assert_eq!(size_of::<Entry>(), 72);
        assert_eq!(SLOTS * size_of::<Entry>(), 288 << 10);
    }
}
