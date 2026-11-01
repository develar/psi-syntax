//! A hand-written port of `Kotlin.flex` (JFlex, generated as `_JetLexer.java`) with the `KotlinLexer` (FlexAdapter)
//! behaviour: longest match, the earlier rule on a tie, the same lexical states and the same state stack, so every
//! input splits into exactly the tokens the compiler's lexer produces. Offsets are bytes; a byte that does not start a
//! valid UTF-8 sequence is one character (it lexes as `BAD_CHARACTER`, or as part of a string or comment).
//!
//! Rules are matched per state in the order of the spec; the comments name the spec rule each branch implements.

pub(crate) use psi_builder::lexer::char_at;
use psi_builder::lexer::in_ranges;

use crate::tokens::TokenKind;
use crate::tokens::TokenKind::*;
use crate::unicode::{KOTLIN_DIGIT, KOTLIN_LETTER};

/// The lexical states of Kotlin.flex (`%xstate` exclusive, `%state` inclusive of the unprefixed rules). The comment
/// states BLOCK_COMMENT and DOC_COMMENT are pushed and popped within one token (the comment ends at its closing `*/` or
/// at `<<EOF>>`), so the port scans a comment in place and never leaves the lexer in them.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum State {
    Initial,
    StringPrefix,
    String,
    RawString,
    ShortTemplateEntry,
    LongTemplateEntry,
    UnmatchedBacktick,
}

/// `_JetLexer.State`: a saved lexical state with its brace count and required interpolation prefix.
#[derive(Copy, Clone, Debug)]
struct Saved {
    state: State,
    l_brace_count: i32,
    required_interpolation_prefix: i32,
}

/// A token: its type and byte range in the lexed text.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Token {
    pub kind: TokenKind,
    pub start: usize,
    pub end: usize,
}

/// The Kotlin lexer over a byte slice, started in the initial state (`KotlinLexer.start(text)`).
pub struct KotlinLexer<'a> {
    text: &'a [u8],
    pos: usize,
    state: State,
    states: Vec<Saved>,
    l_brace_count: i32,
    required_interpolation_prefix: i32,
    /// Whether the last returned token started in the initial state with an empty state stack.
    last_plain: bool,
}

/// `LETTER = [:letter:]|_`.
#[inline]
fn is_letter(cp: u32) -> bool {
    if cp < 0x80 { (cp as u8).is_ascii_alphabetic() || cp == b'_' as u32 } else { in_ranges(&KOTLIN_LETTER, cp) }
}

/// `IDENTIFIER_PART = [:digit:]|{LETTER}`.
#[inline]
fn is_identifier_part(cp: u32) -> bool {
    if cp < 0x80 {
        (cp as u8).is_ascii_alphanumeric() || cp == b'_' as u32
    } else {
        in_ranges(&KOTLIN_LETTER, cp) || in_ranges(&KOTLIN_DIGIT, cp)
    }
}

/// `is_identifier_part` of each ASCII byte (`is_ascii_alphanumeric() || '_'`); false at 0x80 and above, where
/// `plain_identifier_end` decodes the character and asks `is_identifier_part`.
const ASCII_IDENTIFIER_PART: [bool; 256] = {
    let mut table = [false; 256];
    let mut b = 0;
    while b < 0x80 {
        table[b] = (b as u8).is_ascii_alphanumeric() || b == b'_' as usize;
        b += 1;
    }
    table
};

/// The hard keywords Kotlin.flex lists as literal rules (before `{IDENTIFIER}`).
fn hard_keyword(text: &[u8]) -> Option<TokenKind> {
    Some(match text {
        b"typealias" => TYPE_ALIAS_KEYWORD,
        b"interface" => INTERFACE_KEYWORD,
        b"continue" => CONTINUE_KEYWORD,
        b"package" => PACKAGE_KEYWORD,
        b"return" => RETURN_KEYWORD,
        b"object" => OBJECT_KEYWORD,
        b"while" => WHILE_KEYWORD,
        b"break" => BREAK_KEYWORD,
        b"class" => CLASS_KEYWORD,
        b"throw" => THROW_KEYWORD,
        b"false" => FALSE_KEYWORD,
        b"super" => SUPER_KEYWORD,
        b"typeof" => TYPEOF_KEYWORD,
        b"when" => WHEN_KEYWORD,
        b"true" => TRUE_KEYWORD,
        b"this" => THIS_KEYWORD,
        b"null" => NULL_KEYWORD,
        b"else" => ELSE_KEYWORD,
        b"try" => TRY_KEYWORD,
        b"val" => VAL_KEYWORD,
        b"var" => VAR_KEYWORD,
        b"fun" => FUN_KEYWORD,
        b"for" => FOR_KEYWORD,
        b"is" => IS_KEYWORD,
        b"in" => IN_KEYWORD,
        b"if" => IF_KEYWORD,
        b"do" => DO_KEYWORD,
        b"as" => AS_KEYWORD,
        _ => return None,
    })
}

