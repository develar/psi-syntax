//! `QuickScanner.cs`: a state machine over the next characters that finds a token whose text, trivia included, makes
//! the same token wherever it appears (whitespace and ends of line, then an identifier, a keyword, a decimal integer or
//! a simple punctuation, then whitespace and at most one end of line), and the token map of `LexerCache` that keeps
//! what such a text lexed to. Roslyn's table classifies the first 0x180 UTF-16 code units; here a byte at or above
//! 0x80 is `Complex` (a character that is not ASCII ends the quick scan, and `LexSyntaxToken` lexes it as Roslyn does),
//! which changes which tokens are found in the map, never a token.

use super::Lexer;
use super::lexer_cache::{EOL_BIT, Key, MAX_CACHED_TOKEN_SIZE, MAX_TRIVIA, QuickToken};
use crate::green::Green;
use crate::syntax_kind::SyntaxKind;

/// `QuickScanState`.
mod state {
    pub(super) const INITIAL: u8 = 0;
    pub(super) const FOLLOWING_WHITE: u8 = 1;
    pub(super) const FOLLOWING_CR: u8 = 2;
    pub(super) const IDENT: u8 = 3;
    pub(super) const NUMBER: u8 = 4;
    pub(super) const PUNCTUATION: u8 = 5;
    pub(super) const DOT: u8 = 6;
    pub(super) const COMPOUND_PUNCT_START: u8 = 7;
    pub(super) const DONE_AFTER_NEXT: u8 = 8;
    // we are relying on Bad state immediately following Done to be able to detect exiting conditions in one
    // "state >= Done" test.
    pub(super) const DONE: u8 = 9;
    pub(super) const BAD: u8 = DONE + 1;
}

/// `CharFlags`.
mod flags {
    pub(super) const WHITE: u8 = 0; // simple whitespace (space/tab)
    pub(super) const CR: u8 = 1; // carriage return
    pub(super) const LF: u8 = 2; // line feed
    pub(super) const LETTER: u8 = 3; // letter
    pub(super) const DIGIT: u8 = 4; // digit 0-9
    pub(super) const PUNCT: u8 = 5; // some simple punctuation (parens, braces, comma, equals, question)
    pub(super) const DOT: u8 = 6; // dot is different from other punctuation when followed by a digit (Ex: .9 )
    pub(super) const COMPOUND: u8 = 7; // may be a part of compound punctuation
    pub(super) const SLASH: u8 = 8; // /
    pub(super) const COMPLEX: u8 = 9; // complex - causes scanning to abort
    // EndOfFile (10), the last column of `s_stateTransitions`: no character has it
}

/// The columns of a row of `s_stateTransitions` (11 flags, padded to 16 so a row is indexed by a shift).
const COLUMNS: usize = 16;

/// `s_stateTransitions`, one row of 16 per state (the rows of `Done` and `Bad` are never read), each entry the next
/// state times `COLUMNS`: the index of its row, so a step is one add and one load. 256 entries, the rows past `Done`
/// all `Bad`, so that a `u8` index needs no bounds check.
const STATE_TRANSITIONS: [u8; 256] = {
    use state::*;
    const fn row(r: [u8; 11]) -> [u8; COLUMNS] {
        let mut out = [BAD; COLUMNS];
        let mut i = 0;
        while i < 11 {
            out[i] = r[i];
            i += 1;
        }
        out
    }
    let rows: [[u8; COLUMNS]; 10] = [
        // Initial: White, CR, LF, Letter, Digit, Punct, Dot, Compound, Slash, Complex, EndOfFile
        row([INITIAL, INITIAL, INITIAL, IDENT, NUMBER, PUNCTUATION, DOT, COMPOUND_PUNCT_START, BAD, BAD, BAD]),
        // Following White
        row([FOLLOWING_WHITE, FOLLOWING_CR, DONE_AFTER_NEXT, DONE, DONE, DONE, DONE, DONE, BAD, BAD, DONE]),
        // Following CR
        row([DONE, DONE, DONE_AFTER_NEXT, DONE, DONE, DONE, DONE, DONE, DONE, DONE, DONE]),
        // Identifier
        row([FOLLOWING_WHITE, FOLLOWING_CR, DONE_AFTER_NEXT, IDENT, IDENT, DONE, DONE, DONE, BAD, BAD, DONE]),
        // Number: Letter might be 'e' or 'x' or suffix; a dot is too complex for us to handle here
        row([FOLLOWING_WHITE, FOLLOWING_CR, DONE_AFTER_NEXT, BAD, NUMBER, DONE, BAD, DONE, BAD, BAD, DONE]),
        // Punctuation
        row([FOLLOWING_WHITE, FOLLOWING_CR, DONE_AFTER_NEXT, DONE, DONE, DONE, DONE, DONE, BAD, BAD, DONE]),
        // Dot: followed by a number could be a fp `.0` or a range + num `..0`; a dot is the DotDot range token
        row([FOLLOWING_WHITE, FOLLOWING_CR, DONE_AFTER_NEXT, DONE, BAD, DONE, BAD, DONE, BAD, BAD, DONE]),
        // Compound Punctuation
        row([FOLLOWING_WHITE, FOLLOWING_CR, DONE_AFTER_NEXT, DONE, DONE, BAD, DONE, BAD, BAD, BAD, DONE]),
        // Done after next
        row([DONE, DONE, DONE, DONE, DONE, DONE, DONE, DONE, DONE, DONE, DONE]),
        // Done (never read)
        row([DONE; 11]),
    ];
    let mut out = [BAD * COLUMNS as u8; 256];
    let mut r = 0;
    while r < 10 {
        let mut c = 0;
        while c < COLUMNS {
            out[r * COLUMNS + c] = rows[r][c] * COLUMNS as u8;
            c += 1;
        }
        r += 1;
    }
    out
};

