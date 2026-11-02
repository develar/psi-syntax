//! The port of java-syntax's JavaDoc lexer, `JavaDocLexer.kt`: `MergingLexerAdapter` (adjacent DOC_SPACE or
//! DOC_COMMENT_DATA tokens become one) over `AsteriskStripperLexer` (the leading `*` or `///` of each line, the space
//! after it) over `_JavaDocLexer.flex` (`FlexJavaDocLexer`, a hand port of the JFlex rules: in each state the longest
//! match among the rules of that state, the earlier rule on a tie).
//!
//! Offsets are bytes where the Kotlin code counts UTF-16 units (see `lexer`).

use psi_builder::lexer::char_at;

use crate::lexer::is_java_letter;
use crate::tokens::{SPACE_COMMENTS_TOKEN_SET, TokenKind};

/// The lexical states of `_JavaDocLexer` (`%state`), numbered as JFlex numbers them.
pub(crate) const YYINITIAL: u8 = 0;
pub(crate) const COMMENT_DATA_START: u8 = 2;
pub(crate) const COMMENT_DATA: u8 = 4;
pub(crate) const TAG_DOC_SPACE: u8 = 6;
pub(crate) const PARAM_TAG_SPACE: u8 = 8;
pub(crate) const DOC_TAG_VALUE: u8 = 10;
pub(crate) const DOC_TAG_VALUE_IN_PAREN: u8 = 12;
pub(crate) const DOC_TAG_VALUE_IN_LTGT: u8 = 14;
pub(crate) const INLINE_TAG_NAME: u8 = 16;
pub(crate) const SNIPPET_TAG_COMMENT_DATA_UNTIL_COLON: u8 = 18;
pub(crate) const INLINE_TAG_LITERAL_SPACE: u8 = 20;
pub(crate) const INLINE_TAG_LITERAL_TEXT: u8 = 22;
pub(crate) const SNIPPET_ATTRIBUTE_VALUE_DOUBLE_QUOTES: u8 = 24;
pub(crate) const SNIPPET_ATTRIBUTE_VALUE_SINGLE_QUOTES: u8 = 26;

/// `JavaDocLexer(level)` over a doc comment's text (`/** ... */` or `/// ...` lines): the tokens with their offsets.
pub struct JavaDocLexer<'t> {
    original: AsteriskStripperLexer<'t>,
    /// The merged token being returned: type, start (`MergingLexerAdapterBase.myTokenType`, `myTokenStart`).
    my_token: Option<(TokenKind, usize)>,
    located: bool,
}

impl<'t> JavaDocLexer<'t> {
    /// `JavaDocLexer(level)` (`isJdk15Enabled = level.isAtLeast(JDK_1_5)`), started on the whole `buffer`.
    pub fn new(is_jdk15_enabled: bool, buffer: &'t [u8]) -> Self {
        JavaDocLexer { original: AsteriskStripperLexer::new(is_jdk15_enabled, buffer), my_token: None, located: false }
    }

    /// The current token and moves past it: `(type, start, end)`, `None` at the end.
    pub fn next_token(&mut self) -> Option<(TokenKind, usize, usize)> {
        self.locate_token();
        let (kind, start) = self.my_token?;
        let end = self.original.get_token_start();
        self.my_token = None;
        self.located = false;
        Some((kind, start, end))
    }

    /// `MergingLexerAdapterBase.locateToken()` with `MergingLexerAdapter.merge`.
    fn locate_token(&mut self) {
        if self.located {
            return;
        }
        self.located = true;
        let Some(token_type) = self.original.get_token_type() else {
            self.my_token = None;
            return;
        };
        let start = self.original.get_token_start();
        self.original.advance();
        if SPACE_COMMENTS_TOKEN_SET.contains(token_type) {
            while self.original.get_token_type() == Some(token_type) {
                self.original.advance();
            }
        }
        self.my_token = Some((token_type, start));
    }
}

/// `AsteriskStripperLexer`.
pub(crate) struct AsteriskStripperLexer<'t> {
    my_flex: FlexJavaDocLexer,
    my_buffer: &'t [u8],
    my_buffer_index: usize,
    my_buffer_end_offset: usize,
    my_token_end_offset: usize,
    my_token_type: Option<TokenKind>,
    located: bool,
    my_after_line_break: bool,
    my_in_leading_space: bool,
    my_markdown_mode: bool,
}