impl<'a> KotlinLexer<'a> {
    /// `KotlinLexer().start(text)`.
    pub fn new(text: &'a [u8]) -> Self {
        KotlinLexer {
            text,
            pos: 0,
            state: State::Initial,
            states: Vec::new(),
            l_brace_count: 0,
            required_interpolation_prefix: 0,
            last_plain: true,
        }
    }

    /// Whether the last token returned by `advance` started in the initial state with an empty state stack: lexing the
    /// text from that token in isolation yields the same tokens.
    pub fn last_token_was_plain(&self) -> bool {
        self.last_plain
    }

    #[inline]
    fn at(&self, i: usize) -> Option<u8> {
        self.text.get(i).copied()
    }

    #[inline]
    fn starts_with(&self, i: usize, s: &[u8]) -> bool {
        self.text.get(i..i + s.len()) == Some(s)
    }

    fn push_state(&mut self, state: State) {
        self.states.push(Saved {
            state: self.state,
            l_brace_count: self.l_brace_count,
            required_interpolation_prefix: self.required_interpolation_prefix,
        });
        self.l_brace_count = 0;
        self.required_interpolation_prefix = -1;
        self.state = state;
    }

    fn push_interpolation_prefix(&mut self, interpolation_prefix: i32) {
        self.states.push(Saved {
            state: self.state,
            l_brace_count: self.l_brace_count,
            required_interpolation_prefix: self.required_interpolation_prefix,
        });
        self.l_brace_count = 0;
        self.required_interpolation_prefix = interpolation_prefix;
        self.state = State::StringPrefix;
    }

    fn pop_state(&mut self) {
        // Stack.pop() on an empty stack throws in Java; the rules never pop more than they push.
        if let Some(saved) = self.states.pop() {
            self.l_brace_count = saved.l_brace_count;
            self.required_interpolation_prefix = saved.required_interpolation_prefix;
            self.state = saved.state;
        } else {
            self.state = State::Initial;
        }
    }

    /// The end of `{IDENTIFIER}` = `{PLAIN_IDENTIFIER}|{ESCAPED_IDENTIFIER}` starting at `i`, if one matches.
    fn identifier_end(&self, i: usize) -> Option<usize> {
        let c = self.at(i)?;
        if c == b'`' {
            // ESCAPED_IDENTIFIER = `[^`\n]+`
            let mut j = i + 1;
            while let Some(b) = self.at(j) {
                if b == b'`' {
                    return if j > i + 1 { Some(j + 1) } else { None };
                }
                if b == b'\n' {
                    return None;
                }
                j += 1;
            }
            return None;
        }
        self.plain_identifier_end(i)
    }

    /// The end of `{PLAIN_IDENTIFIER} = {LETTER} {IDENTIFIER_PART}*` starting at `i`.
    fn plain_identifier_end(&self, i: usize) -> Option<usize> {
        if i >= self.text.len() {
            return None;
        }
        let (cp, len) = char_at(self.text, i);
        if !is_letter(cp) {
            return None;
        }
        let mut j = i + len;
        loop {
            let rest = &self.text[j..];
            j += rest.iter().position(|&b| !ASCII_IDENTIFIER_PART[b as usize]).unwrap_or(rest.len());
            if j >= self.text.len() || self.text[j] < 0x80 {
                break;
            }
            let (cp, len) = char_at(self.text, j);
            if !is_identifier_part(cp) {
                break;
            }
            j += len;
        }
        Some(j)
    }

    /// The next token (`FlexAdapter.advance` + `getTokenType`), or `None` at the end of the text.
    pub fn advance(&mut self) -> Option<Token> {
        loop {
            let start = self.pos;
            let plain = self.state == State::Initial && self.states.is_empty();
            if start >= self.text.len() {
                return None;
            }
            let kind = match self.state {
                State::Initial | State::UnmatchedBacktick => self.lex_inclusive(start),
                State::LongTemplateEntry => match self.text[start] {
                    // <LONG_TEMPLATE_ENTRY> "{"
                    b'{' => {
                        self.l_brace_count += 1;
                        self.pos = start + 1;
                        Some(LBRACE)
                    }
                    // <LONG_TEMPLATE_ENTRY> "}"
                    b'}' => {
                        self.pos = start + 1;
                        if self.l_brace_count == 0 {
                            self.pop_state();
                            Some(LONG_TEMPLATE_ENTRY_END)
                        } else {
                            self.l_brace_count -= 1;
                            Some(RBRACE)
                        }
                    }
                    _ => self.lex_inclusive(start),
                },
                State::StringPrefix => Some(self.lex_string_prefix(start)),
                State::String => Some(self.lex_string(start)),
                State::RawString => Some(self.lex_raw_string(start)),
                State::ShortTemplateEntry => Some(self.lex_short_template_entry(start)),
            };
            if let Some(kind) = kind {
                self.last_plain = plain;
                return Some(Token { kind, start, end: self.pos });
            }
        }
    }

