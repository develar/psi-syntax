//! The storage of `com.intellij.psi.tree.TokenSet`: a set of element types by `ElementType::index` (tokens below 256,
//! node types from 256), built at compile time. A language's own `TokenSet` wraps it, with `const` constructors over
//! its token and node types.

/// A set of element type indices below 512.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ElementTypeSet {
    bits: [u64; 8],
}

impl ElementTypeSet {
    /// `TokenSet.EMPTY`.
    pub const EMPTY: ElementTypeSet = ElementTypeSet { bits: [0; 8] };

    /// This set with `index` added.
    #[inline]
    pub const fn with(self, index: usize) -> ElementTypeSet {
        let mut bits = self.bits;
        bits[index / 64] |= 1 << (index % 64);
        ElementTypeSet { bits }
    }

    /// `TokenSet.orSet(sets...)`.
    pub const fn or_set(sets: &[ElementTypeSet]) -> ElementTypeSet {
        let mut bits = [0u64; 8];
        let mut i = 0;
        while i < sets.len() {
            let mut w = 0;
            while w < 8 {
                bits[w] |= sets[i].bits[w];
                w += 1;
            }
            i += 1;
        }
        ElementTypeSet { bits }
    }

    /// `TokenSet.andSet(a, b)`.
    pub const fn and_set(a: ElementTypeSet, b: ElementTypeSet) -> ElementTypeSet {
        let mut bits = [0u64; 8];
        let mut w = 0;
        while w < 8 {
            bits[w] = a.bits[w] & b.bits[w];
            w += 1;
        }
        ElementTypeSet { bits }
    }

    /// `TokenSet.andNot(a, b)`.
    pub const fn and_not(a: ElementTypeSet, b: ElementTypeSet) -> ElementTypeSet {
        let mut bits = [0u64; 8];
        let mut w = 0;
        while w < 8 {
            bits[w] = a.bits[w] & !b.bits[w];
            w += 1;
        }
        ElementTypeSet { bits }
    }

    /// Whether `index` is in the set.
    #[inline]
    pub const fn contains_index(&self, index: usize) -> bool {
        self.bits[index / 64] & (1 << (index % 64)) != 0
    }
}