impl<'t> AsteriskStripperLexer<'t> {
    /// The lexer started on `buffer` (`start(buffer, 0, buffer.length, 0)`).
    fn new(is_jdk15_enabled: bool, buffer: &'t [u8]) -> Self {
        let mut lexer = AsteriskStripperLexer {
            my_flex: FlexJavaDocLexer::new(is_jdk15_enabled),
            my_buffer: buffer,
            my_buffer_index: 0,
            my_buffer_end_offset: buffer.len(),
            my_token_end_offset: 0,
            my_token_type: None,
            located: false,
            my_after_line_break: false,
            my_in_leading_space: false,
            my_markdown_mode: false,
        };
        lexer.select_lexer_mode(0);
        lexer
    }

    fn get_token_type(&mut self) -> Option<TokenKind> {
        self.locate_token();
        self.my_token_type
    }

    fn get_token_start(&mut self) -> usize {
        self.locate_token();
        self.my_buffer_index
    }

    fn advance(&mut self) {
        self.locate_token();
        self.my_token_type = None;
        self.located = false;
    }

    /// `locateToken()`.
    fn locate_token(&mut self) {
        if self.located {
            return;
        }
        self.located = true;
        self.do_locate_token();

        if self.my_token_type == Some(TokenKind::DOC_SPACE) {
            let (start, end) = (self.my_buffer_index, self.my_token_end_offset);
            self.my_after_line_break = contain_line_breaks(&self.my_buffer[start..end.min(self.my_buffer.len())]);
        }
    }

    /// `selectLexerMode(buffer, startOffset)`: enables markdown mode if necessary.
    fn select_lexer_mode(&mut self, start_offset: usize) {
        self.my_markdown_mode = self.my_buffer[start_offset.min(self.my_buffer.len())..].starts_with(b"///");
        self.my_flex.reset(self.my_markdown_mode);
    }

    /// `doLocateToken()`.
    fn do_locate_token(&mut self) {
        if self.my_token_end_offset == self.my_buffer_end_offset {
            self.my_token_type = None;
            self.my_buffer_index = self.my_buffer_end_offset;
            return;
        }

        self.my_buffer_index = self.my_token_end_offset;

        if self.my_after_line_break {
            self.my_after_line_break = false;

            if self.my_markdown_mode {
                while self.detect_leading_slashes() {
                    self.my_token_end_offset += 3;
                }
            } else {
                while self.detect_leading_asterisks() {
                    self.my_token_end_offset += 1;
                }
            }

            self.my_in_leading_space = true;
            if self.my_buffer_index < self.my_token_end_offset {
                self.my_token_type = Some(TokenKind::DOC_COMMENT_LEADING_ASTERISKS);
                return;
            }
        }

        if self.my_in_leading_space {
            self.my_in_leading_space = false;
            let mut lf = false;
            while self.my_token_end_offset < self.my_buffer_end_offset {
                let (c, len) = char_at(self.my_buffer, self.my_token_end_offset);
                if !is_kotlin_whitespace(c) {
                    break;
                }
                if c == b'\n' as u32 {
                    lf = true;
                }
                self.my_token_end_offset += len;
            }

            let state = self.my_flex.yystate();
            if state == COMMENT_DATA
                || state != INLINE_TAG_LITERAL_TEXT
                    && self.my_token_end_offset < self.my_buffer_end_offset
                    && matches!(self.my_buffer[self.my_token_end_offset], b'@' | b'{' | b'"' | b'<')
            {
                self.my_flex.yybegin(COMMENT_DATA_START);
            }

            if self.my_buffer_index < self.my_token_end_offset {
                self.my_token_type = Some(
                    if lf
                        || state == PARAM_TAG_SPACE
                        || state == TAG_DOC_SPACE
                        || state == INLINE_TAG_NAME
                        || state == DOC_TAG_VALUE_IN_PAREN
                        || state == SNIPPET_TAG_COMMENT_DATA_UNTIL_COLON
                    {
                        TokenKind::DOC_SPACE
                    } else {
                        TokenKind::DOC_COMMENT_DATA
                    },
                );
                return;
            }
        }

        self.flex_locate_token();
    }

    /// `detectLeadingAsterisks()`: true for `*` or `*` not followed by `/`.
    fn detect_leading_asterisks(&self) -> bool {
        let p = self.my_token_end_offset;
        p < self.my_buffer_end_offset
            && self.my_buffer[p] == b'*'
            && (p + 1 >= self.my_buffer_end_offset || self.my_buffer[p + 1] != b'/')
    }