    /// The rules of the initial state and of the inclusive states (`%state LONG_TEMPLATE_ENTRY UNMATCHED_BACKTICK`).
    /// Returns `None` when a rule matched without returning a token (a comment opener, a bare `"`).
    fn lex_inclusive(&mut self, start: usize) -> Option<TokenKind> {
        let text = self.text;
        let c = text[start];
        let next = self.at(start + 1);
        let (kind, len) = match c {
            b'"' | b'$' => {
                // {INTERPOLATION}?\"
                let mut dollars = 0;
                while self.at(start + dollars) == Some(b'$') {
                    dollars += 1;
                }
                if self.at(start + dollars) == Some(b'"') {
                    let interpolation_prefix = dollars as i32;
                    self.push_interpolation_prefix(interpolation_prefix.max(1));
                    self.pos = start + dollars; // yypushback(1)
                    return if interpolation_prefix != 0 { Some(INTERPOLATION_PREFIX) } else { None };
                }
                // {FIELD_IDENTIFIER} = \${IDENTIFIER}
                match self.identifier_end(start + 1) {
                    Some(end) => (FIELD_IDENTIFIER, end - start),
                    None => (BAD_CHARACTER, 1),
                }
            }
            // ({WHITE_SPACE_CHAR})+
            b' ' | b'\n' | b'\t' | 0x0C => {
                let mut j = start + 1;
                while matches!(self.at(j), Some(b' ' | b'\n' | b'\t' | 0x0C)) {
                    j += 1;
                }
                (WHITE_SPACE, j - start)
            }
            b'/' => match next {
                // {EOL_COMMENT}
                Some(b'/') => (EOL_COMMENT, self.rest_of_line(start) - start),
                Some(b'*') => {
                    if self.starts_with(start, b"/**/") {
                        // "/**/"
                        (BLOCK_COMMENT, 4)
                    } else {
                        // "/**" and "/*": pushState(DOC_COMMENT | BLOCK_COMMENT); the comment ends at the "*/" that
                        // closes it (nested "/*" count) or at <<EOF>>, where the state is popped again
                        let doc = self.at(start + 2) == Some(b'*');
                        let end = self.comment_end(start + if doc { 3 } else { 2 });
                        (if doc { DOC_COMMENT } else { BLOCK_COMMENT }, end - start)
                    }
                }
                Some(b'=') => (DIVEQ, 2),
                _ => (DIV, 1),
            },
            b'#' => {
                if next == Some(b'!') {
                    // {SHEBANG_COMMENT}: `zzCurrentPos == 0`, else yypushback(yylength() - 1) and HASH. zzCurrentPos
                    // is the token start unless the match scanned to the end of the buffer: zzRefill then stores the
                    // scan position into it, so a shebang line without a `\n` after it is a HASH too.
                    let end = self.rest_of_line(start);
                    if start == 0 && end < text.len() { (SHEBANG_COMMENT, end - start) } else { (HASH, 1) }
                } else {
                    (HASH, 1)
                }
            }
            b'0'..=b'9' => self.lex_number(start),
            b'.' => {
                if matches!(next, Some(b'0'..=b'9')) {
                    // {DOUBLE_LITERAL} starting with "."
                    (FLOAT_LITERAL, self.double_literal_end(start).unwrap_or(start + 1) - start)
                } else if next == Some(b'.') {
                    match self.at(start + 2) {
                        Some(b'.') => (RESERVED, 3),
                        Some(b'<') => (RANGE_UNTIL, 3),
                        _ => (RANGE, 2),
                    }
                } else {
                    (DOT, 1)
                }
            }
            b'\'' => (CHARACTER_LITERAL, self.character_literal_end(start) - start),
            b'`' => match self.identifier_end(start) {
                Some(end) => (IDENTIFIER, end - start),
                None => {
                    // {LONELY_BACKTICK}
                    self.push_state(State::UnmatchedBacktick);
                    (BAD_CHARACTER, 1)
                }
            },
            b'!' => {
                if self.starts_with(start, b"!==") {
                    (EXCLEQEQEQ, 3)
                } else if next == Some(b'=') {
                    (EXCLEQ, 2)
                } else if self.starts_with(start, b"!in") || self.starts_with(start, b"!is") {
                    // \!in{IDENTIFIER_PART} and \!is{IDENTIFIER_PART}: yypushback(3), EXCL
                    let followed_by_part = start + 3 < text.len() && is_identifier_part(char_at(text, start + 3).0);
                    if followed_by_part {
                        (EXCL, 1)
                    } else if text[start + 2] == b'n' {
                        (NOT_IN, 3)
                    } else {
                        (NOT_IS, 3)
                    }
                } else {
                    (EXCL, 1)
                }
            }
            b'=' => {
                if self.starts_with(start, b"===") {
                    (EQEQEQ, 3)
                } else {
                    match next {
                        Some(b'=') => (EQEQ, 2),
                        Some(b'>') => (DOUBLE_ARROW, 2),
                        _ => (EQ, 1),
                    }
                }
            }
            b'+' => match next {
                Some(b'+') => (PLUSPLUS, 2),
                Some(b'=') => (PLUSEQ, 2),
                _ => (PLUS, 1),
            },
            b'-' => match next {
                Some(b'-') => (MINUSMINUS, 2),
                Some(b'=') => (MINUSEQ, 2),
                Some(b'>') => (ARROW, 2),
                _ => (MINUS, 1),
            },
            b'<' => {
                if next == Some(b'=') {
                    (LTEQ, 2)
                } else {
                    (LT, 1)
                }
            }
            b'>' => {
                if next == Some(b'=') {
                    (GTEQ, 2)
                } else {
                    (GT, 1)
                }
            }
            b'&' => {
                if next == Some(b'&') {
                    (ANDAND, 2)
                } else {
                    (AND, 1)
                }
            }
            b'|' => {
                if next == Some(b'|') {
                    (OROR, 2)
                } else {
                    (BAD_CHARACTER, 1)
                }
            }
            b'*' => {
                if next == Some(b'=') {
                    (MULTEQ, 2)
                } else {
                    (MUL, 1)
                }
            }
            b'%' => {
                if next == Some(b'=') {
                    (PERCEQ, 2)
                } else {
                    (PERC, 1)
                }
            }
            b':' => {
                if next == Some(b':') {
                    (COLONCOLON, 2)
                } else {
                    (COLON, 1)
                }
            }
            b';' => {
                if next == Some(b';') {
                    (DOUBLE_SEMICOLON, 2)
                } else {
                    (SEMICOLON, 1)
                }
            }
            b'?' => (QUEST, 1),
            b',' => (COMMA, 1),
            b'@' => (AT, 1),
            b'[' => (LBRACKET, 1),
            b']' => (RBRACKET, 1),
            b'{' => (LBRACE, 1),
            b'}' => (RBRACE, 1),
            b'(' => (LPAR, 1),
            b')' => (RPAR, 1),
            _ => match self.plain_identifier_end(start) {
                Some(end) => {
                    let word = &text[start..end];
                    if word == b"as" && self.at(end) == Some(b'?') {
                        // "as?"
                        (AS_SAFE, 3)
                    } else {
                        // the keyword rules win the tie with {IDENTIFIER}
                        (hard_keyword(word).unwrap_or(IDENTIFIER), end - start)
                    }
                }
                // error fallback [\s\S]: one character
                None => (BAD_CHARACTER, char_at(text, start).1),
            },
        };
        self.pos = start + len;
        Some(kind)
    }

