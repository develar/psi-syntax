//! `Lexer.Interpolation.cs`.

use std::ops::Range;

/// `Lexer.Interpolation`: where an interpolation's braces and format colon are, as offsets into the file (Roslyn's are
/// into the text of the lexer that scanned the token, the token's own text when the parser rescans it).
#[derive(Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct Interpolation {
    pub open_brace_range: Range<u32>,
    /// Range of the format colon in the interpolation.  Empty if there is no colon.
    pub colon_range: Range<u32>,
    /// Range of the close brace.  Empty if there was no close brace (an error condition).
    pub close_brace_range: Range<u32>,
}

impl Interpolation {
    /// `HasColon`.
    pub fn has_colon(&self) -> bool {
        self.colon_range.start != self.colon_range.end
    }
}