    /// `detectLeadingSlashes()`: the three leading slashes of a markdown line.
    fn detect_leading_slashes(&self) -> bool {
        let p = self.my_token_end_offset;
        p + 2 < self.my_buffer_end_offset && self.my_buffer[p..].starts_with(b"///")
    }

    /// `flexLocateToken()`.
    fn flex_locate_token(&mut self) {
        let (kind, end) = self.my_flex.advance(&self.my_buffer[..self.my_buffer_end_offset], self.my_buffer_index);
        self.my_token_type = Some(kind);
        self.my_token_end_offset = end;
    }
}

/// `CharArrayUtilKmp.containLineBreaks`.
fn contain_line_breaks(text: &[u8]) -> bool {
    text.iter().any(|&b| b == b'\n' || b == b'\r')
}

/// Kotlin's `Char.isWhitespace()` on the JVM: `Character.isWhitespace(c) || Character.isSpaceChar(c)`.
fn is_kotlin_whitespace(c: u32) -> bool {
    matches!(
        c,
        0x09..=0x0D | 0x1C..=0x20 | 0xA0 | 0x1680 | 0x2000..=0x200A | 0x2028 | 0x2029 | 0x202F | 0x205F | 0x3000
    )
}

/// The longest match of a state's rules: its rule number (the order in the spec) and length; an earlier rule wins a
/// tie.
#[derive(Default)]
struct Best {
    rule: u16,
    len: usize,
}

impl Best {
    fn offer(&mut self, rule: u16, len: usize) {
        if len > self.len || (len == self.len && len > 0 && rule < self.rule) {
            self.rule = rule;
            self.len = len;
        }
    }
}

/// `_JavaDocLexer` (_JavaDocLexer.flex), positioned per token (`goTo(offset)`, then `advance()`), with its lexical
/// state and the fields of its `%{ %}` block.
pub(crate) struct FlexJavaDocLexer {
    zz_lexical_state: u8,
    my_jdk1_5_enabled: bool,
    /// Nesting level of braces of inline tags having literal text.
    my_literal_text_content_braces_level: i32,
    /// Enable markdown support for java 23.
    my_markdown_mode: bool,
    /// Whether comment data should take into account spaces, only used with `my_markdown_mode`.
    comment_data_with_spaces: bool,
    /// Whether we're in a @value tag and should accept a FORMAT_STRING.
    in_value_tag: bool,
    /// Nesting level of generic types `<T>`, only used in DOC_TAG_VALUE outside of parentheses.
    generic_nesting_level: i32,
}

/// The bytes that end a `comment_data_run`: the line terminators of JFlex's `.` in ASCII (`\n`, `\r`, `\u000B`; `\f`
/// is `[\ \t\f]+`), and `{`, `}` and `*`, which start the rules whose tokens are not DOC_COMMENT_DATA (an inline tag's
/// start and end, `"*"+"/"`).
const DATA_STOP: [bool; 256] = {
    let mut table = [false; 256];
    let stops = b"\n\r\x0B{}*";
    let mut i = 0;
    while i < stops.len() {
        table[stops[i] as usize] = true;
        i += 1;
    }
    table
};

/// Outside markdown mode, in COMMENT_DATA (or in COMMENT_DATA_START at a byte that is not `[\ \t\f]` or `@`), every
/// rule that can match at a character which is not a `DATA_STOP` byte or a line terminator above ASCII returns
/// DOC_COMMENT_DATA and moves to (or stays in) COMMENT_DATA: `.`, `[\ \t\f]+` (`commentDataWithSpaces` is set only in
/// markdown mode), the escapes, the code fence and the punctuation rules; none of their matches extends over a
/// `DATA_STOP` byte. So the run of such characters merges (`MergingLexerAdapter`) into what its one-rule tokens would:
/// its end, the end of the text when nothing stops it.
fn comment_data_run(text: &[u8], pos: usize) -> usize {
    let mut p = pos;
    while let Some(&b) = text.get(p) {
        if b < 0x80 {
            if DATA_STOP[b as usize] {
                break;
            }
            p += 1;
        } else {
            let (c, len) = char_at(text, p);
            if !is_dot(c) {
                break;
            }
            p += len;
        }
    }
    p
}

/// `{WHITE_DOC_SPACE_CHAR}` = `[\ \t\f\n\r]`.
fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | 0x0C | b'\n' | b'\r')
}