    /// The end of `[^\n]*` from `i`.
    fn rest_of_line(&self, i: usize) -> usize {
        match self.text[i..].iter().position(|&b| b == b'\n') {
            Some(p) => i + p,
            None => self.text.len(),
        }
    }

    /// `{INTEGER_LITERAL}\.\.`, `{INTEGER_LITERAL}`, `{DOUBLE_LITERAL}` at a digit: the longest wins, INTEGER on a tie.
    fn lex_number(&self, start: usize) -> (TokenKind, usize) {
        let integer_end = self.integer_literal_end(start);
        if self.starts_with(integer_end, b"..") {
            // {INTEGER_LITERAL}\.\. { yypushback(2); return INTEGER_LITERAL; }
            return (INTEGER_LITERAL, integer_end - start);
        }
        match self.double_literal_end(start) {
            Some(double_end) if double_end > integer_end => (FLOAT_LITERAL, double_end - start),
            _ => (INTEGER_LITERAL, integer_end - start),
        }
    }

    /// The end of the longest `{INTEGER_LITERAL}` at `start` (a digit):
    /// `{DIGIT} {DIGIT_OR_UNDERSCORE}* | 0 [Xx] {HEX_DIGIT_OR_UNDERSCORE}* | 0 [Bb] {DIGIT_OR_UNDERSCORE}*`, each followed
    /// by `[Uu]? [Ll]?`.
    fn integer_literal_end(&self, start: usize) -> usize {
        let skip = |mut j: usize, pred: fn(u8) -> bool| {
            while self.at(j).is_some_and(pred) {
                j += 1;
            }
            j
        };
        let suffix = |mut j: usize| {
            if matches!(self.at(j), Some(b'u' | b'U')) {
                j += 1;
            }
            if matches!(self.at(j), Some(b'l' | b'L')) {
                j += 1;
            }
            j
        };
        let decimal = suffix(skip(start + 1, |b| b == b'_' || b.is_ascii_digit()));
        if self.text[start] == b'0' {
            match self.at(start + 1) {
                Some(b'x' | b'X') => {
                    return decimal.max(suffix(skip(start + 2, |b| b == b'_' || b.is_ascii_hexdigit())));
                }
                Some(b'b' | b'B') => return decimal.max(suffix(skip(start + 2, |b| b == b'_' || b.is_ascii_digit()))),
                _ => {}
            }
        }
        decimal
    }

