//! The stack a parse may take, measured where it recurses, as Roslyn's `StackGuard.EnsureSufficientExecutionStack`
//! measures it (IntelliJ's parsers have no bound but the thread's stack). A parse promises to fit `STACK` bytes of its
//! thread's stack beyond its caller's frames on every target and build, which a depth limit alone cannot keep: a
//! level's frames are the compiler's choice, and the deepest form at the limits took 902 KB on macOS arm64 and 1061 KB
//! on Windows x64 with the same code. So at every level it counts, a parser also compares the address of a local with
//! the one where its builder started (`StackGuard`): past `STACK_BUDGET` it takes the path it takes past its depth
//! limit, and the rest of `STACK` holds the work below the deepest level it counted (DESIGN, "Stack").
//!
//! Within one build the bound is deterministic: the stack between two frames is the sum of the frames of the calls
//! between them, the same for the same input. Across targets the frames differ, so the depth limit, a count, stays
//! the bound wherever it is reached first, which `tests/robustness.rs` of each language checks for every recursive form
//! up to a number of levels on every target its CI runs.

/// The stack a parse is promised to fit beyond its caller's frames, on every target and in every build with
/// optimizations (a debug build's frames are several times larger).
pub const STACK: usize = 1 << 20;

/// The stack a parse's levels may take from where its builder started (`StackGuard`): three quarters of
/// `STACK`, the rest for the work below the deepest level it counts (a token's lexing, a node's making, an error's
/// recovery, a lexer's own bounded recursion: C#'s documentation comments nested past their limit take about 53 KB on
/// macOS arm64, DESIGN "Stack").
pub const STACK_BUDGET: usize = STACK / 4 * 3;

/// The address of a local in the frame of the function this is inlined into: the stack grows down on every target
/// psi-syntax builds for, so a deeper frame has a lower address.
#[inline(always)]
pub fn here() -> usize {
    let marker = 0u8;
    std::hint::black_box(&marker) as *const u8 as usize
}

/// Where a builder's parse started on its thread's stack (`here` in the frame that starts it), and the address below
/// which its levels have taken more than `STACK_BUDGET`: a level compares an address with it, one comparison.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct StackGuard {
    start: usize,
    limit: usize,
}

impl Default for StackGuard {
    /// A guard that never runs out (no address is below 0): for a builder that is not parsing.
    fn default() -> Self {
        StackGuard { start: 0, limit: 0 }
    }
}

impl StackGuard {
    /// A guard started at the frame this is inlined into.
    #[inline(always)]
    pub fn here() -> Self {
        let start = here();
        StackGuard { start, limit: start.saturating_sub(STACK_BUDGET) }
    }

    /// Whether a frame at `address` (`here`) is past the budget.
    #[inline(always)]
    pub fn is_past(self, address: usize) -> bool {
        address < self.limit
    }

    /// Whether a frame at `address` holds `bytes` more below it within the budget.
    #[inline(always)]
    pub fn fits_below(self, address: usize, bytes: usize) -> bool {
        address.saturating_sub(bytes) >= self.limit
    }

    /// The bytes between the start and a frame at `address` (0 for `usize::MAX`, no frame).
    pub fn used_to(self, address: usize) -> usize {
        self.start.saturating_sub(address)
    }
}

/// The most a parse took (measurements, `ksdump`, `tests/robustness.rs`): the deepest level it counted and the most
/// stack its levels took from where their builder started (`StackGuard::used_to` the lowest frame); the level the
/// bounds stopped at, if they did, is counted too.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct StackUse {
    pub levels: u32,
    pub bytes: usize,
}

impl StackUse {
    /// The larger of each.
    pub fn max(self, other: StackUse) -> StackUse {
        StackUse { levels: self.levels.max(other.levels), bytes: self.bytes.max(other.bytes) }
    }

    /// Whether the stack, not the depth limit, stopped the parse.
    pub fn out_of_stack(self) -> bool {
        self.bytes > STACK_BUDGET
    }
}