/// JFlex's `.`: any character but the line terminators `[\n\r\u000B\u000C\u0085  ]`.
fn is_dot(c: u32) -> bool {
    !matches!(c, 0x0A | 0x0B | 0x0C | 0x0D | 0x85 | 0x2028 | 0x2029)
}

impl FlexJavaDocLexer {
    /// `_JavaDocLexer(isJdk1_5Enabled)`.
    pub(crate) fn new(is_jdk1_5_enabled: bool) -> Self {
        FlexJavaDocLexer {
            zz_lexical_state: YYINITIAL,
            my_jdk1_5_enabled: is_jdk1_5_enabled,
            my_literal_text_content_braces_level: 0,
            my_markdown_mode: false,
            comment_data_with_spaces: false,
            in_value_tag: false,
            generic_nesting_level: 0,
        }
    }

    /// `reset(buffer, start, end, YYINITIAL)` followed by `_reset(isMarkdownModeEnabled)`.
    pub(crate) fn reset(&mut self, is_markdown_mode_enabled: bool) {
        self.zz_lexical_state = YYINITIAL;
        self.my_markdown_mode = is_markdown_mode_enabled;
        self.my_literal_text_content_braces_level = 0;
        self.comment_data_with_spaces = false;
        self.in_value_tag = false;
        self.generic_nesting_level = 0;
    }

    pub(crate) fn yystate(&self) -> u8 {
        self.zz_lexical_state
    }

    pub(crate) fn yybegin(&mut self, state: u8) {
        self.zz_lexical_state = state;
    }

    /// `checkAhead(c)`: whether the character after the match (at `marked`) is `c`.
    fn check_ahead(text: &[u8], marked: usize, c: u8) -> bool {
        text.get(marked) == Some(&c)
    }