    /// The end of the longest `{DOUBLE_LITERAL}` at `start`:
    /// `{DIGITS}? "." {DIGITS} {EXPONENT_PART}? [Ff]? | {DIGITS} ({EXPONENT_PART} [Ff]? | [Ff])`, where
    /// `DIGITS = {DIGIT} {DIGIT_OR_UNDERSCORE}*` and `EXPONENT_PART = [Ee][+-]? {DIGIT_OR_UNDERSCORE}*`.
    fn double_literal_end(&self, start: usize) -> Option<usize> {
        let digits = |j: usize| -> Option<usize> {
            if !self.at(j).is_some_and(|b| b.is_ascii_digit()) {
                return None;
            }
            let mut k = j + 1;
            while self.at(k).is_some_and(|b| b == b'_' || b.is_ascii_digit()) {
                k += 1;
            }
            Some(k)
        };
        let exponent = |j: usize| -> Option<usize> {
            if !matches!(self.at(j), Some(b'e' | b'E')) {
                return None;
            }
            let mut k = j + 1;
            if matches!(self.at(k), Some(b'+' | b'-')) {
                k += 1;
            }
            while self.at(k).is_some_and(|b| b == b'_' || b.is_ascii_digit()) {
                k += 1;
            }
            Some(k)
        };
        let float_suffix = |j: usize| if matches!(self.at(j), Some(b'f' | b'F')) { j + 1 } else { j };
        let mut best: Option<usize> = None;
        let whole = digits(start);
        // {DIGITS}? "." {DIGITS} {EXPONENT_PART}? [Ff]?
        let dot = whole.unwrap_or(start);
        if self.at(dot) == Some(b'.')
            && let Some(fraction) = digits(dot + 1)
        {
            let end = float_suffix(exponent(fraction).unwrap_or(fraction));
            best = Some(end);
        }
        // {DIGITS} ({EXPONENT_PART} [Ff]? | [Ff])
        if let Some(whole) = whole {
            let end = match exponent(whole) {
                Some(e) => Some(float_suffix(e)),
                None => matches!(self.at(whole), Some(b'f' | b'F')).then_some(whole + 1),
            };
            if let Some(end) = end {
                best = Some(best.map_or(end, |b| b.max(end)));
            }
        }
        best
    }

    /// The end of `"'"([^\\\'\n]|{ESCAPE_SEQUENCE})*("'"|\\)?` at `start` (a quote).
    fn character_literal_end(&self, start: usize) -> usize {
        let mut j = start + 1;
        loop {
            match self.at(j) {
                None | Some(b'\'') | Some(b'\n') => break,
                Some(b'\\') => match self.escape_sequence_end(j) {
                    Some(end) => j = end,
                    None => break,
                },
                Some(_) => j += char_at(self.text, j).1,
            }
        }
        if matches!(self.at(j), Some(b'\'' | b'\\')) {
            j += 1;
        }
        j
    }

    /// The end of `ESCAPE_SEQUENCE=\\(u{HEX_DIGIT}{HEX_DIGIT}{HEX_DIGIT}{HEX_DIGIT}|[^\n])` at `i` (a backslash).
    fn escape_sequence_end(&self, i: usize) -> Option<usize> {
        match self.at(i + 1) {
            None | Some(b'\n') => None,
            Some(b'u') if self.text.get(i + 2..i + 6).is_some_and(|h| h.iter().all(u8::is_ascii_hexdigit)) => {
                Some(i + 6)
            }
            Some(_) => Some(i + 1 + char_at(self.text, i + 1).1),
        }
    }

