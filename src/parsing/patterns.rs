//! The token-stream patterns of the Kotlin parser: `TokenStreamPredicate`, `AbstractTokenStreamPredicate` (`or`),
//! the `At` / `AtSet` predicates of `AbstractKotlinParsing`, `TokenStreamPattern`, `AbstractTokenStreamPattern`,
//! `FirstBefore` and `LastBefore`. `AbstractKotlinParsing.matchTokenStreamPredicate` drives a pattern over the tokens
//! ahead; a predicate asks the parser (`at`, `atSet`, `lookahead`) about the current token, so it receives the parser.

use crate::parsing::abstract_parsing::AbstractKotlinParsing;
use crate::tokens::{TokenKind, TokenSet};

/// `TokenStreamPredicate` over the parser type `P`.
pub enum TokenStreamPredicate<P> {
    /// `AbstractKotlinParsing.At(lookFor, topLevelOnly)`: `(topLevel || !topLevelOnly) && at(lookFor)`.
    At { look_for: TokenKind, top_level_only: bool },
    /// `AbstractKotlinParsing.AtSet(lookFor, topLevelOnly)`: `(topLevel || !atSet(topLevelOnly)) && atSet(lookFor)`.
    AtSet { look_for: TokenSet, top_level_only: TokenSet },
    /// An anonymous `AbstractTokenStreamPredicate`: `matching(parser, topLevel)`.
    Custom(fn(&mut P, bool) -> bool),
    /// `AbstractTokenStreamPredicate.or(other)`.
    Or(Box<TokenStreamPredicate<P>>, Box<TokenStreamPredicate<P>>),
}

impl<P> TokenStreamPredicate<P> {
    /// `new At(lookFor)`: top level only.
    pub fn at(look_for: TokenKind) -> Self {
        TokenStreamPredicate::At { look_for, top_level_only: true }
    }

    /// `new AtSet(lookFor)`: `topLevelOnly = lookFor`.
    pub fn at_set(look_for: TokenSet) -> Self {
        TokenStreamPredicate::AtSet { look_for, top_level_only: look_for }
    }

    /// `or(other)`.
    pub fn or(self, other: TokenStreamPredicate<P>) -> Self {
        TokenStreamPredicate::Or(Box::new(self), Box::new(other))
    }

    /// `matching(topLevel)`.
    pub fn matching<'b>(&self, parser: &mut P, top_level: bool) -> bool
    where
        P: AbstractKotlinParsing<'b>,
    {
        match self {
            TokenStreamPredicate::At { look_for, top_level_only } => {
                (top_level || !top_level_only) && parser.at(*look_for)
            }
            TokenStreamPredicate::AtSet { look_for, top_level_only } => {
                (top_level || !parser.at_set(top_level_only)) && parser.at_set(look_for)
            }
            TokenStreamPredicate::Custom(matching) => matching(parser, top_level),
            TokenStreamPredicate::Or(first, other) => {
                if first.matching(parser, top_level) {
                    return true;
                }
                other.matching(parser, top_level)
            }
        }
    }
}

/// `TokenStreamPattern` over the parser type `P`.
pub trait TokenStreamPattern<P> {
    /// `processToken(offset, topLevel)`: called on each token; `true` stops.
    fn process_token(&mut self, parser: &mut P, offset: i32, top_level: bool) -> bool;

    /// `result()`: where the pattern matched, -1 if it did not.
    fn result(&self) -> i32;

    /// `isTopLevel(...)` (`AbstractTokenStreamPattern`): all counts zero.
    fn is_top_level(
        &self,
        open_angle_brackets: i32,
        open_brackets: i32,
        open_braces: i32,
        open_parentheses: i32,
    ) -> bool {
        open_braces == 0 && open_brackets == 0 && open_parentheses == 0 && open_angle_brackets == 0
    }

    /// `handleUnmatchedClosing(token)` (`AbstractTokenStreamPattern`).
    fn handle_unmatched_closing(&mut self, _token: TokenKind) -> bool {
        false
    }

    /// `reset()` (`AbstractTokenStreamPattern`).
    fn reset(&mut self);
}

/// `FirstBefore(lookFor, stopAt)`: the offset of the first `lookFor` before `stopAt`.
pub struct FirstBefore<P> {
    last_occurrence: i32,
    look_for: TokenStreamPredicate<P>,
    stop_at: TokenStreamPredicate<P>,
}

impl<P> FirstBefore<P> {
    pub fn new(look_for: TokenStreamPredicate<P>, stop_at: TokenStreamPredicate<P>) -> Self {
        FirstBefore { last_occurrence: -1, look_for, stop_at }
    }

    /// `AbstractTokenStreamPattern.fail()`.
    pub fn fail(&mut self) {
        self.last_occurrence = -1;
    }
}

impl<'b, P: AbstractKotlinParsing<'b>> TokenStreamPattern<P> for FirstBefore<P> {
    fn process_token(&mut self, parser: &mut P, offset: i32, top_level: bool) -> bool {
        if self.look_for.matching(parser, top_level) {
            self.last_occurrence = offset;
            return true;
        }
        if self.stop_at.matching(parser, top_level) {
            return true;
        }
        false
    }

    fn result(&self) -> i32 {
        self.last_occurrence
    }

    fn reset(&mut self) {
        self.last_occurrence = -1;
    }
}

/// `LastBefore(lookFor, stopAt)`: the offset of the last `lookFor` before a top-level `stopAt`.
pub struct LastBefore<P> {
    last_occurrence: i32,
    dont_stop_right_after_occurrence: bool,
    look_for: TokenStreamPredicate<P>,
    stop_at: TokenStreamPredicate<P>,
    previous_look_for_result: bool,
}

impl<P> LastBefore<P> {
    /// `new LastBefore(lookFor, stopAt)`.
    pub fn new(look_for: TokenStreamPredicate<P>, stop_at: TokenStreamPredicate<P>) -> Self {
        Self::with_dont_stop_right_after_occurrence(look_for, stop_at, false)
    }

    /// The private three-argument constructor.
    fn with_dont_stop_right_after_occurrence(
        look_for: TokenStreamPredicate<P>,
        stop_at: TokenStreamPredicate<P>,
        dont_stop_right_after_occurrence: bool,
    ) -> Self {
        LastBefore {
            last_occurrence: -1,
            dont_stop_right_after_occurrence,
            look_for,
            stop_at,
            previous_look_for_result: false,
        }
    }
}

impl<'b, P: AbstractKotlinParsing<'b>> TokenStreamPattern<P> for LastBefore<P> {
    fn process_token(&mut self, parser: &mut P, offset: i32, top_level: bool) -> bool {
        let look_for_result = self.look_for.matching(parser, top_level);
        if look_for_result {
            self.last_occurrence = offset;
        }
        if self.stop_at.matching(parser, top_level)
            && top_level
            && (!self.dont_stop_right_after_occurrence || !self.previous_look_for_result)
        {
            return true;
        }
        self.previous_look_for_result = look_for_result;
        false
    }

    fn result(&self) -> i32 {
        self.last_occurrence
    }

    fn reset(&mut self) {
        self.last_occurrence = -1;
        self.previous_look_for_result = false;
    }
}