    /// `goTo(pos)` + `advance()`: the token at `pos < text.len()` and its end (equal to `pos` for the one rule that
    /// pushes its whole match back).
    pub(crate) fn advance(&mut self, text: &[u8], pos: usize) -> (TokenKind, usize) {
        use TokenKind::*;
        let state = self.zz_lexical_state;
        let byte = |i: usize| text.get(pos + i).copied();
        let b0 = text[pos];
        if !self.my_markdown_mode
            && !DATA_STOP[b0 as usize]
            && (state == COMMENT_DATA || (state == COMMENT_DATA_START && !matches!(b0, b' ' | b'\t' | 0x0C | b'@')))
        {
            let end = comment_data_run(text, pos);
            if end > pos {
                self.yybegin(COMMENT_DATA);
                return (DOC_COMMENT_DATA, end);
            }
        }
        let (c0, l0) = char_at(text, pos);
        let mut best = Best::default();

        // The rule numbers are the order of the rules in _JavaDocLexer.flex.
        let ws_run = |from: usize| text[from..].iter().take_while(|&&b| is_ws(b)).count();
        let run_while = |from: usize, f: &dyn Fn(u32) -> bool| -> usize {
            let mut p = from;
            while p < text.len() {
                let (c, l) = char_at(text, p);
                if !f(c) {
                    break;
                }
                p += l;
            }
            p - from
        };
        let starts = |s: &[u8]| text[pos..].starts_with(s);
        let is_cds = state == COMMENT_DATA_START;
        let is_cd = state == COMMENT_DATA;

        match state {
            YYINITIAL => {
                if starts(b"/**") {
                    best.offer(1, 3);
                }
                if starts(b"///") {
                    best.offer(2, 3);
                }
            }
            COMMENT_DATA_START | COMMENT_DATA => {
                if is_cds {
                    best.offer(3, ws_run(pos));
                }
                if is_cd {
                    best.offer(4, text[pos..].iter().take_while(|&&b| matches!(b, b' ' | b'\t' | 0x0C)).count());
                    let lr = text[pos..].iter().take_while(|&&b| b == b'\n' || b == b'\r').count();
                    if lr > 0 {
                        best.offer(5, lr + ws_run(pos + lr));
                    }
                }
                if is_cds && starts(b"@param") {
                    best.offer(17, 6);
                }
                // {CODE_FENCE}: 3 to 20 backticks or tildes
                if b0 == b'`' || b0 == b'~' {
                    let n = text[pos..].iter().take_while(|&&b| b == b0).count().min(20);
                    if n >= 3 {
                        best.offer(24, n);
                    }
                }
                if b0 == b'\\' {
                    match byte(1) {
                        Some(b'\\') => best.offer(25, 2),
                        Some(b'[') => best.offer(26, 2),
                        Some(b']') => best.offer(27, 2),
                        Some(b'(') => best.offer(28, 2),
                        Some(b')') => best.offer(29, 2),
                        Some(b'`') => best.offer(30, 2),
                        _ => {}
                    }
                }
                match b0 {
                    b',' => best.offer(31, 1),
                    b'`' => best.offer(32, 1),
                    b'#' => {
                        best.offer(33, 1);
                        if byte(1) == Some(b'#') {
                            best.offer(34, 2);
                        }
                    }
                    b'/' => best.offer(35, 1),
                    b'[' => best.offer(36, 1),
                    b']' => best.offer(37, 1),
                    b'(' => best.offer(38, 1),
                    b')' => best.offer(39, 1),
                    b'<' => best.offer(40, 1),
                    b'>' => best.offer(41, 1),
                    b'{' => best.offer(42, 1),
                    b'}' => best.offer(48, 1),
                    _ => {}
                }
                if is_dot(c0) {
                    best.offer(51, l0);
                }
                if is_cds && b0 == b'@' {
                    for name in [&b"author"[..], b"deprecated", b"hidden", b"index", b"implNote", b"return", b"serial"]
                    {
                        if text[pos + 1..].starts_with(name) {
                            best.offer(52, 1 + name.len());
                        }
                    }
                    let n = text[pos + 1..].iter().take_while(|&&b| !is_ws(b)).count();
                    if n > 0 {
                        best.offer(53, 1 + n);
                    }
                }
            }
            TAG_DOC_SPACE => {
                if b0 == b'}' {
                    best.offer(48, 1);
                }
                best.offer(75, ws_run(pos));
            }
            PARAM_TAG_SPACE => best.offer(18, ws_run(pos)),
            DOC_TAG_VALUE | DOC_TAG_VALUE_IN_PAREN => {
                let in_paren = state == DOC_TAG_VALUE_IN_PAREN;
                if !in_paren {
                    best.offer(6, ws_run(pos));
                }
                best.offer(
                    7,
                    run_while(pos, &|c| {
                        is_java_letter(c) || matches!(c, 0x30..=0x39 | 0x5F | 0x2E | 0x24 | 0x5B | 0x5D | 0x2D)
                    }),
                );
                if in_paren {
                    match b0 {
                        b')' => best.offer(9, 1),
                        b',' => best.offer(13, 1),
                        b'<' => best.offer(15, 1),
                        b'>' => best.offer(16, 1),
                        _ => {}
                    }
                    best.offer(14, ws_run(pos));
                } else {
                    match b0 {
                        b'(' => best.offer(8, 1),
                        b'/' => best.offer(10, 1),
                        b'#' => {
                            if byte(1) == Some(b'#') {
                                best.offer(11, 2);
                            }
                            best.offer(12, 1);
                        }
                        b',' => best.offer(13, 1),
                        b'<' => best.offer(19, 1),
                        b'}' => best.offer(48, 1),
                        _ => {}
                    }
                    if is_dot(c0) {
                        best.offer(51, l0);
                    }
                    // {FORMAT_STRING} = %[^\ \t\f\n\r}]+
                    if b0 == b'%' {
                        let n = text[pos + 1..].iter().take_while(|&&b| !is_ws(b) && b != b'}').count();
                        if n > 0 {
                            best.offer(74, 1 + n);
                        }
                    }
                }
            }
            DOC_TAG_VALUE_IN_LTGT => {
                match b0 {
                    b',' => best.offer(13, 1),
                    b'<' => best.offer(20, 1),
                    b'?' => best.offer(21, 1),
                    b'>' => best.offer(22, 1),
                    b' ' => best.offer(23, 1),
                    b'}' => best.offer(48, 1),
                    _ => {}
                }
                // {IDENTIFIER} = {ALPHA}({ALPHA}|{DIGIT}|[":.-"])*
                if is_java_letter(c0) {
                    best.offer(
                        21,
                        l0 + run_while(pos + l0, &|c| {
                            is_java_letter(c) || matches!(c, 0x30..=0x39 | 0x3A | 0x2E | 0x2D)
                        }),
                    );
                }
            }
            INLINE_TAG_NAME => {
                if b0 == b'@' {
                    let rest = &text[pos + 1..];
                    if rest.starts_with(b"param") {
                        best.offer(17, 6);
                    }
                    for name in [&b"code"[..], b"literal"] {
                        if rest.starts_with(name) {
                            best.offer(43, 1 + name.len());
                        }
                    }
                    for name in [&b"return"[..], b"summary"] {
                        if rest.starts_with(name) {
                            best.offer(44, 1 + name.len());
                        }
                    }
                    if rest.starts_with(b"snippet") {
                        best.offer(45, 8);
                    }
                    if rest.starts_with(b"value") {
                        best.offer(46, 6);
                    }
                    let n = rest.iter().take_while(|&&b| !is_ws(b) && b != b'}').count();
                    if n > 0 {
                        best.offer(47, 1 + n);
                    }
                }
                if b0 == b'}' {
                    best.offer(48, 1);
                }
            }
            SNIPPET_TAG_COMMENT_DATA_UNTIL_COLON => {
                if b0 == b'}' {
                    best.offer(48, 1);
                }
                // {IDENTIFIER_WITHOUT_SYMBOLS} = {ALPHA}({ALPHA}|{DIGIT})*
                if is_java_letter(c0) {
                    best.offer(60, l0 + run_while(pos + l0, &|c| is_java_letter(c) || matches!(c, 0x30..=0x39)));
                }
                match b0 {
                    b'=' => best.offer(61, 1),
                    b'"' => best.offer(62, 1),
                    b'\'' => best.offer(63, 1),
                    b':' => best.offer(64, 1),
                    b'*' => best.offer(65, 1),
                    _ => {}
                }
                if starts(b"///") {
                    best.offer(66, 3);
                }
                best.offer(67, ws_run(pos));
                if b0 != b'*' {
                    best.offer(68, l0);
                }
                if b0 != b'/' {
                    best.offer(69, l0);
                }
            }
            INLINE_TAG_LITERAL_SPACE => {
                if b0 == b'}' {
                    best.offer(48, 1);
                }
                best.offer(49, ws_run(pos));
                if is_dot(c0) {
                    best.offer(50, l0);
                }
            }
            INLINE_TAG_LITERAL_TEXT => {
                match b0 {
                    b'}' => best.offer(70, 1),
                    b'{' => best.offer(71, 1),
                    _ => {}
                }
                best.offer(72, ws_run(pos));
                best.offer(73, text[pos..].iter().take_while(|&&b| !matches!(b, b'{' | b'}' | b'\n')).count());
            }
            SNIPPET_ATTRIBUTE_VALUE_DOUBLE_QUOTES | SNIPPET_ATTRIBUTE_VALUE_SINGLE_QUOTES => {
                let quote = if state == SNIPPET_ATTRIBUTE_VALUE_DOUBLE_QUOTES { b'"' } else { b'\'' };
                let base = if quote == b'"' { 54 } else { 57 };
                if b0 == b'}' {
                    best.offer(48, 1);
                }
                best.offer(base, ws_run(pos));
                best.offer(base + 1, text[pos..].iter().take_while(|&&b| b != b'\n' && b != quote).count());
                if b0 == quote {
                    best.offer(base + 2, 1);
                }
            }
            _ => {}
        }
        // "*"+"/" in every state
        let stars = text[pos..].iter().take_while(|&&b| b == b'*').count();
        if stars > 0 && byte(stars) == Some(b'/') {
            best.offer(76, stars + 1);
        }
        // [^] in every state
        best.offer(77, l0);

        let end = pos + best.len;
        let kind = match best.rule {
            1 => {
                if self.my_markdown_mode {
                    DOC_COMMENT_BAD_CHARACTER
                } else {
                    self.yybegin(COMMENT_DATA_START);
                    DOC_COMMENT_START
                }
            }
            2 => {
                if self.my_markdown_mode {
                    self.yybegin(COMMENT_DATA_START);
                    DOC_COMMENT_LEADING_ASTERISKS
                } else {
                    DOC_COMMENT_BAD_CHARACTER
                }
            }
            3 => DOC_SPACE,
            4 => {
                if self.comment_data_with_spaces {
                    DOC_SPACE
                } else {
                    DOC_COMMENT_DATA
                }
            }
            5 => DOC_SPACE,
            6 => {
                self.yybegin(COMMENT_DATA);
                DOC_SPACE
            }
            7 => DOC_TAG_VALUE_TOKEN,
            8 => {
                self.yybegin(DOC_TAG_VALUE_IN_PAREN);
                DOC_TAG_VALUE_LPAREN
            }
            9 => {
                self.yybegin(DOC_TAG_VALUE);
                DOC_TAG_VALUE_RPAREN
            }
            10 => DOC_TAG_VALUE_SLASH,
            11 => DOC_TAG_VALUE_DOUBLE_SHARP_TOKEN,
            12 => DOC_TAG_VALUE_SHARP_TOKEN,
            13 => DOC_TAG_VALUE_COMMA,
            14 => DOC_SPACE,
            15 => DOC_TAG_VALUE_LT,
            16 => DOC_TAG_VALUE_GT,
            17 => {
                self.yybegin(PARAM_TAG_SPACE);
                DOC_TAG_NAME
            }
            18 => {
                self.yybegin(DOC_TAG_VALUE);
                DOC_SPACE
            }
            19 => {
                if self.my_jdk1_5_enabled {
                    self.yybegin(DOC_TAG_VALUE_IN_LTGT);
                    self.generic_nesting_level += 1;
                    DOC_TAG_VALUE_LT
                } else {
                    self.yybegin(COMMENT_DATA);
                    DOC_COMMENT_DATA
                }
            }
            20 => {
                self.generic_nesting_level += 1;
                DOC_TAG_VALUE_LT
            }
            21 => DOC_TAG_VALUE_TOKEN,
            22 => {
                self.generic_nesting_level -= 1;
                if self.generic_nesting_level <= 0 {
                    self.yybegin(COMMENT_DATA);
                }
                DOC_TAG_VALUE_GT
            }
            23 => DOC_SPACE,
            24 => {
                self.yybegin(COMMENT_DATA);
                if self.my_markdown_mode { DOC_CODE_FENCE } else { DOC_COMMENT_DATA }
            }
            25..=30 => {
                self.yybegin(COMMENT_DATA);
                DOC_COMMENT_DATA
            }
            31..=41 => {
                self.yybegin(COMMENT_DATA);
                if self.my_markdown_mode {
                    match best.rule {
                        31 => DOC_COMMA,
                        32 => DOC_INLINE_CODE_FENCE,
                        33 => DOC_SHARP,
                        34 => DOC_DOUBLE_SHARP,
                        35 => DOC_TAG_VALUE_SLASH,
                        36 => {
                            self.comment_data_with_spaces = true;
                            DOC_LBRACKET
                        }
                        37 => {
                            self.comment_data_with_spaces = false;
                            DOC_RBRACKET
                        }
                        38 => DOC_LPAREN,
                        39 => DOC_RPAREN,
                        40 => DOC_LT,
                        _ => DOC_GT,
                    }
                } else {
                    DOC_COMMENT_DATA
                }
            }
            42 => {
                if Self::check_ahead(text, end, b'@') {
                    self.yybegin(INLINE_TAG_NAME);
                } else {
                    self.yybegin(COMMENT_DATA);
                }
                DOC_INLINE_TAG_START
            }
            43 => {
                self.yybegin(INLINE_TAG_LITERAL_SPACE);
                DOC_TAG_NAME
            }
            44 => {
                self.yybegin(DOC_TAG_VALUE);
                DOC_TAG_NAME
            }
            45 => {
                self.yybegin(SNIPPET_TAG_COMMENT_DATA_UNTIL_COLON);
                DOC_TAG_NAME
            }
            46 => {
                self.in_value_tag = true;
                self.yybegin(TAG_DOC_SPACE);
                DOC_TAG_NAME
            }
            47 => {
                self.yybegin(TAG_DOC_SPACE);
                DOC_TAG_NAME
            }
            48 => {
                self.in_value_tag = false;
                self.yybegin(COMMENT_DATA);
                DOC_INLINE_TAG_END
            }
            49 => {
                self.yybegin(INLINE_TAG_LITERAL_TEXT);
                DOC_SPACE
            }
            50 => {
                self.yybegin(INLINE_TAG_LITERAL_TEXT);
                DOC_COMMENT_DATA
            }
            51 => {
                self.yybegin(COMMENT_DATA);
                DOC_COMMENT_DATA
            }
            52 => {
                self.yybegin(DOC_TAG_VALUE);
                DOC_TAG_NAME
            }
            53 => {
                self.yybegin(TAG_DOC_SPACE);
                DOC_TAG_NAME
            }
            54 | 57 => DOC_SPACE,
            55 | 58 => DOC_TAG_VALUE_TOKEN,
            56 | 59 => {
                self.yybegin(SNIPPET_TAG_COMMENT_DATA_UNTIL_COLON);
                DOC_TAG_VALUE_QUOTE
            }
            60 | 61 => DOC_TAG_VALUE_TOKEN,
            62 => {
                self.yybegin(SNIPPET_ATTRIBUTE_VALUE_DOUBLE_QUOTES);
                DOC_TAG_VALUE_QUOTE
            }
            63 => {
                self.yybegin(SNIPPET_ATTRIBUTE_VALUE_SINGLE_QUOTES);
                DOC_TAG_VALUE_QUOTE
            }
            64 => {
                self.yybegin(INLINE_TAG_LITERAL_TEXT);
                DOC_TAG_VALUE_COLON
            }
            65 => {
                if self.my_markdown_mode {
                    DOC_COMMENT_DATA
                } else {
                    DOC_COMMENT_LEADING_ASTERISKS
                }
            }
            66 => {
                if self.my_markdown_mode {
                    DOC_COMMENT_LEADING_ASTERISKS
                } else {
                    DOC_COMMENT_DATA
                }
            }
            67 => DOC_SPACE,
            68 | 69 => {
                self.yybegin(SNIPPET_TAG_COMMENT_DATA_UNTIL_COLON);
                DOC_COMMENT_DATA
            }
            70 => {
                if self.my_literal_text_content_braces_level > 0 {
                    self.my_literal_text_content_braces_level -= 1;
                    DOC_COMMENT_DATA
                } else {
                    self.yybegin(COMMENT_DATA);
                    DOC_INLINE_TAG_END
                }
            }
            71 => {
                self.my_literal_text_content_braces_level += 1;
                DOC_COMMENT_DATA
            }
            72 => DOC_SPACE,
            73 => DOC_COMMENT_DATA,
            74 => {
                if self.in_value_tag {
                    self.in_value_tag = false;
                    self.yybegin(TAG_DOC_SPACE);
                    DOC_TAG_FORMAT_STRING
                } else {
                    // yypushback(yylength()): the token is empty
                    self.yybegin(COMMENT_DATA);
                    return (DOC_COMMENT_DATA, pos);
                }
            }
            75 => {
                if Self::check_ahead(text, end, b'<') || Self::check_ahead(text, end, b'"') {
                    self.yybegin(COMMENT_DATA);
                } else if Self::check_ahead(text, end, b'{') {
                    // lbrace - there's a error in JLex when typing lbrace directly
                    self.yybegin(COMMENT_DATA);
                } else {
                    self.yybegin(DOC_TAG_VALUE);
                }
                DOC_SPACE
            }
            76 => {
                if self.my_markdown_mode {
                    DOC_COMMENT_DATA
                } else {
                    DOC_COMMENT_END
                }
            }
            _ => DOC_COMMENT_BAD_CHARACTER,
        };
        (kind, end)
    }
}