    /// `<STRING_PREFIX>`: `{THREE_QUO}` opens a raw string, `\"` a plain one.
    fn lex_string_prefix(&mut self, start: usize) -> TokenKind {
        if self.starts_with(start, b"\"\"\"") {
            self.state = State::RawString;
            self.pos = start + 3;
            OPEN_QUOTE
        } else if self.text[start] == b'"' {
            self.state = State::String;
            self.pos = start + 1;
            OPEN_QUOTE
        } else {
            // No rule of the exclusive STRING_PREFIX state matches (unreachable: the quote was pushed back).
            self.pos = start + char_at(self.text, start).1;
            BAD_CHARACTER
        }
    }

    /// `<STRING>`.
    fn lex_string(&mut self, start: usize) -> TokenKind {
        match self.text[start] {
            b'\n' => {
                // { popState(); yypushback(1); return DANGLING_NEWLINE; }: an empty token
                self.pop_state();
                self.pos = start;
                DANGLING_NEWLINE
            }
            b'"' => {
                self.pop_state();
                self.pos = start + 1;
                CLOSING_QUOTE
            }
            b'\\' => match self.escape_sequence_end(start) {
                Some(end) => {
                    self.pos = end;
                    ESCAPE_SEQUENCE
                }
                None => {
                    // error fallback for exclusive states: .
                    self.pos = start + 1;
                    BAD_CHARACTER
                }
            },
            b'$' => self.lex_template_start(start),
            _ => {
                self.pos = self.regular_string_part_end(start);
                REGULAR_STRING_PART
            }
        }
    }

    /// `<RAW_STRING>`.
    fn lex_raw_string(&mut self, start: usize) -> TokenKind {
        match self.text[start] {
            b'\n' | b'\\' => {
                self.pos = start + 1;
                REGULAR_STRING_PART
            }
            b'"' => {
                let mut quotes = 0;
                while self.at(start + quotes) == Some(b'"') {
                    quotes += 1;
                }
                if quotes >= 3 {
                    // {THREE_OR_MORE_QUO}
                    if quotes <= 3 {
                        self.pop_state();
                        self.pos = start + 3;
                        CLOSING_QUOTE
                    } else {
                        // yypushback(3): the closing quotes go back to the stream
                        self.pos = start + quotes - 3;
                        REGULAR_STRING_PART
                    }
                } else {
                    self.pos = start + 1;
                    REGULAR_STRING_PART
                }
            }
            b'$' => self.lex_template_start(start),
            _ => {
                self.pos = self.regular_string_part_end(start);
                REGULAR_STRING_PART
            }
        }
    }

    /// The end of `REGULAR_STRING_PART=[^\\\"\n\$]+` at `start`.
    fn regular_string_part_end(&self, start: usize) -> usize {
        match self.text[start..].iter().position(|&b| matches!(b, b'\\' | b'"' | b'\n' | b'$')) {
            Some(p) => start + p.max(1),
            None => self.text.len(),
        }
    }

    /// `<STRING, RAW_STRING>` at a `$`: `{SHORT_TEMPLATE_ENTRY}`, `{LONELY_DOLLAR}`, `{LONG_TEMPLATE_ENTRY_START}`.
    fn lex_template_start(&mut self, start: usize) -> TokenKind {
        let mut dollars = 0;
        while self.at(start + dollars) == Some(b'$') {
            dollars += 1;
        }
        let after = start + dollars;
        let required = self.required_interpolation_prefix;
        let interpolation_prefix = dollars as i32;
        if self.identifier_end(after).is_some() {
            // {SHORT_TEMPLATE_ENTRY} = {INTERPOLATION}{IDENTIFIER}; the identifier is pushed back
            if interpolation_prefix == required {
                self.push_state(State::ShortTemplateEntry);
                self.pos = after;
                SHORT_TEMPLATE_ENTRY_START
            } else if interpolation_prefix < required {
                self.pos = after;
                REGULAR_STRING_PART
            } else {
                self.pos = after - required as usize;
                REGULAR_STRING_PART
            }
        } else if self.at(after) == Some(b'{') {
            // {LONG_TEMPLATE_ENTRY_START} = {INTERPOLATION}\{
            if interpolation_prefix == required {
                self.push_state(State::LongTemplateEntry);
                self.pos = after + 1;
                LONG_TEMPLATE_ENTRY_START
            } else if interpolation_prefix < required {
                self.pos = after;
                REGULAR_STRING_PART
            } else {
                self.pos = after - required as usize;
                REGULAR_STRING_PART
            }
        } else {
            // {LONELY_DOLLAR}
            self.pos = after;
            REGULAR_STRING_PART
        }
    }

