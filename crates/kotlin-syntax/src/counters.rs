//! Event counters for profiling the Kotlin parser: how often the semantic layer, the chameleons and the KDoc lexer
//! run per token, beside the builder's own (`psi_builder::counters`). Built with `--features counters` they count per
//! thread (`snapshot`); without it `add` is an empty inline function and the parser is unchanged. `ksdump --bench`
//! prints them per file token.

pub use psi_builder::counters::ENABLED;

/// What is counted here (the builder's events are `psi_builder::counters::Counter`).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Counter {
    /// Tokens the KDoc lexer made.
    KDocTokens,
    /// Chameleons expanded, by kind.
    BlockChameleons,
    LambdaChameleons,
    KDocChameleons,
    KDocLinkChameleons,
    /// `SemanticWhitespaceAwarePsiBuilder.getTokenType()`: the parser's `tt()`, through the adapters.
    TokenType,
    /// `newlineBeforeCurrentToken()`.
    NewlineBefore,
    /// `SOFT_KEYWORD_TEXTS.get(text)`: a soft keyword looked up by an identifier's text.
    SoftKeywordLookup,
}

impl Counter {
    /// Every counter, in declaration order.
    pub const ALL: [Counter; 8] = [
        Counter::KDocTokens,
        Counter::BlockChameleons,
        Counter::LambdaChameleons,
        Counter::KDocChameleons,
        Counter::KDocLinkChameleons,
        Counter::TokenType,
        Counter::NewlineBefore,
        Counter::SoftKeywordLookup,
    ];
}

/// A counter of a Kotlin parse: the builder's or the parser's.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum AnyCounter {
    Builder(psi_builder::counters::Counter),
    Kotlin(Counter),
}

impl std::fmt::Debug for AnyCounter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnyCounter::Builder(c) => c.fmt(f),
            AnyCounter::Kotlin(c) => c.fmt(f),
        }
    }
}

/// Every counter of a Kotlin parse, in the order `ksdump --bench` prints them.
pub const ALL: [AnyCounter; 25] = {
    use AnyCounter::{Builder as B, Kotlin as K};
    use psi_builder::counters::Counter as P;
    [
        B(P::FileTokens),
        B(P::RelexedTokens),
        B(P::ReusedTokens),
        K(Counter::KDocTokens),
        K(Counter::BlockChameleons),
        K(Counter::LambdaChameleons),
        K(Counter::KDocChameleons),
        K(Counter::KDocLinkChameleons),
        K(Counter::TokenType),
        B(P::RawTokenType),
        B(P::LookAhead),
        B(P::LookAheadSteps),
        B(P::RawLookup),
        B(P::TokenText),
        K(Counter::NewlineBefore),
        B(P::Advance),
        B(P::Mark),
        B(P::Precede),
        B(P::Done),
        B(P::Drop),
        B(P::Rollback),
        B(P::RollbackLexemes),
        B(P::IndexOfSearch),
        K(Counter::SoftKeywordLookup),
        B(P::BalancedSkipTokens),
    ]
};

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

/// This thread's counts of every counter of a Kotlin parse, in `ALL` order (zeros without the feature).
pub fn snapshot() -> Vec<u64> {
    let builder = psi_builder::counters::snapshot();
    #[cfg(feature = "counters")]
    let own: Vec<u64> = COUNTS.with(|c| Counter::ALL.iter().map(|&k| c[k as usize].get()).collect());
    #[cfg(not(feature = "counters"))]
    let own = vec![0u64; Counter::ALL.len()];
    ALL.iter()
        .map(|c| match *c {
            AnyCounter::Builder(b) => builder[b as usize],
            AnyCounter::Kotlin(k) => own[k as usize],
        })
        .collect()
}