/// `CharProperties` of the ASCII characters (`#` is complex as it may start directives); every byte from 0x80 is
/// `Complex`.
const CHAR_PROPERTIES: [u8; 256] = {
    use flags::*;
    let mut t = [COMPLEX; 256];
    t[b'\t' as usize] = WHITE;
    t[b'\n' as usize] = LF;
    t[0x0B] = WHITE; // VT
    t[0x0C] = WHITE; // FF
    t[b'\r' as usize] = CR;
    t[b' ' as usize] = WHITE;
    let compound = b"!%&*+-:<=>?^|~";
    let mut i = 0;
    while i < compound.len() {
        t[compound[i] as usize] = COMPOUND;
        i += 1;
    }
    let punct = b"(),;[]{}";
    let mut i = 0;
    while i < punct.len() {
        t[punct[i] as usize] = PUNCT;
        i += 1;
    }
    t[b'.' as usize] = DOT;
    t[b'/' as usize] = SLASH;
    let mut c = b'0';
    while c <= b'9' {
        t[c as usize] = DIGIT;
        c += 1;
    }
    let mut c = b'A';
    while c <= b'Z' {
        t[c as usize] = LETTER;
        t[(c + 32) as usize] = LETTER;
        c += 1;
    }
    t[b'_' as usize] = LETTER;
    t
};

impl<'t> Lexer<'t> {
    /// `QuickScanSyntaxToken()`: the token of the text at the position from the map when the state machine accepts
    /// it, lexed and added to the map when it is not there yet; `None` when the state machine does not accept it.
    #[inline]
    pub(crate) fn quick_scan_syntax_token(&mut self) -> Option<Green> {
        self.start();
        let start = self.text_window.position() as usize;
        let text = self.text_window.text();
        let end = (self.text_window.text_end() as usize).min(start + MAX_CACHED_TOKEN_SIZE);
        // Cap how much of the char span we're willing to look at.
        let span = text.get(start..end)?;

        // the state times `COLUMNS` (`STATE_TRANSITIONS`); the hash is the key's (`Key::new`), not a running one
        let mut st = state::INITIAL * COLUMNS as u8;
        let mut current_index = 0;
        loop {
            let Some(&c) = span.get(current_index) else {
                return None; // ran out of characters in window
            };
            let flags = CHAR_PROPERTIES[c as usize];
            // a row's start plus a column below 16: at most 255
            st = STATE_TRANSITIONS[st.wrapping_add(flags) as usize];
            // NOTE: that Bad > Done and it is the only state like that as a result, we will exit the loop on either
            // Bad or Done.
            if st >= state::DONE * COLUMNS as u8 {
                break;
            }
            current_index += 1;
        }
        if st != state::DONE * COLUMNS as u8 {
            return None;
        }

        // this is a good token!
        let token_length = current_index;
        let key = Key::new(text, start, token_length);
        if let Some(token) = self.buffers.cache.find_item(&key) {
            self.text_window.reset((start + token_length) as u32);
            return Some(self.arena.quick_token(start as u32, &token));
        }
        let token = self.create_quick_token();
        if let Some(shape) = self.quick_token_of(token, token_length as u32) {
            self.buffers.cache.add_item(&key, shape);
        }
        Some(token)
    }