/// Lexes a doc comment's `text` with `JavaDocLexer` into `types` and `starts` (cleared first; one more start than
/// types: the last is the text length).
pub fn tokenize_into(text: &[u8], is_jdk15_enabled: bool, types: &mut Vec<TokenKind>, starts: &mut Vec<u32>) {
    types.clear();
    starts.clear();
    let text = &text[..text.len().min(u32::MAX as usize)];
    let mut lexer = JavaDocLexer::new(is_jdk15_enabled, text);
    while let Some((kind, start, _)) = lexer.next_token() {
        types.push(kind);
        starts.push(start as u32);
    }
    starts.push(text.len() as u32);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(text: &str) -> Vec<(TokenKind, &str)> {
        let mut lexer = JavaDocLexer::new(true, text.as_bytes());
        let mut out = Vec::new();
        while let Some((kind, start, end)) = lexer.next_token() {
            out.push((kind, &text[start..end]));
        }
        out
    }

    #[test]
    fn html_comment() {
        use TokenKind::*;
        assert_eq!(
            lex("/**\n * name{name} abc{}dfg\n */"),
            vec![
                (DOC_COMMENT_START, "/**"),
                (DOC_SPACE, "\n "),
                (DOC_COMMENT_LEADING_ASTERISKS, "*"),
                (DOC_COMMENT_DATA, " name"),
                (DOC_INLINE_TAG_START, "{"),
                (DOC_COMMENT_DATA, "name"),
                (DOC_INLINE_TAG_END, "}"),
                (DOC_COMMENT_DATA, " abc"),
                (DOC_INLINE_TAG_START, "{"),
                (DOC_INLINE_TAG_END, "}"),
                (DOC_COMMENT_DATA, "dfg"),
                (DOC_SPACE, "\n "),
                (DOC_COMMENT_END, "*/"),
            ]
        );
    }
}
