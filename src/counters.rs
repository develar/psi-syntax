//! Event counters for profiling the parser: how often the builder's cursor, markers and lexers run per token.
//! Built with `--features counters` they count per thread (`snapshot`); without it `add` is an empty inline function
//! and the parser is unchanged. `ksdump --bench` prints them per file token.

/// What is counted.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Counter {
    /// Tokens the Kotlin lexer made for whole files.
    FileTokens,
    /// Tokens lexed again for a chameleon (a block or lambda whose file tokens could not be reused, a markdown link).
    RelexedTokens,
    /// File tokens copied into a chameleon's builder instead of lexing its text again.
    ReusedTokens,
    /// Tokens the KDoc lexer made.
    KDocTokens,
    /// Chameleons expanded, by kind.
    BlockChameleons,
    LambdaChameleons,
    KDocChameleons,
    KDocLinkChameleons,
    /// `SemanticWhitespaceAwarePsiBuilder.getTokenType()`: the parser's `tt()`, through the adapters.
    TokenType,
    /// `PsiBuilderImpl.getTokenType()`.
    RawTokenType,
    /// `PsiBuilderImpl.lookAhead(steps)`, and the steps asked for.
    LookAhead,
    LookAheadSteps,
    /// `PsiBuilderImpl.rawLookup(steps)`.
    RawLookup,
    /// `PsiBuilderImpl.getTokenText()` (as text or bytes).
    TokenText,
    /// `newlineBeforeCurrentToken()`.
    NewlineBefore,
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
    /// `SOFT_KEYWORD_TEXTS.get(text)`: a soft keyword looked up by an identifier's text.
    SoftKeywordLookup,
    /// Tokens `advanceBalancedBlock` stepped over (inside a block or lambda that a later expansion parses again).
    BalancedSkipTokens,
}

impl Counter {
    /// Every counter, in declaration order.
    pub const ALL: [Counter; 25] = [
        Counter::FileTokens,
        Counter::RelexedTokens,
        Counter::ReusedTokens,
        Counter::KDocTokens,
        Counter::BlockChameleons,
        Counter::LambdaChameleons,
        Counter::KDocChameleons,
        Counter::KDocLinkChameleons,
        Counter::TokenType,
        Counter::RawTokenType,
        Counter::LookAhead,
        Counter::LookAheadSteps,
        Counter::RawLookup,
        Counter::TokenText,
        Counter::NewlineBefore,
        Counter::Advance,
        Counter::Mark,
        Counter::Precede,
        Counter::Done,
        Counter::Drop,
        Counter::Rollback,
        Counter::RollbackLexemes,
        Counter::IndexOfSearch,
        Counter::SoftKeywordLookup,
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