    /// `<SHORT_TEMPLATE_ENTRY>`: `"this"` or `{IDENTIFIER}`, then back to the string.
    fn lex_short_template_entry(&mut self, start: usize) -> TokenKind {
        match self.identifier_end(start) {
            Some(end) => {
                self.pop_state();
                self.pos = end;
                if &self.text[start..end] == b"this" { THIS_KEYWORD } else { IDENTIFIER }
            }
            None => {
                // error fallback for exclusive states (unreachable: an identifier was pushed back)
                self.pos = start + char_at(self.text, start).1;
                BAD_CHARACTER
            }
        }
    }

    /// `<BLOCK_COMMENT, DOC_COMMENT>` from `from`: the end of the `*/` closing the comment (`commentDepth` counts the
    /// nested `/*`), or the end of the text (`<<EOF>>`).
    fn comment_end(&self, from: usize) -> usize {
        let text = self.text;
        let mut comment_depth = 0;
        let mut j = from;
        while j < text.len() {
            match text[j] {
                b'/' if text.get(j + 1) == Some(&b'*') => {
                    comment_depth += 1;
                    j += 2;
                }
                b'*' if text.get(j + 1) == Some(&b'/') => {
                    j += 2;
                    if comment_depth > 0 {
                        comment_depth -= 1;
                    } else {
                        return j;
                    }
                }
                _ => j += 1,
            }
        }
        text.len()
    }
}

/// The tokens of a text: types, start offsets (one more than tokens: the last is the text length) and, per token,
/// whether it started in the lexer's initial state with an empty state stack.
pub type Lexed = psi_builder::Lexed<TokenKind>;

/// Lexes `text` (at most `u32::MAX` bytes are lexed; offsets are `u32`) into token types and starts, as
/// `TokenSequence.performLexing` does with `KotlinLexer`.
pub fn tokenize(text: &[u8]) -> Lexed {
    let mut lexed = Lexed::default();
    tokenize_into(text, &mut lexed);
    lexed
}

