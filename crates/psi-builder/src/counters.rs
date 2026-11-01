//! Event counters for profiling the builder: how often its cursor, markers and the parse driver's lexing run per
//! token. Built with `--features counters` they count per thread (`snapshot`); without it `add` is an empty inline
//! function and the builder is unchanged. A language crate counts its own events beside these (Kotlin's
//! `kotlin_syntax::counters`); `ksdump --bench` prints both per file token.

/// What is counted.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Counter {
    /// Tokens the language's lexer made for whole files.
    FileTokens,
    /// Tokens lexed again for a chameleon (a body whose file tokens could not be reused, Kotlin's markdown link).
    RelexedTokens,
    /// File tokens copied into a chameleon's builder instead of lexing its text again.
    ReusedTokens,
    /// `PsiBuilderImpl.getTokenType()`.
    RawTokenType,
    /// `PsiBuilderImpl.lookAhead(steps)`, and the steps asked for.
    LookAhead,
    LookAheadSteps,
    /// `PsiBuilderImpl.rawLookup(steps)`.
    RawLookup,
    /// `PsiBuilderImpl.getTokenText()` (as text or bytes).
    TokenText,
    /// `PsiBuilderImpl.advanceLexer()`.
    Advance,
    /// Markers: `mark`, `precede`, done (any form), `drop`, `rollbackTo` and the lexemes it rewinds.
    Mark,
    Precede,
    Done,
    Drop,
    Rollback,
    RollbackLexemes,
    /// `MarkerProduction.indexOf` beyond its linear window (the binary search over the production).
    IndexOfSearch,
    /// Tokens a balanced block skip stepped over (inside a block or lambda that a later expansion parses again).
    BalancedSkipTokens,
}

impl Counter {
    /// Every counter, in declaration order.
    pub const ALL: [Counter; 17] = [
        Counter::FileTokens,
        Counter::RelexedTokens,
        Counter::ReusedTokens,
        Counter::RawTokenType,
        Counter::LookAhead,
        Counter::LookAheadSteps,
        Counter::RawLookup,
        Counter::TokenText,
        Counter::Advance,
        Counter::Mark,
        Counter::Precede,
        Counter::Done,
        Counter::Drop,
        Counter::Rollback,
        Counter::RollbackLexemes,
        Counter::IndexOfSearch,
        Counter::BalancedSkipTokens,
    ];
}

/// Whether the counters are compiled in.
pub const ENABLED: bool = cfg!(feature = "counters");

#[cfg(feature = "counters")]
thread_local! {
    static COUNTS: [std::cell::Cell<u64>; Counter::ALL.len()] = const { [const { std::cell::Cell::new(0) }; Counter::ALL.len()] };
}

/// Adds `n` to this thread's counter.
#[inline(always)]
pub fn add(counter: Counter, n: u64) {
    #[cfg(feature = "counters")]
    COUNTS.with(|c| {
        let cell = &c[counter as usize];
        cell.set(cell.get() + n);
    });
    #[cfg(not(feature = "counters"))]
    let _ = (counter, n);
}

/// This thread's counts, in `Counter::ALL` order (zeros without the feature).
pub fn snapshot() -> Vec<u64> {
    #[cfg(feature = "counters")]
    {
        COUNTS.with(|c| Counter::ALL.iter().map(|&k| c[k as usize].get()).collect())
    }
    #[cfg(not(feature = "counters"))]
    {
        vec![0; Counter::ALL.len()]
    }
}
