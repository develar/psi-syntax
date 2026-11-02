//! What a reusable parser keeps of its buffers from one file to the next, and how it lets go of them.
//!
//! A `Parser` clears its buffers per file and keeps their capacity, so a thread that parses many files allocates
//! only while its files grow. The capacity is then that of the largest file seen: a long-lived parser that met one
//! huge file holds that file's buffers for good (a C# parser 1.24 GB after dotnet/runtime's 24 MB `hugeexpr1.cs`).
//! So after each parse a parser whose buffers hold more than its retained bytes (`RETAINED_BYTES` by default) shrinks
//! them to half of that (`retained_after`), each buffer to the same share of its capacity (`Shrink`): the proportions
//! of the files that grew them stay, so the next files reuse them, and the half left free lets files of other
//! proportions grow a buffer without passing the bound again at once. A file that needs more grows its buffers again,
//! by doubling, as a fresh parser does.

use std::collections::HashMap;
use std::hash::BuildHasher;

/// The bytes a parser's buffers may keep from one parse to the next by default (`Parser::set_retained_bytes`): 64
/// MiB. A C# parser needs more for 20 of the 62 615 files of tools/csharp-oracle's four corpora (files of 1.0 MB and
/// more: a file over 100 KB needs 25 bytes of buffers a source byte, the median), a Kotlin or Java parser (14 and 18
/// bytes a source byte) for none of the community files; 16 threads keep at most 1 GiB where one C# parser kept 1.24 GB
/// (DESIGN, "Retained buffers").
pub const RETAINED_BYTES: usize = 64 << 20;

/// What the buffers are shrunk to when a parse leaves more than `retained` bytes in them: half of it.
pub fn retained_after(retained: usize) -> usize {
    retained / 2
}

/// A shrink of a parser's buffers holding `total` bytes to `keep` bytes: every buffer is emptied and keeps
/// `keep / total` of its capacity (rounded down), so the buffers hold at most `keep` bytes in all (and, for each
/// `nested` vector, the inline bytes of the one element it keeps past its share), in the proportions they had.
#[derive(Copy, Clone, Debug)]
pub struct Shrink {
    keep: usize,
    total: usize,
}

impl Shrink {
    /// The shrink of buffers of `total` bytes to `keep` (at most `total`).
    pub fn new(keep: usize, total: usize) -> Shrink {
        Shrink { keep: keep.min(total), total }
    }

    /// The share of `capacity` elements (or bytes) a buffer keeps.
    pub fn elements(self, capacity: usize) -> usize {
        if self.total == 0 {
            return capacity;
        }
        (capacity as u128 * self.keep as u128 / self.total as u128) as usize
    }

    /// Empties `v` and shrinks it to its share.
    pub fn vec<T>(self, v: &mut Vec<T>) {
        v.clear();
        v.shrink_to(self.elements(v.capacity()));
    }

    /// Empties `s` and shrinks it to its share.
    pub fn string(self, s: &mut String) {
        s.clear();
        s.shrink_to(self.elements(s.capacity()));
    }

    /// Empties `m` and shrinks it to its share.
    pub fn map<K: Eq + std::hash::Hash, V, S: BuildHasher>(self, m: &mut HashMap<K, V, S>) {
        m.clear();
        m.shrink_to(self.elements(m.capacity()));
    }

    /// Keeps the share of `v`'s elements, each a buffer that `shrink` shrinks in turn: the elements past the share
    /// are dropped, then `v` is shrunk to the share. The share counts elements, not bytes, and the first element
    /// often holds most of them (Kotlin's and Java's frames: the first is the file's builder), so unless the shrink
    /// keeps nothing, the first element stays even when the share of a few rounds down to none: shrunk to its own
    /// share, like every other buffer, instead of freed whole for the next file to grow again.
    pub fn nested<T>(self, v: &mut Vec<T>, mut shrink: impl FnMut(&mut T)) {
        let mut keep = self.elements(v.capacity());
        if keep == 0 && self.keep > 0 {
            keep = v.len().min(1);
        }
        v.truncate(keep);
        v.iter_mut().for_each(&mut shrink);
        v.shrink_to(keep);
    }
}

/// The heap bytes of `v`'s capacity (not of what its elements own).
pub fn vec_bytes<T>(v: &Vec<T>) -> usize {
    v.capacity() * size_of::<T>()
}

/// The heap bytes of `m`'s capacity: its entries and a control byte each (its table's own rounding aside).
pub fn map_bytes<K, V, S>(m: &HashMap<K, V, S>) -> usize {
    m.capacity() * (size_of::<(K, V)>() + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shrinks_in_proportion_and_never_past_the_bound() {
        let (mut a, mut b): (Vec<u64>, Vec<u8>) = (Vec::with_capacity(1000), Vec::with_capacity(4000));
        a.extend(0..10);
        let total = vec_bytes(&a) + vec_bytes(&b);
        let s = Shrink::new(total / 4, total);
        s.vec(&mut a);
        s.vec(&mut b);
        assert!(a.is_empty() && vec_bytes(&a) + vec_bytes(&b) <= total / 4);
        assert!((240..=250).contains(&a.capacity()) && (990..=1000).contains(&b.capacity()));
        let mut nested: Vec<Vec<u32>> = (0..8).map(|_| Vec::with_capacity(100)).collect();
        Shrink::new(0, 1).nested(&mut nested, |v| Shrink::new(0, 1).vec(v));
        assert_eq!(nested.capacity(), 0);
        let mut nested: Vec<Vec<u32>> = (0..8).map(|_| Vec::with_capacity(100)).collect();
        nested.shrink_to_fit();
        Shrink::new(1, 2).nested(&mut nested, |v| Shrink::new(1, 2).vec(v));
        assert_eq!(nested.len(), 4);
        assert!(nested.iter().all(|v| v.capacity() <= 50));
        // a share of 4 elements under 1/4 keeps the first, shrunk to its share, rather than none
        let mut nested: Vec<Vec<u32>> = (0..4).map(|_| Vec::with_capacity(100)).collect();
        nested.shrink_to_fit();
        Shrink::new(1, 10).nested(&mut nested, |v| Shrink::new(1, 10).vec(v));
        assert_eq!(nested.len(), 1);
        assert!(nested[0].capacity() <= 10);
    }
}