/// `tokenize` into `lexed`, which is cleared first (its buffers are reused).
pub fn tokenize_into(text: &[u8], lexed: &mut Lexed) {
    let text = &text[..text.len().min(u32::MAX as usize)];
    lexed.begin(text.len());
    let mut lexer = KotlinLexer::new(text);
    while let Some(token) = lexer.advance() {
        lexed.push(token.kind, token.start as u32, lexer.last_token_was_plain());
    }
    lexed.end(text.len() as u32);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(text: &str) -> Vec<(TokenKind, &str)> {
        let mut lexer = KotlinLexer::new(text.as_bytes());
        let mut out = Vec::new();
        while let Some(t) = lexer.advance() {
            out.push((t.kind, &text[t.start..t.end]));
        }
        out
    }

    #[test]
    fn numbers() {
        assert_eq!(lex("1..2"), [(INTEGER_LITERAL, "1"), (RANGE, ".."), (INTEGER_LITERAL, "2")]);
        assert_eq!(lex("1."), [(INTEGER_LITERAL, "1"), (DOT, ".")]);
        assert_eq!(lex("1.5f"), [(FLOAT_LITERAL, "1.5f")]);
        assert_eq!(lex(".5e+3"), [(FLOAT_LITERAL, ".5e+3")]);
        assert_eq!(lex("1e"), [(FLOAT_LITERAL, "1e")]);
        assert_eq!(lex("0x1fUL"), [(INTEGER_LITERAL, "0x1fUL")]);
        assert_eq!(lex("0b102"), [(INTEGER_LITERAL, "0b102")]);
        assert_eq!(lex("1F"), [(FLOAT_LITERAL, "1F")]);
        assert_eq!(lex("0x.."), [(INTEGER_LITERAL, "0x"), (RANGE, "..")]);
        assert_eq!(lex("1..<2"), [(INTEGER_LITERAL, "1"), (RANGE_UNTIL, "..<"), (INTEGER_LITERAL, "2")]);
        assert_eq!(lex("0...1"), [(INTEGER_LITERAL, "0"), (RESERVED, "..."), (INTEGER_LITERAL, "1")]);
    }

    #[test]
    fn strings_and_templates() {
        assert_eq!(
            lex(r#""a$b${c}""#),
            [
                (OPEN_QUOTE, "\""),
                (REGULAR_STRING_PART, "a"),
                (SHORT_TEMPLATE_ENTRY_START, "$"),
                (IDENTIFIER, "b"),
                (LONG_TEMPLATE_ENTRY_START, "${"),
                (IDENTIFIER, "c"),
                (LONG_TEMPLATE_ENTRY_END, "}"),
                (CLOSING_QUOTE, "\""),
            ]
        );
        assert_eq!(
            lex("$$\"$x$$y\""),
            [
                (INTERPOLATION_PREFIX, "$$"),
                (OPEN_QUOTE, "\""),
                (REGULAR_STRING_PART, "$"),
                (REGULAR_STRING_PART, "x"),
                (SHORT_TEMPLATE_ENTRY_START, "$$"),
                (IDENTIFIER, "y"),
                (CLOSING_QUOTE, "\""),
            ]
        );
        assert_eq!(
            lex("\"a\nb"),
            [
                (OPEN_QUOTE, "\""),
                (REGULAR_STRING_PART, "a"),
                (DANGLING_NEWLINE, ""),
                (WHITE_SPACE, "\n"),
                (IDENTIFIER, "b")
            ]
        );
        assert_eq!(
            lex("\"\"\"a\n\"\"\"\"\""),
            [
                (OPEN_QUOTE, "\"\"\""),
                (REGULAR_STRING_PART, "a"),
                (REGULAR_STRING_PART, "\n"),
                (REGULAR_STRING_PART, "\"\""),
                (CLOSING_QUOTE, "\"\"\"")
            ]
        );
        assert_eq!(
            lex("\"\\u12\""),
            [(OPEN_QUOTE, "\""), (ESCAPE_SEQUENCE, "\\u"), (REGULAR_STRING_PART, "12"), (CLOSING_QUOTE, "\"")]
        );
        assert_eq!(
            lex("\"${ {} }\""),
            [
                (OPEN_QUOTE, "\""),
                (LONG_TEMPLATE_ENTRY_START, "${"),
                (WHITE_SPACE, " "),
                (LBRACE, "{"),
                (RBRACE, "}"),
                (WHITE_SPACE, " "),
                (LONG_TEMPLATE_ENTRY_END, "}"),
                (CLOSING_QUOTE, "\"")
            ]
        );
    }

    #[test]
    fn comments_identifiers_operators() {
        assert_eq!(lex("/* a /* b */ c */x"), [(BLOCK_COMMENT, "/* a /* b */ c */"), (IDENTIFIER, "x")]);
        assert_eq!(lex("/**/"), [(BLOCK_COMMENT, "/**/")]);
        assert_eq!(lex("/***/"), [(DOC_COMMENT, "/***/")]);
        assert_eq!(lex("/** a"), [(DOC_COMMENT, "/** a")]);
        assert_eq!(
            lex("#!/bin\n#!x"),
            [(SHEBANG_COMMENT, "#!/bin"), (WHITE_SPACE, "\n"), (HASH, "#"), (EXCL, "!"), (IDENTIFIER, "x")]
        );
        assert_eq!(lex("#!x"), [(HASH, "#"), (EXCL, "!"), (IDENTIFIER, "x")]);
        assert_eq!(
            lex("`a b` `c"),
            [(IDENTIFIER, "`a b`"), (WHITE_SPACE, " "), (BAD_CHARACTER, "`"), (IDENTIFIER, "c")]
        );
        assert_eq!(
            lex("!in !inx !is"),
            [
                (NOT_IN, "!in"),
                (WHITE_SPACE, " "),
                (EXCL, "!"),
                (IDENTIFIER, "inx"),
                (WHITE_SPACE, " "),
                (NOT_IS, "!is")
            ]
        );
        assert_eq!(
            lex("as? as?.x"),
            [(AS_SAFE, "as?"), (WHITE_SPACE, " "), (AS_SAFE, "as?"), (DOT, "."), (IDENTIFIER, "x")]
        );
        assert_eq!(
            lex("a?.b?:c!!"),
            [
                (IDENTIFIER, "a"),
                (QUEST, "?"),
                (DOT, "."),
                (IDENTIFIER, "b"),
                (QUEST, "?"),
                (COLON, ":"),
                (IDENTIFIER, "c"),
                (EXCL, "!"),
                (EXCL, "!")
            ]
        );
        assert_eq!(
            lex("'a' '\\'' '"),
            [
                (CHARACTER_LITERAL, "'a'"),
                (WHITE_SPACE, " "),
                (CHARACTER_LITERAL, "'\\''"),
                (WHITE_SPACE, " "),
                (CHARACTER_LITERAL, "'")
            ]
        );
        assert_eq!(
            lex("$x \r|"),
            [(FIELD_IDENTIFIER, "$x"), (WHITE_SPACE, " "), (BAD_CHARACTER, "\r"), (BAD_CHARACTER, "|")]
        );
        assert_eq!(lex("Жx1 €"), [(IDENTIFIER, "Жx1"), (WHITE_SPACE, " "), (BAD_CHARACTER, "€")]);
    }

    #[test]
    fn invalid_utf8_is_bad_characters() {
        let text = b"a\xFF\xC3";
        let lexed = tokenize(text);
        assert_eq!(lexed.types, [IDENTIFIER, BAD_CHARACTER, BAD_CHARACTER]);
        assert_eq!(lexed.starts, [0, 1, 2, 3]);
    }
}