    /// `CreateQuickToken(lexer)`.
    fn create_quick_token(&mut self) -> Green {
        let start = self.lexeme_start_position;
        self.text_window.reset(start);
        self.lex_syntax_token()
    }

    /// What the map keeps of `token` (`full_width` wide): `None` for a token with diagnostics, a value, a text that is
    /// not the source's or trivia other than whitespace and ends of line.
    fn quick_token_of(&self, token: Green, full_width: u32) -> Option<QuickToken> {
        let a = &self.arena;
        if a.full_width(token) != full_width || a.contains_diagnostics(token) || a.token_value(token).is_some() {
            return None;
        }
        a.token_source_range(token)?;
        let leading = a.trivia_items(a.leading_trivia(token));
        let trailing = a.trivia_items(a.trailing_trivia(token));
        if leading.len() + trailing.len() > MAX_TRIVIA {
            return None;
        }
        let mut trivia = [0u8; MAX_TRIVIA];
        for (slot, &t) in trivia.iter_mut().zip(leading.iter().chain(trailing)) {
            let width = a.trivia_width(t);
            if width >= EOL_BIT as u32 || !a.trivia_structure(t).is_null() || a.trivia_contains_diagnostics(t) {
                return None;
            }
            *slot = match a.trivia_kind(t) {
                SyntaxKind::WhitespaceTrivia => width as u8,
                SyntaxKind::EndOfLineTrivia => EOL_BIT | width as u8,
                _ => return None,
            };
        }
        Some(QuickToken {
            kind: a.kind(token),
            contextual: a.contextual_kind(token),
            width: a.width(token) as u8,
            leading: leading.len() as u8,
            trailing: trailing.len() as u8,
            trivia,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::green::GreenArena;
    use crate::lexer::{LexerBuffers, LexerMode, LexerOptions};

    /// The records of `text` lexed in `Syntax` mode up to the end of file, through `Lex` (`quick`: the quick scanner
    /// first, with `buffers`' map) or `LexSyntaxToken` alone: the token, trivia and trivia reference records, each
    /// token's diagnostics and values, and where the lexer ended.
    fn lex_all(text: &[u8], buffers: &mut LexerBuffers, quick: bool) -> String {
        let mut lexer = Lexer::new(text, LexerOptions::default(), GreenArena::new(), std::mem::take(buffers));
        let mut out = String::new();
        loop {
            let position = lexer.position();
            let token = if quick {
                lexer.lex(LexerMode::SYNTAX)
            } else {
                lexer.mode = LexerMode::SYNTAX;
                lexer.lex_syntax_token()
            };
            let a = lexer.arena_ref();
            let diagnostics: Vec<_> = a.diagnostics(token).map(|d| *a.diagnostic(d)).collect();
            out.push_str(&format!("{:?} {:?} {:?}\n", diagnostics, a.token_value(token), lexer.position()));
            if a.kind(token) == SyntaxKind::EndOfFileToken || (lexer.position() == position && a.full_width(token) == 0)
            {
                break;
            }
        }
        let (arena, back) = lexer.into_parts();
        *buffers = back;
        let r = arena.records();
        out.push_str(&format!("{:?}\n{:?}\n{:?}\n", r.tokens, r.trivia, r.trivia_refs));
        out
    }

    /// Deterministic variants of `text`: bytes the quick scanner stops at or accepts, inserted and replaced.
    fn variants(text: &[u8], seed: u64) -> Vec<Vec<u8>> {
        const BYTES: &[u8] = b" \t\r\n\x0b\x0c#/@$\"'\\.0123456789abcxyz_(){}[];,:<=>?!&|^~%*+-\xc3\xa9";
        let mut state = seed | 1;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        (0..3)
            .map(|_| {
                let mut v = text.to_vec();
                for _ in 0..1 + next() % 8 {
                    let at = (next() as usize) % (v.len() + 1);
                    let b = BYTES[(next() as usize) % BYTES.len()];
                    if next() % 2 == 0 || at == v.len() {
                        v.insert(at, b);
                    } else {
                        v[at] = b;
                    }
                }
                v
            })
            .collect()
    }

    /// A hit makes the records `LexSyntaxToken` makes, the map warm or cold, over the goldens and variants of them.
    #[test]
    fn a_hit_makes_the_records_lex_syntax_token_makes() {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/csharp");
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .flat_map(|d| std::fs::read_dir(d.path()).into_iter().flatten().flatten())
            .map(|f| f.path())
            .filter(|p| p.extension().is_some_and(|e| e == "cs"))
            .collect();
        files.sort();
        let mut inputs: Vec<Vec<u8>> = vec![
            b"class C { void M() { int x = 1; } }\r\n".to_vec(),
            b"  a  \n\n  b\r\n c\r d\n\n\n\n\n\n\n\n\n\ne ( ) ; .x 1 ".to_vec(),
        ];
        for (i, f) in files.iter().enumerate() {
            let text = std::fs::read(f).unwrap_or_default();
            inputs.extend(variants(&text, i as u64 + 1));
            inputs.push(text);
        }
        assert!(inputs.len() > 1000, "the goldens: {}", inputs.len());
        let (mut quick, mut slow) = (LexerBuffers::default(), LexerBuffers::default());
        for text in &inputs {
            let expected = lex_all(text, &mut slow, false);
            // cold for the text (but warm from the inputs before), then warm
            assert_eq!(lex_all(text, &mut quick, true), expected, "{}", String::from_utf8_lossy(text));
            assert_eq!(lex_all(text, &mut quick, true), expected, "{}", String::from_utf8_lossy(text));
        }
        let (lookups, hits) = (quick.cache.lookups.get(), quick.cache.hits.get());
        assert!(hits * 3 > lookups && lookups > 100_000, "{hits} hits of {lookups} lookups");
    }

    /// The full width the quick scanner accepts at the start of `text`, `None` when it leaves the text to
    /// `LexSyntaxToken`.
    fn accepted(text: &[u8]) -> Option<u32> {
        let mut lexer = Lexer::new(text, LexerOptions::default(), GreenArena::new(), LexerBuffers::default());
        let token = lexer.quick_scan_syntax_token()?;
        Some(lexer.arena_ref().full_width(token))
    }

    #[test]
    fn the_state_machine_accepts_what_roslyns_does() {
        for (text, width) in [
            (&b"a b"[..], 2),
            (b"class\nx", 6),
            (b"  x  \r\nint", 7),
            (b"\r\n\n x\n\n", 6),
            (b"( a", 2),
            (b"1 x", 2),
            (b"a.b", 1),
            (b"? .", 2),
            (b"-.5", 1),
            (b"x\n#if", 2),
            (b"x\r#if", 2),
        ] {
            assert_eq!(accepted(text), Some(width), "{}", String::from_utf8_lossy(text));
        }
        let long = [b'a'; MAX_CACHED_TOKEN_SIZE];
        for text in [
            &b"#if X\n"[..],
            b"/x",
            b"// c\nx ",
            b"a/ ",
            b"@x ",
            b"$\"a\" ",
            b"\"a\" ",
            b"'a' ",
            b"\\u0061 ",
            b"a\\u0061 ",
            "é ".as_bytes(),
            "aé ".as_bytes(),
            b".. ",
            b"..5",
            b".5 ",
            b"1. ",
            b"1.5 ",
            b"1e3 ",
            b"0x1 ",
            b"1_0 ",
            b"<= ",
            b"=> ",
            b"<<<<<<< a",
            b"a #if",
            // the text ends before the state machine does
            b"abc",
            &long,
        ] {
            assert_eq!(accepted(text), None, "{}", String::from_utf8_lossy(text));
        }
    }

    /// `Lex` asks the map in `Syntax` and `DebuggerSyntax` mode only, as Roslyn's does.
    #[test]
    fn the_scan_runs_only_in_the_syntax_modes() {
        let text = b"a b c d e f g h i j k l m n o p q r s t u v w x y z\n";
        let lookups = |mode: LexerMode| {
            let mut lexer = Lexer::new(text, LexerOptions::default(), GreenArena::new(), LexerBuffers::default());
            for _ in 0..8 {
                lexer.lex(mode);
            }
            lexer.buffers.cache.lookups.get()
        };
        for mode in [
            LexerMode::DIRECTIVE,
            LexerMode::XML_DOC_COMMENT,
            LexerMode::XML_ELEMENT_TAG,
            LexerMode::XML_CREF_QUOTE,
            LexerMode::XML_NAME_QUOTE,
        ] {
            assert_eq!(lookups(mode), 0, "{mode:?}");
        }
        assert_eq!(lookups(LexerMode::SYNTAX), 8);
        assert_eq!(lookups(LexerMode::DEBUGGER_SYNTAX), 8);
    }
}
