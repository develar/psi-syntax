//! The port of java-syntax's Java lexer: `JavaLexer.kt` (whitespace, comments, character and string literals, text
//! blocks, string templates, unicode escapes, by hand) over `_JavaLexer.flex` (numbers, keywords, identifiers,
//! operators: `FlexJavaLexer`, a hand port of the JFlex rules: longest match, the earlier rule on a tie), and
//! `JavaTypeEscapeLexer.kt`.
//!
//! Offsets are bytes where the Kotlin code counts UTF-16 units: a character is one step (`mySymbolLength` is its byte
//! length, or the length of its unicode escape), a byte that does not start valid UTF-8 is one character (U+FFFD).
//! The code only compares characters with ASCII ones, and a backslash never occurs inside a multi-byte sequence, so the
//! token boundaries are the same.

use psi_builder::lexer::{Keyword, char_at, find_any3, find_either, in_ranges};

use crate::language_level::LanguageLevel;
use crate::tokens::TokenKind;
use crate::unicode::{JAVA_LETTER, JAVA_LETTER_DIGIT};

/// The tokens of a text: types, start offsets (one more than tokens: the last is the text length) and, per token,
/// whether it started in the lexer's initial state with an empty state stack.
pub type Lexed = psi_builder::Lexed<TokenKind>;

/// `STATE_DEFAULT`.
const STATE_DEFAULT: i32 = 0;
/// `STATE_TEXT_BLOCK_TEMPLATE`.
const STATE_TEXT_BLOCK_TEMPLATE: i32 = 1;

/// `LiteralType`: the quote that ends the literal (`c`).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum LiteralType {
    String,
    Char,
    TextBlock,
}

impl LiteralType {
    fn c(self) -> u32 {
        match self {
            LiteralType::String | LiteralType::TextBlock => b'"' as u32,
            LiteralType::Char => b'\'' as u32,
        }
    }
}

/// `JavaLexer`: a lexer over `buffer[start..end]`, positioned with `start` and moved with `advance`.
pub struct JavaLexer<'t> {
    my_flex_lexer: FlexJavaLexer,
    my_state_stack: Vec<i32>,
    my_buffer: &'t [u8],
    my_buffer_index: usize,
    my_buffer_end_offset: usize,
    /// Positioned after the last symbol of the current token.
    my_token_end_offset: usize,
    my_token_type: Option<TokenKind>,
    /// State at the start of the current token; `get_state` reflects this, not the post-mutation top of the stack.
    my_state_before_token: i32,
    /// Whether the state stack was `[STATE_DEFAULT]` at the start of the current token (not in the Kotlin code: what
    /// `Lexed::is_plain` records).
    my_plain_before_token: bool,
    /// The length of the last valid unicode escape (6 or greater), or of the last character when none was found.
    my_symbol_length: usize,
    /// Whether `my_token_type` holds the located token (the Kotlin code tests `myTokenType != null`, and keeps a null
    /// type at the end of the buffer).
    located: bool,
}

impl<'t> JavaLexer<'t> {
    /// `JavaLexer(level)`, started on the whole `buffer` (`start(buffer)`).
    pub fn new(level: LanguageLevel, buffer: &'t [u8]) -> Self {
        let mut lexer = JavaLexer {
            my_flex_lexer: FlexJavaLexer::new(level),
            my_state_stack: Vec::with_capacity(1),
            my_buffer: buffer,
            my_buffer_index: 0,
            my_buffer_end_offset: 0,
            my_token_end_offset: 0,
            my_token_type: None,
            my_state_before_token: STATE_DEFAULT,
            my_plain_before_token: true,
            my_symbol_length: 1,
            located: false,
        };
        lexer.start(buffer, 0, buffer.len(), STATE_DEFAULT);
        lexer
    }

    /// `start(buffer, startOffset, endOffset, initialState)`.
    pub fn start(&mut self, buffer: &'t [u8], start_offset: usize, end_offset: usize, initial_state: i32) {
        self.my_buffer = buffer;
        self.my_buffer_index = start_offset;
        self.my_buffer_end_offset = end_offset.min(buffer.len());
        self.my_token_type = None;
        self.located = false;
        self.my_token_end_offset = start_offset;
        self.my_symbol_length = 1;
        self.my_state_stack.clear();
        self.my_state_stack.push(initial_state);
        self.my_state_before_token = initial_state;
        self.my_plain_before_token = initial_state == STATE_DEFAULT;
    }

    /// `getState()`.
    pub fn get_state(&self) -> i32 {
        self.my_state_before_token
    }

    /// `getTokenType()`: `None` at the end.
    pub fn get_token_type(&mut self) -> Option<TokenKind> {
        self.locate_token();
        self.my_token_type
    }

    /// `getTokenStart()`.
    pub fn get_token_start(&mut self) -> usize {
        self.locate_token();
        self.my_buffer_index
    }

    /// `getTokenEnd()`.
    pub fn get_token_end(&mut self) -> usize {
        self.locate_token();
        self.my_token_end_offset
    }

    /// `advance()`.
    pub fn advance(&mut self) {
        self.locate_token();
        self.my_token_type = None;
        self.located = false;
    }

    /// Whether the current token started in the initial state with the state stack `[STATE_DEFAULT]`, and does not
    /// start with a backslash that a backslash before it could escape (`locate_char_at` looks back over them), so the
    /// text from it lexes alone into the same tokens.
    pub fn token_is_plain(&mut self) -> bool {
        self.locate_token();
        self.my_plain_before_token
            && !(self.my_buffer.get(self.my_buffer_index) == Some(&b'\\')
                && self.my_buffer_index > 0
                && self.my_buffer[self.my_buffer_index - 1] == b'\\')
    }

    /// The current token and moves past it: `(type, start, end)`, `None` at the end.
    pub fn next_token(&mut self) -> Option<(TokenKind, usize, usize)> {
        let kind = self.get_token_type()?;
        let token = (kind, self.my_buffer_index, self.my_token_end_offset);
        self.advance();
        Some(token)
    }

    /// `locateToken()`: handles whitespace, comment, string literal, text block and string template tokens; other
    /// tokens are handled by calling the flex lexer.
    fn locate_token(&mut self) {
        if self.located {
            return;
        }
        self.located = true;

        if self.my_token_end_offset == self.my_buffer_end_offset {
            self.my_token_type = None;
            self.my_buffer_index = self.my_buffer_end_offset;
            return;
        }

        self.my_state_before_token = self.top_int();
        self.my_plain_before_token = self.my_state_stack.len() == 1 && self.my_state_before_token == STATE_DEFAULT;
        self.my_buffer_index = self.my_token_end_offset;

        let c = self.locate_char_at(self.my_buffer_index);
        match c {
            // allowed only at the end of files
            0x1A => {
                if self.my_buffer_index + self.my_symbol_length == self.my_buffer_end_offset {
                    self.my_token_type = Some(TokenKind::WHITE_SPACE);
                    self.my_token_end_offset = self.my_buffer_index + self.my_symbol_length;
                } else {
                    self.flex_locate_token();
                }
            }
            0x20 | 0x09 | 0x0A | 0x0D | 0x0C => {
                self.my_token_type = Some(TokenKind::WHITE_SPACE);
                self.my_token_end_offset = self.get_whitespaces(self.my_buffer_index + self.my_symbol_length);
            }
            0x7B => {
                // '{'
                let count1 = self.top_int() >> 16;
                if count1 > 0 {
                    let state = self.pop_int();
                    self.push((state & STATE_TEXT_BLOCK_TEMPLATE) | (count1.wrapping_add(1) << 16));
                }
                self.my_token_type = Some(TokenKind::LBRACE);
                self.my_token_end_offset = self.my_buffer_index + self.my_symbol_length;
            }
            0x7D => {
                // '}'
                let count2 = self.top_int() >> 16;
                let mut update_token = true;
                if count2 > 0 {
                    if count2 != 1 {
                        let state = self.pop_int();
                        self.push((state & STATE_TEXT_BLOCK_TEMPLATE) | (count2.wrapping_sub(1) << 16));
                    } else {
                        let state = self.pop_int();
                        if self.my_state_stack.is_empty() {
                            self.push(STATE_DEFAULT);
                        }
                        if (state & STATE_TEXT_BLOCK_TEMPLATE) != 0 {
                            let fragment = self.locate_literal_end(
                                self.my_buffer_index + self.my_symbol_length,
                                LiteralType::TextBlock,
                            );
                            self.my_token_type = Some(if fragment {
                                TokenKind::TEXT_BLOCK_TEMPLATE_MID
                            } else {
                                TokenKind::TEXT_BLOCK_TEMPLATE_END
                            });
                        } else {
                            let fragment = self
                                .locate_literal_end(self.my_buffer_index + self.my_symbol_length, LiteralType::String);
                            self.my_token_type = Some(if fragment {
                                TokenKind::STRING_TEMPLATE_MID
                            } else {
                                TokenKind::STRING_TEMPLATE_END
                            });
                        }
                        update_token = false;
                    }
                }
                if update_token {
                    self.my_token_type = Some(TokenKind::RBRACE);
                    self.my_token_end_offset = self.my_buffer_index + self.my_symbol_length;
                }
            }
            0x2F => {
                // '/'
                if self.my_buffer_index + self.my_symbol_length >= self.my_buffer_end_offset {
                    self.my_token_type = Some(TokenKind::DIV);
                    self.my_token_end_offset = self.my_buffer_end_offset;
                } else {
                    let l1 = self.my_symbol_length;
                    let next_char = self.locate_char_at(self.my_buffer_index + l1);
                    if next_char == b'/' as u32 {
                        let l2 = self.my_symbol_length;
                        if self.my_buffer_index + l1 + l2 < self.my_buffer_end_offset
                            && self.locate_char_at(self.my_buffer_index + l1 + l2) == b'/' as u32
                        {
                            // Java 23 Markdown comments
                            self.my_token_type = Some(TokenKind::DOC_MARKDOWN_COMMENT);
                            self.my_token_end_offset =
                                self.get_closing_markdown_comment(self.my_buffer_index + l1 + l2);
                        } else {
                            self.my_token_type = Some(TokenKind::END_OF_LINE_COMMENT);
                            self.my_token_end_offset = self.get_line_terminator(self.my_buffer_index + l1 + l2);
                        }
                    } else if next_char == b'*' as u32 {
                        let l2 = self.my_symbol_length;
                        if self.my_buffer_index + l1 + l2 < self.my_buffer_end_offset
                            && self.locate_char_at(self.my_buffer_index + l1 + l2) == b'*' as u32
                        {
                            let l3 = self.my_symbol_length;
                            if self.my_buffer_index + l1 + l2 + l3 < self.my_buffer_end_offset
                                && self.locate_char_at(self.my_buffer_index + l1 + l2 + l3) == b'/' as u32
                            {
                                self.my_token_type = Some(TokenKind::C_STYLE_COMMENT);
                                self.my_token_end_offset = self.my_buffer_index + l1 + l2 + l3 + self.my_symbol_length;
                            } else {
                                self.my_token_type = Some(TokenKind::DOC_COMMENT);
                                self.my_token_end_offset =
                                    self.get_closing_comment(self.my_buffer_index + l1 + l2 + l3);
                            }
                        } else {
                            self.my_token_type = Some(TokenKind::C_STYLE_COMMENT);
                            self.my_token_end_offset =
                                self.get_closing_comment(self.my_buffer_index + l1 + l2 + self.my_symbol_length);
                        }
                    } else {
                        self.flex_locate_token();
                    }
                }
            }
            0x23 => {
                // '#'
                if self.my_buffer_index == 0
                    && self.my_symbol_length == 1
                    && self.my_buffer_end_offset > 1
                    && self.char_at(1) == b'!' as u32
                {
                    self.my_token_type = Some(TokenKind::END_OF_LINE_COMMENT);
                    self.my_token_end_offset = self.get_line_terminator(2);
                } else {
                    self.flex_locate_token();
                }
            }
            0x27 => {
                // '\''
                self.my_token_type = Some(TokenKind::CHARACTER_LITERAL);
                self.locate_literal_end(self.my_buffer_index + self.my_symbol_length, LiteralType::Char);
            }
            0x22 => {
                // '"'
                let l1 = self.my_symbol_length;
                if self.my_buffer_index + l1 < self.my_buffer_end_offset
                    && self.locate_char_at(self.my_buffer_index + l1) == b'"' as u32
                {
                    let l2 = self.my_symbol_length;
                    if self.my_buffer_index + l1 + l2 < self.my_buffer_end_offset
                        && self.locate_char_at(self.my_buffer_index + l1 + l2) == b'"' as u32
                    {
                        let fragment = self.locate_literal_end(
                            self.my_buffer_index + l1 + l2 + self.my_symbol_length,
                            LiteralType::TextBlock,
                        );
                        self.my_token_type = Some(if fragment {
                            TokenKind::TEXT_BLOCK_TEMPLATE_BEGIN
                        } else {
                            TokenKind::TEXT_BLOCK_LITERAL
                        });
                    } else {
                        self.my_token_type = Some(TokenKind::STRING_LITERAL);
                        self.my_token_end_offset = self.my_buffer_index + l1 + l2;
                    }
                } else {
                    let fragment = self.locate_literal_end(self.my_buffer_index + l1, LiteralType::String);
                    self.my_token_type =
                        Some(if fragment { TokenKind::STRING_TEMPLATE_BEGIN } else { TokenKind::STRING_LITERAL });
                }
            }
            _ => self.flex_locate_token(),
        }

        if self.my_token_end_offset > self.my_buffer_end_offset {
            self.my_token_end_offset = self.my_buffer_end_offset;
        }
    }

    /// `getWhitespaces(offset)`. The run of plain whitespace bytes is skipped first (each is one character);
    /// `get_chars` goes on only where a backslash (a unicode escape) or `\u001a` could continue it.
    #[inline(always)]
    fn get_whitespaces(&mut self, offset: usize) -> usize {
        let buffer = &self.my_buffer[..self.my_buffer_end_offset];
        let mut pos = offset;
        while let Some(&b) = buffer.get(pos) {
            if !ASCII_WHITE_SPACE[b as usize] {
                if b == b'\\' || b == 0x1A {
                    return self.get_chars(pos, b" \t\n\r\x0C", Some(0x1A));
                }
                break;
            }
            pos += 1;
        }
        pos
    }

    /// `getSimpleWhitespaces(offset)`.
    fn get_simple_whitespaces(&mut self, offset: usize) -> usize {
        self.get_chars(offset, b" \t", None)
    }

    /// `getChars(offset, charsToDetect, endChar)`: the position after the run of `chars_to_detect` from `offset`;
    /// `end_char` counts only as the last character of the buffer.
    fn get_chars(&mut self, offset: usize, chars_to_detect: &[u8], end_char: Option<u32>) -> usize {
        let mut pos = offset;
        while pos < self.my_buffer_end_offset {
            let mut detected = false;
            let c = self.locate_char_at(pos);
            for &d in chars_to_detect {
                if d as u32 == c {
                    pos += self.my_symbol_length;
                    detected = true;
                    break;
                }
            }
            if !detected
                && let Some(end_char) = end_char
                && pos + self.my_symbol_length == self.my_buffer_end_offset
                && end_char == c
            {
                pos += self.my_symbol_length;
                break;
            }
            if !detected {
                break;
            }
        }
        pos
    }

    /// `flexLocateToken()`.
    fn flex_locate_token(&mut self) {
        let (kind, end) =
            self.my_flex_lexer.advance(&self.my_buffer[..self.my_buffer_end_offset], self.my_buffer_index);
        self.my_token_type = Some(kind);
        self.my_token_end_offset = end;
    }

    /// `locateLiteralEnd(offset, literalType)`: sets the token end; `true` if this is a string template fragment.
    /// Bytes that are not a backslash, a quote, CR or LF are skipped without decoding (see `get_closing_comment`).
    fn locate_literal_end(&mut self, offset: usize, literal_type: LiteralType) -> bool {
        let mut pos = offset;

        while pos < self.my_buffer_end_offset {
            let buffer = &self.my_buffer[..self.my_buffer_end_offset];
            while buffer.get(pos).is_some_and(|&b| !LITERAL_STOP[b as usize]) {
                pos += 1;
            }
            if pos >= self.my_buffer_end_offset {
                break;
            }
            let c = self.locate_char_at(pos);

            if c == b'\\' as u32 {
                pos += self.my_symbol_length;
                // on (encoded) backslash we also need to skip the next symbol (e.g. \\u005c" is translated to \")
                if pos < self.my_buffer_end_offset
                    && self.locate_char_at(pos) == b'{' as u32
                    && literal_type != LiteralType::Char
                {
                    pos += self.my_symbol_length;
                    self.my_token_end_offset = pos;
                    if self.top_int() == 0 {
                        self.pop_int();
                    }
                    if literal_type == LiteralType::TextBlock {
                        self.push(STATE_TEXT_BLOCK_TEMPLATE | (1 << 16));
                    } else {
                        self.push(STATE_DEFAULT | (1 << 16));
                    }
                    return true;
                }
            } else if c == literal_type.c() {
                if literal_type == LiteralType::TextBlock {
                    pos += self.my_symbol_length;
                    if pos < self.my_buffer_end_offset && self.locate_char_at(pos) == b'"' as u32 {
                        pos += self.my_symbol_length;
                        if pos < self.my_buffer_end_offset && self.locate_char_at(pos) == b'"' as u32 {
                            self.my_token_end_offset = pos + self.my_symbol_length;
                            return false;
                        }
                    }
                    continue;
                } else {
                    self.my_token_end_offset = pos + self.my_symbol_length;
                    return false;
                }
            } else if (c == b'\n' as u32 || c == b'\r' as u32)
                && self.my_symbol_length == 1
                && literal_type != LiteralType::TextBlock
            {
                self.my_token_end_offset = pos;
                return false;
            }
            pos += self.my_symbol_length;
        }
        self.my_token_end_offset = pos;
        false
    }

    /// `getClosingMarkdownComment(offset)`.
    fn get_closing_markdown_comment(&mut self, offset: usize) -> usize {
        let mut pos = offset;
        while pos < self.my_buffer_end_offset {
            pos = self.get_line_terminator(pos);
            // Account for whitespaces beforehand
            let new_pos = self.get_simple_whitespaces(pos + self.my_symbol_length);

            match self.get_char_seq_at(new_pos, b"///") {
                None => break,
                Some(new_pos) => pos = new_pos,
            }
        }
        pos
    }

    /// `getClosingComment(offset)`. Bytes other than `*` and a backslash are skipped without decoding: such a byte is
    /// not `*` and starts no unicode escape, and an ASCII byte is never inside a multi-byte character.
    fn get_closing_comment(&mut self, offset: usize) -> usize {
        let mut pos = offset;

        while pos < self.my_buffer_end_offset {
            pos = find_either(&self.my_buffer[..self.my_buffer_end_offset], pos, b'*', b'\\');
            if pos >= self.my_buffer_end_offset {
                break;
            }
            let c = self.locate_char_at(pos);
            pos += self.my_symbol_length;
            if c == b'*' as u32 && pos < self.my_buffer_end_offset && self.locate_char_at(pos) == b'/' as u32 {
                break;
            }
        }

        pos + self.my_symbol_length
    }

    /// `getLineTerminator(offset)`. Bytes other than CR, LF and a backslash are skipped without decoding (see
    /// `get_closing_comment`).
    fn get_line_terminator(&mut self, offset: usize) -> usize {
        let mut pos = offset;

        while pos < self.my_buffer_end_offset {
            pos = find_any3(&self.my_buffer[..self.my_buffer_end_offset], pos, b'\n', b'\r', b'\\');
            if pos >= self.my_buffer_end_offset {
                break;
            }
            let c = self.locate_char_at(pos);
            if c == b'\r' as u32 || c == b'\n' as u32 {
                break;
            }
            pos += self.my_symbol_length;
        }

        pos
    }

    /// `getCharSeqAt(offset, charSequence)`: the position after the sequence, `None` (-1) when it is not there.
    fn get_char_seq_at(&mut self, offset: usize, char_sequence: &[u8]) -> Option<usize> {
        let mut pos = offset;
        for &c in char_sequence {
            if !self.is_located_char_at(pos, c as u32) {
                return None;
            }
            pos += self.my_symbol_length;
        }
        Some(pos)
    }

    /// `isLocatedCharAt(offset, charToDetect)`.
    fn is_located_char_at(&mut self, offset: usize, char_to_detect: u32) -> bool {
        offset < self.my_buffer_end_offset && self.locate_char_at(offset) == char_to_detect
    }

    /// `charAt(offset)`: the character at `offset` (its code point; U+FFFD for an invalid byte).
    fn char_at(&self, offset: usize) -> u32 {
        if offset < self.my_buffer.len() { char_at(self.my_buffer, offset).0 } else { 0 }
    }

    /// `locateCharAt(offset)`: the character at `offset`, a unicode escape (`\uXXXX`, any number of `u`) decoded;
    /// `my_symbol_length` becomes its length. An ASCII byte other than a backslash is itself (inline); the rest is
    /// `locate_char_at_slow`.
    #[inline(always)]
    fn locate_char_at(&mut self, offset: usize) -> u32 {
        if let Some(&b) = self.my_buffer.get(offset)
            && b < 0x80
            && b != b'\\'
        {
            self.my_symbol_length = 1;
            return b as u32;
        }
        self.locate_char_at_slow(offset)
    }

    /// `locate_char_at` of a backslash, a non-ASCII byte or an offset past the buffer.
    #[inline(never)]
    fn locate_char_at_slow(&mut self, offset: usize) -> u32 {
        let (first, len) = if offset < self.my_buffer.len() { char_at(self.my_buffer, offset) } else { (0, 1) };
        self.my_symbol_length = len;
        if first != b'\\' as u32 {
            return first;
        }
        let buffer = self.my_buffer;
        let end = self.my_buffer_end_offset;
        let byte = |i: usize| buffer.get(i).copied().unwrap_or(0);
        let mut pos = offset + 1;
        if pos < end && byte(pos) == b'\\' {
            return first;
        }
        let mut escaped = true;
        let mut i = offset;
        while i > 0 {
            i -= 1;
            if byte(i) != b'\\' {
                break;
            }
            escaped = !escaped;
        }
        if !escaped {
            return first;
        }
        if pos < end && byte(pos) != b'u' {
            return first;
        }
        loop {
            pos += 1;
            if !(pos < end && byte(pos) == b'u') {
                break;
            }
        }
        if pos + 3 >= end {
            return first;
        }
        let mut result = 0u32;
        let max = pos + 4;
        while pos < max {
            result <<= 4;
            let c = byte(pos);
            match c {
                b'0'..=b'9' => result += (c - b'0') as u32,
                b'a'..=b'f' => result += (c - b'a') as u32 + 10,
                b'A'..=b'F' => result += (c - b'A') as u32 + 10,
                _ => return first,
            }
            pos += 1;
        }
        self.my_symbol_length = pos - offset;
        result
    }

    fn top_int(&self) -> i32 {
        self.my_state_stack.last().copied().unwrap_or(STATE_DEFAULT)
    }

    fn push(&mut self, value: i32) {
        self.my_state_stack.push(value);
    }

    fn pop_int(&mut self) -> i32 {
        self.my_state_stack.pop().unwrap_or(STATE_DEFAULT)
    }
}

/// `_JavaLexer` (_JavaLexer.flex), positioned per token (`goTo(offset)`, then `advance()`): numbers, keywords,
/// identifiers and operators; any other character is one BAD_CHARACTER. The rules are matched as JFlex matches them:
/// the longest match, and on a tie the earlier rule (the numbers, then the keywords, then `{IDENTIFIER}`, then the
/// operators).
pub(crate) struct FlexJavaLexer {
    my_assert_keyword: bool,
    my_enum_keyword: bool,
}

impl FlexJavaLexer {
    /// `_JavaLexer(level)`.
    pub(crate) fn new(level: LanguageLevel) -> Self {
        FlexJavaLexer {
            my_assert_keyword: level.is_at_least(LanguageLevel::JDK_1_4),
            my_enum_keyword: level.is_at_least(LanguageLevel::JDK_1_5),
        }
    }

    /// `goTo(pos)` + `advance()`: the token at `pos < text.len()` and its end.
    #[inline(always)]
    pub(crate) fn advance(&self, text: &[u8], pos: usize) -> (TokenKind, usize) {
        let b = text[pos];
        if ASCII_JAVA_LETTER[b as usize] {
            let end = identifier_end(text, pos + 1);
            return (self.keyword_or_identifier_at(text, pos, end), end);
        }
        if b.is_ascii_digit() || (b == b'.' && text.get(pos + 1).is_some_and(u8::is_ascii_digit)) {
            return number(text, pos);
        }
        if b >= 0x80 {
            let (cp, len) = char_at(text, pos);
            if is_java_letter(cp) {
                // the keywords are ASCII
                return (TokenKind::IDENTIFIER, identifier_end(text, pos + len));
            }
            return (TokenKind::BAD_CHARACTER, pos + len);
        }
        operator(text, pos).unwrap_or((TokenKind::BAD_CHARACTER, pos + 1))
    }

    /// The keyword rules before `{IDENTIFIER}` at `text[start..end]`: the keyword of exactly this text, else
    /// IDENTIFIER (`assert` and `enum` below their levels too). Through `KEYWORDS`, a perfect hash of `KEYWORD_LIST`:
    /// one candidate, compared as one or two words read from `text`.
    #[inline(always)]
    fn keyword_or_identifier_at(&self, text: &[u8], start: usize, end: usize) -> TokenKind {
        let len = end - start;
        if !(2..=12).contains(&len) || !text[start].is_ascii_lowercase() {
            return TokenKind::IDENTIFIER;
        }
        let entry = &KEYWORDS[keyword_hash(text[start], text[start + 1], text[end - 1], len)];
        if !entry.matches(text, start, end) {
            return TokenKind::IDENTIFIER;
        }
        match entry.kind {
            TokenKind::ASSERT_KEYWORD if !self.my_assert_keyword => TokenKind::IDENTIFIER,
            TokenKind::ENUM_KEYWORD if !self.my_enum_keyword => TokenKind::IDENTIFIER,
            kind => kind,
        }
    }
}

/// `[:jletter:]`: `Character.isJavaIdentifierStart` as JFlex's tables have it.
#[inline(always)]
pub(crate) fn is_java_letter(cp: u32) -> bool {
    if cp < 0x80 {
        let b = cp as u8;
        b.is_ascii_alphabetic() || b == b'$' || b == b'_'
    } else {
        in_ranges_cold(&JAVA_LETTER, cp)
    }
}

/// `[:jletterdigit:]`: `Character.isJavaIdentifierPart` as JFlex's tables have it (in ASCII also the identifier-ignorable
/// controls 0x00-0x08, 0x0E-0x1B and 0x7F).
#[inline(always)]
pub(crate) fn is_java_letter_digit(cp: u32) -> bool {
    if cp < 0x80 {
        let b = cp as u8;
        b.is_ascii_alphanumeric() || b == b'$' || b == b'_' || b <= 0x08 || (0x0E..=0x1B).contains(&b) || b == 0x7F
    } else {
        in_ranges_cold(&JAVA_LETTER_DIGIT, cp)
    }
}

/// `in_ranges` above ASCII, out of line: the ASCII tests stay inlined in the per-character loops.
#[cold]
#[inline(never)]
fn in_ranges_cold(ranges: &[(u32, u32)], cp: u32) -> bool {
    in_ranges(ranges, cp)
}

/// The end of `[:jletterdigit:]*` from `pos`: ASCII through `ASCII_JAVA_LETTER_DIGIT`, other characters decoded.
#[inline(always)]
fn identifier_end(text: &[u8], mut pos: usize) -> usize {
    loop {
        while let Some(&b) = text.get(pos) {
            if !ASCII_JAVA_LETTER_DIGIT[b as usize] {
                break;
            }
            pos += 1;
        }
        if text.get(pos).is_none_or(|&b| b < 0x80) {
            return pos;
        }
        let (cp, len) = char_at(text, pos);
        if !is_java_letter_digit(cp) {
            return pos;
        }
        pos += len;
    }
}

/// `is_java_letter` of each ASCII byte; false at 0x80 and above.
const ASCII_JAVA_LETTER: [bool; 256] = {
    let mut table = [false; 256];
    let mut b = 0;
    while b < 0x80 {
        let c = b as u8;
        table[b] = c.is_ascii_alphabetic() || c == b'$' || c == b'_';
        b += 1;
    }
    table
};

/// `is_java_letter_digit` of each ASCII byte; false at 0x80 and above.
const ASCII_JAVA_LETTER_DIGIT: [bool; 256] = {
    let mut table = [false; 256];
    let mut b = 0;
    while b < 0x80 {
        let c = b as u8;
        table[b] = c.is_ascii_alphanumeric() || c == b'$' || c == b'_' || c <= 0x08 || (c >= 0x0E && c <= 0x1B) || c == 0x7F;
        b += 1;
    }
    table
};

/// The bytes `getWhitespaces` detects: ` `, `\t`, `\n`, `\r`, `\f`.
const ASCII_WHITE_SPACE: [bool; 256] = {
    let mut table = [false; 256];
    table[b' ' as usize] = true;
    table[b'\t' as usize] = true;
    table[b'\n' as usize] = true;
    table[b'\r' as usize] = true;
    table[0x0C] = true;
    table
};

/// The bytes at which `locate_literal_end` decodes: a backslash, the quotes, CR and LF.
const LITERAL_STOP: [bool; 256] = {
    let mut table = [false; 256];
    table[b'\\' as usize] = true;
    table[b'"' as usize] = true;
    table[b'\'' as usize] = true;
    table[b'\n' as usize] = true;
    table[b'\r' as usize] = true;
    table
};

/// The keyword rules of `_JavaLexer.flex` (`assert` and `enum` are identifiers below their levels).
const KEYWORD_LIST: [(&[u8], TokenKind); 53] = {
    use TokenKind::*;
    [
        (b"true", TRUE_KEYWORD),
        (b"false", FALSE_KEYWORD),
        (b"null", NULL_KEYWORD),
        (b"abstract", ABSTRACT_KEYWORD),
        (b"assert", ASSERT_KEYWORD),
        (b"boolean", BOOLEAN_KEYWORD),
        (b"break", BREAK_KEYWORD),
        (b"byte", BYTE_KEYWORD),
        (b"case", CASE_KEYWORD),
        (b"catch", CATCH_KEYWORD),
        (b"char", CHAR_KEYWORD),
        (b"class", CLASS_KEYWORD),
        (b"const", CONST_KEYWORD),
        (b"continue", CONTINUE_KEYWORD),
        (b"default", DEFAULT_KEYWORD),
        (b"do", DO_KEYWORD),
        (b"double", DOUBLE_KEYWORD),
        (b"else", ELSE_KEYWORD),
        (b"enum", ENUM_KEYWORD),
        (b"extends", EXTENDS_KEYWORD),
        (b"final", FINAL_KEYWORD),
        (b"finally", FINALLY_KEYWORD),
        (b"float", FLOAT_KEYWORD),
        (b"for", FOR_KEYWORD),
        (b"goto", GOTO_KEYWORD),
        (b"if", IF_KEYWORD),
        (b"implements", IMPLEMENTS_KEYWORD),
        (b"import", IMPORT_KEYWORD),
        (b"instanceof", INSTANCEOF_KEYWORD),
        (b"int", INT_KEYWORD),
        (b"interface", INTERFACE_KEYWORD),
        (b"long", LONG_KEYWORD),
        (b"native", NATIVE_KEYWORD),
        (b"new", NEW_KEYWORD),
        (b"package", PACKAGE_KEYWORD),
        (b"private", PRIVATE_KEYWORD),
        (b"public", PUBLIC_KEYWORD),
        (b"short", SHORT_KEYWORD),
        (b"super", SUPER_KEYWORD),
        (b"switch", SWITCH_KEYWORD),
        (b"synchronized", SYNCHRONIZED_KEYWORD),
        (b"this", THIS_KEYWORD),
        (b"throw", THROW_KEYWORD),
        (b"protected", PROTECTED_KEYWORD),
        (b"transient", TRANSIENT_KEYWORD),
        (b"return", RETURN_KEYWORD),
        (b"void", VOID_KEYWORD),
        (b"static", STATIC_KEYWORD),
        (b"strictfp", STRICTFP_KEYWORD),
        (b"while", WHILE_KEYWORD),
        (b"try", TRY_KEYWORD),
        (b"volatile", VOLATILE_KEYWORD),
        (b"throws", THROWS_KEYWORD),
    ]
};

/// The slot of a keyword candidate in `KEYWORDS`: its first, second and last byte and its length.
#[inline(always)]
const fn keyword_hash(first: u8, second: u8, last: u8, len: usize) -> usize {
    (first as usize * 2 + second as usize * 18 + last as usize * 19 + len * 20) & 127
}

/// `KEYWORD_LIST` by `keyword_hash`; an empty slot has length 0. Building it fails to compile on a collision.
const KEYWORDS: [Keyword<TokenKind>; 128] = {
    let mut table = [Keyword::empty(TokenKind::IDENTIFIER); 128];
    let mut i = 0;
    while i < KEYWORD_LIST.len() {
        let (text, kind) = KEYWORD_LIST[i];
        let len = text.len();
        let slot = keyword_hash(text[0], text[1], text[len - 1], len);
        assert!(table[slot].len == 0, "keyword_hash collides");
        table[slot] = Keyword::new(text, kind);
        i += 1;
    }
    table
};

/// The end of `[_0-9]*` from `pos`.
fn digits_or_underscores(text: &[u8], mut pos: usize) -> usize {
    while text.get(pos).is_some_and(|&b| b.is_ascii_digit() || b == b'_') {
        pos += 1;
    }
    pos
}

/// The end of `[_0-9A-Fa-f]*` from `pos`.
fn hex_digits_or_underscores(text: &[u8], mut pos: usize) -> usize {
    while text.get(pos).is_some_and(|&b| b.is_ascii_hexdigit() || b == b'_') {
        pos += 1;
    }
    pos
}

fn at(text: &[u8], pos: usize, set: &[u8]) -> bool {
    text.get(pos).is_some_and(|b| set.contains(b))
}

/// `{DEC_EXPONENT}` or `{HEX_EXPONENT}` (`[Ee]` or `[Pp]`, then `[+-]? {DIGIT_OR_UNDERSCORE}*`) at `pos`: its end.
fn exponent_end(text: &[u8], pos: usize, letters: &[u8]) -> Option<usize> {
    if !at(text, pos, letters) {
        return None;
    }
    let mut end = pos + 1;
    if at(text, end, b"+-") {
        end += 1;
    }
    Some(digits_or_underscores(text, end))
}

/// The numeric rules (`{LONG_LITERAL}`, `{INTEGER_LITERAL}`, `{FLOAT_LITERAL}`, `{DOUBLE_LITERAL}`, in this order) at
/// `pos`, which starts with a digit or with `.` and a digit: the longest match, the earlier rule on a tie.
fn number(text: &[u8], pos: usize) -> (TokenKind, usize) {
    use TokenKind::*;
    // (rule order, end) of the best match so far
    let mut best: Option<(u8, usize)> = None;
    let mut offer = |rule: u8, end: usize| {
        if best.is_none_or(|(r, e)| end > e || (end == e && rule < r)) {
            best = Some((rule, end));
        }
    };
    const LONG: u8 = 0;
    const INTEGER: u8 = 1;
    const FLOAT: u8 = 2;
    const DOUBLE: u8 = 3;
    // A floating-point literal without its suffix ends at `end`: DOUBLE there, or with [Ff] / [Dd] after it.
    let fp = |end: usize, offer: &mut dyn FnMut(u8, usize)| {
        offer(DOUBLE, end);
        if at(text, end, b"Ff") {
            offer(FLOAT, end + 1);
        }
        if at(text, end, b"Dd") {
            offer(DOUBLE, end + 1);
        }
    };

    if text[pos] == b'.' {
        // DEC_SIGNIFICAND = "." {DIGITS}, then {DEC_EXPONENT}?
        let digits = digits_or_underscores(text, pos + 2);
        fp(digits, &mut offer);
        if let Some(e) = exponent_end(text, digits, b"Ee") {
            fp(e, &mut offer);
        }
    } else {
        // {DIGITS}
        let digits = digits_or_underscores(text, pos + 1);
        offer(INTEGER, digits);
        if at(text, digits, b"Ll") {
            offer(LONG, digits + 1);
        }
        if at(text, digits, b"Ff") {
            offer(FLOAT, digits + 1);
        }
        if at(text, digits, b"Dd") {
            offer(DOUBLE, digits + 1);
        }
        // {DIGITS} {DEC_EXPONENT}
        if let Some(e) = exponent_end(text, digits, b"Ee") {
            fp(e, &mut offer);
        }
        // DEC_SIGNIFICAND = {DIGITS} "." {DIGIT_OR_UNDERSCORE}*, then {DEC_EXPONENT}?
        if at(text, digits, b".") {
            let fraction = digits_or_underscores(text, digits + 1);
            fp(fraction, &mut offer);
            if let Some(e) = exponent_end(text, fraction, b"Ee") {
                fp(e, &mut offer);
            }
        }
        if text[pos] == b'0' && at(text, pos + 1, b"Xx") {
            // HEX_INTEGER_LITERAL = 0 [Xx] {HEX_DIGIT_OR_UNDERSCORE}*
            let hex = hex_digits_or_underscores(text, pos + 2);
            offer(INTEGER, hex);
            if at(text, hex, b"Ll") {
                offer(LONG, hex + 1);
            }
            // HEX_SIGNIFICAND = 0 [Xx] ({HEX_DIGIT_OR_UNDERSCORE}+ "."? | {HEX_DIGIT_OR_UNDERSCORE}* "." {HEX_DIGIT_OR_UNDERSCORE}+)
            let mut significands = [None, None, None];
            if hex > pos + 2 {
                significands[0] = Some(hex);
                if at(text, hex, b".") {
                    significands[1] = Some(hex + 1);
                }
            }
            if at(text, hex, b".") {
                let fraction = hex_digits_or_underscores(text, hex + 1);
                if fraction > hex + 1 {
                    significands[2] = Some(fraction);
                }
            }
            // HEX_FP_LITERAL = {HEX_SIGNIFICAND} {HEX_EXPONENT}
            for significand in significands.into_iter().flatten() {
                if let Some(e) = exponent_end(text, significand, b"Pp") {
                    fp(e, &mut offer);
                }
            }
        } else if text[pos] == b'0' && at(text, pos + 1, b"Bb") {
            // BIN_INTEGER_LITERAL = 0 [Bb] {DIGIT_OR_UNDERSCORE}*
            let bin = digits_or_underscores(text, pos + 2);
            offer(INTEGER, bin);
            if at(text, bin, b"Ll") {
                offer(LONG, bin + 1);
            }
        }
    }
    let (rule, end) = best.unwrap_or((INTEGER, pos + 1));
    let kind = match rule {
        LONG => LONG_LITERAL,
        INTEGER => INTEGER_LITERAL,
        FLOAT => FLOAT_LITERAL,
        _ => DOUBLE_LITERAL,
    };
    (kind, end)
}

/// The operator rules at `pos`: the longest that matches.
fn operator(text: &[u8], pos: usize) -> Option<(TokenKind, usize)> {
    use TokenKind::*;
    let next = |i: usize| text.get(pos + i).copied().unwrap_or(0);
    let (kind, len) = match (text[pos], next(1)) {
        (b'=', b'=') => (EQEQ, 2),
        (b'!', b'=') => (NE, 2),
        (b'|', b'|') => (OROR, 2),
        (b'+', b'+') => (PLUSPLUS, 2),
        (b'-', b'-') => (MINUSMINUS, 2),
        (b'<', b'=') => (LE, 2),
        (b'<', b'<') => {
            if next(2) == b'=' {
                (LTLTEQ, 3)
            } else {
                (LTLT, 2)
            }
        }
        (b'<', _) => (LT, 1),
        (b'>', _) => (GT, 1),
        (b'&', b'&') => (ANDAND, 2),
        (b'+', b'=') => (PLUSEQ, 2),
        (b'-', b'=') => (MINUSEQ, 2),
        (b'*', b'=') => (ASTERISKEQ, 2),
        (b'/', b'=') => (DIVEQ, 2),
        (b'&', b'=') => (ANDEQ, 2),
        (b'|', b'=') => (OREQ, 2),
        (b'^', b'=') => (XOREQ, 2),
        (b'%', b'=') => (PERCEQ, 2),
        (b'&', _) => (AND, 1),
        (b'(', _) => (LPARENTH, 1),
        (b')', _) => (RPARENTH, 1),
        (b'{', _) => (LBRACE, 1),
        (b'}', _) => (RBRACE, 1),
        (b'[', _) => (LBRACKET, 1),
        (b']', _) => (RBRACKET, 1),
        (b';', _) => (SEMICOLON, 1),
        (b',', _) => (COMMA, 1),
        (b'.', b'.') if next(2) == b'.' => (ELLIPSIS, 3),
        (b'.', _) => (DOT, 1),
        (b'=', _) => (EQ, 1),
        (b'!', _) => (EXCL, 1),
        (b'~', _) => (TILDE, 1),
        (b'?', _) => (QUEST, 1),
        (b':', b':') => (DOUBLE_COLON, 2),
        (b':', _) => (COLON, 1),
        (b'+', _) => (PLUS, 1),
        (b'-', b'>') => (ARROW, 2),
        (b'-', _) => (MINUS, 1),
        (b'*', _) => (ASTERISK, 1),
        (b'/', _) => (DIV, 1),
        (b'|', _) => (OR, 1),
        (b'^', _) => (XOR, 1),
        (b'%', _) => (PERC, 1),
        (b'@', _) => (AT, 1),
        _ => return None,
    };
    Some((kind, pos + len))
}

/// Lexes `text` (at most `u32::MAX` bytes are lexed; offsets are `u32`) at `level` into token types and starts, as
/// `TokenSequence.performLexing` does with `JavaLexer`.
pub fn tokenize(text: &[u8], level: LanguageLevel) -> Lexed {
    let mut lexed = Lexed::default();
    tokenize_into(text, level, &mut lexed);
    lexed
}

/// `tokenize` into `lexed`, which is cleared first (its buffers are reused).
pub fn tokenize_into(text: &[u8], level: LanguageLevel, lexed: &mut Lexed) {
    let text = &text[..text.len().min(u32::MAX as usize)];
    let mut out = lexed.writer(text.len(), TokenKind::BAD_CHARACTER);
    let mut lexer = JavaLexer::new(level, text);
    let flex = FlexJavaLexer::new(level);
    let end = text.len();
    let mut pos = 0;
    // What the state stack says at a token start, which only `locate_token` changes: whether it is `[STATE_DEFAULT]`
    // (a plain token), and whether its top counts no template braces (`{` and `}` are then LBRACE and RBRACE).
    let mut plain = true;
    let mut braces_plain = true;
    while pos < end {
        let b = text[pos];
        let (kind, token_end) = match BYTE_CLASS[b as usize] {
            ByteClass::Letter => {
                let token_end = identifier_end(text, pos + 1);
                (flex.keyword_or_identifier_at(text, pos, token_end), token_end)
            }
            ByteClass::Space => {
                let mut token_end = pos + 1;
                while text.get(token_end).is_some_and(|&b| ASCII_WHITE_SPACE[b as usize]) {
                    token_end += 1;
                }
                if text.get(token_end).is_some_and(|&b| b == b'\\' || b == 0x1A) {
                    token_end = lexer.get_chars(token_end, b" \t\n\r\x0C", Some(0x1A));
                }
                (TokenKind::WHITE_SPACE, token_end)
            }
            ByteClass::Single(kind) => (kind, pos + 1),
            ByteClass::Brace(kind) if braces_plain => (kind, pos + 1),
            ByteClass::Operator => flex.advance(text, pos),
            ByteClass::Digit => number(text, pos),
            _ => {
                // `locateToken` from `pos` (and the backslash rule of `token_is_plain`)
                lexer.my_token_end_offset = pos;
                lexer.located = false;
                let kind = lexer.get_token_type().unwrap_or(TokenKind::BAD_CHARACTER);
                out.push(kind, pos as u32, lexer.token_is_plain());
                pos = lexer.my_token_end_offset;
                let top = lexer.top_int();
                plain = lexer.my_state_stack.len() == 1 && top == STATE_DEFAULT;
                braces_plain = top >> 16 == 0;
                continue;
            }
        };
        out.push(kind, pos as u32, plain);
        pos = token_end;
    }
    out.end(text.len() as u32);
}

/// How `tokenize_into` lexes a token by its first byte without `locate_token`: what `locateCharAt` returns for an
/// ASCII byte other than a backslash is that byte, so `locateToken`'s own rules (whitespace, braces, comments,
/// literals, `#`, `\u001a`) see it as it is and the rest go to `FlexJavaLexer`.
#[derive(Copy, Clone)]
enum ByteClass {
    /// `locate_token`: a backslash, a byte at 0x80 or above, `/`, the quotes, `#`, `\u001a`, the controls.
    Slow,
    /// `[:jletter:]` in ASCII: an identifier or keyword.
    Letter,
    /// A digit: `number`.
    Digit,
    /// Whitespace: the run `getWhitespaces` takes.
    Space,
    /// A one-character token of `FlexJavaLexer` that starts no longer one.
    Single(TokenKind),
    /// `{` or `}`, which `locateToken` counts when the state's top counts template braces.
    Brace(TokenKind),
    /// The other operators and `.`: `FlexJavaLexer::advance`.
    Operator,
}

const BYTE_CLASS: [ByteClass; 256] = {
    use TokenKind::*;
    let mut table = [ByteClass::Slow; 256];
    let mut b = 0;
    while b < 0x80 {
        let c = b as u8;
        table[b] = if c.is_ascii_alphabetic() || c == b'$' || c == b'_' {
            ByteClass::Letter
        } else if c.is_ascii_digit() {
            ByteClass::Digit
        } else {
            match c {
                b' ' | b'\t' | b'\n' | b'\r' | 0x0C => ByteClass::Space,
                b'(' => ByteClass::Single(LPARENTH),
                b')' => ByteClass::Single(RPARENTH),
                b'[' => ByteClass::Single(LBRACKET),
                b']' => ByteClass::Single(RBRACKET),
                b';' => ByteClass::Single(SEMICOLON),
                b',' => ByteClass::Single(COMMA),
                b'@' => ByteClass::Single(AT),
                b'~' => ByteClass::Single(TILDE),
                b'?' => ByteClass::Single(QUEST),
                b'{' => ByteClass::Brace(LBRACE),
                b'}' => ByteClass::Brace(RBRACE),
                b'.' | b'=' | b'!' | b'|' | b'+' | b'-' | b'<' | b'>' | b'&' | b'*' | b'^' | b'%' | b':' => {
                    ByteClass::Operator
                }
                _ => ByteClass::Slow,
            }
        };
        b += 1;
    }
    table
};

/// `JavaTypeEscapeLexer(JavaLexer(level))` over `text` into `lexed`: a BAD_CHARACTER followed by `[` or `]` is
/// merged into that bracket (escaped array brackets of markdown JavaDoc reference links, `char\[\]`).
/// (`MergingLexerAdapterBase.merge`: the merged token keeps the bracket's type and spans both.)
pub fn tokenize_type_escaped_into(text: &[u8], level: LanguageLevel, lexed: &mut Lexed) {
    let text = &text[..text.len().min(u32::MAX as usize)];
    lexed.begin(text.len());
    let mut lexer = JavaLexer::new(level, text);
    while let Some(token_type) = lexer.get_token_type() {
        let start = lexer.get_token_start();
        let plain = lexer.token_is_plain();
        lexer.advance();
        let mut kind = token_type;
        if token_type == TokenKind::BAD_CHARACTER {
            let original_token_type = lexer.get_token_type();
            if original_token_type == Some(TokenKind::LBRACKET) || original_token_type == Some(TokenKind::RBRACKET) {
                lexer.advance();
                kind = original_token_type.unwrap_or(token_type);
            }
        }
        lexed.push(kind, start as u32, plain);
    }
    lexed.end(text.len() as u32);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(text: &str) -> Vec<(TokenKind, &str)> {
        let mut lexer = JavaLexer::new(LanguageLevel::HIGHEST, text.as_bytes());
        let mut out = Vec::new();
        while let Some((kind, start, end)) = lexer.next_token() {
            out.push((kind, &text[start..end]));
        }
        out
    }

    #[test]
    fn numbers() {
        use TokenKind::*;
        for (text, kind) in [
            ("1", INTEGER_LITERAL),
            ("1_000", INTEGER_LITERAL),
            ("0x", INTEGER_LITERAL),
            ("0xFFL", LONG_LITERAL),
            ("0b1012", INTEGER_LITERAL),
            ("1e", DOUBLE_LITERAL),
            ("1e+", DOUBLE_LITERAL),
            ("1.", DOUBLE_LITERAL),
            (".5f", FLOAT_LITERAL),
            ("1.f", FLOAT_LITERAL),
            ("1f", FLOAT_LITERAL),
            ("1D", DOUBLE_LITERAL),
            ("0x1p", DOUBLE_LITERAL),
            ("0x1.8p1f", FLOAT_LITERAL),
            ("0x.8p1", DOUBLE_LITERAL),
            ("0x1.p0d", DOUBLE_LITERAL),
        ] {
            assert_eq!(lex(text), vec![(kind, text)], "{text}");
        }
        assert_eq!(lex("1e5L"), vec![(DOUBLE_LITERAL, "1e5"), (IDENTIFIER, "L")]);
        assert_eq!(lex("0x."), vec![(INTEGER_LITERAL, "0x"), (DOT, ".")]);
        assert_eq!(lex("1.2.3"), vec![(DOUBLE_LITERAL, "1.2"), (DOUBLE_LITERAL, ".3")]);
    }

    #[test]
    fn keywords() {
        for (text, kind) in KEYWORD_LIST {
            let text = std::str::from_utf8(text).unwrap();
            assert_eq!(lex(text), vec![(kind, text)]);
            let lexed = tokenize(text.as_bytes(), LanguageLevel::HIGHEST);
            assert_eq!(lexed.types, [kind], "{text}");
        }
        let low = tokenize(b"assert enum", LanguageLevel::JDK_1_3);
        assert_eq!(low.types, [TokenKind::IDENTIFIER, TokenKind::WHITE_SPACE, TokenKind::IDENTIFIER]);
        for word in ["a", "Class", "synchronizedx", "implement", "instanceOf", "nul", "ifx", "x_if", "$if"] {
            assert_eq!(tokenize(word.as_bytes(), LanguageLevel::HIGHEST).types, [TokenKind::IDENTIFIER], "{word}");
        }
    }

    #[test]
    fn comments_literals_and_templates() {
        use TokenKind::*;
        assert_eq!(lex("/**/"), vec![(C_STYLE_COMMENT, "/**/")]);
        assert_eq!(lex("/*/"), vec![(C_STYLE_COMMENT, "/*/")]);
        assert_eq!(lex("/** a */x"), vec![(DOC_COMMENT, "/** a */"), (IDENTIFIER, "x")]);
        assert_eq!(
            lex("/// a\n  /// b\nx"),
            vec![(DOC_MARKDOWN_COMMENT, "/// a\n  /// b"), (WHITE_SPACE, "\n"), (IDENTIFIER, "x")]
        );
        assert_eq!(
            lex("\"a\\{b}c\""),
            vec![(STRING_TEMPLATE_BEGIN, "\"a\\{"), (IDENTIFIER, "b"), (STRING_TEMPLATE_END, "}c\"")]
        );
        assert_eq!(lex("\"\"\"\na\"\"\""), vec![(TEXT_BLOCK_LITERAL, "\"\"\"\na\"\"\"")]);
        assert_eq!(lex("'a\n"), vec![(CHARACTER_LITERAL, "'a"), (WHITE_SPACE, "\n")]);
        assert_eq!(lex("\\u0041"), vec![(BAD_CHARACTER, "\\"), (IDENTIFIER, "u0041")]);
        assert_eq!(lex("a\\u0020b"), vec![(IDENTIFIER, "a"), (WHITE_SPACE, "\\u0020"), (IDENTIFIER, "b")]);
        assert_eq!(lex("#!x\ny"), vec![(END_OF_LINE_COMMENT, "#!x"), (WHITE_SPACE, "\n"), (IDENTIFIER, "y")]);
        assert_eq!(lex("a\x1a"), vec![(IDENTIFIER, "a\x1a")]);
        assert_eq!(lex(" \x1a"), vec![(WHITE_SPACE, " \x1a")]);
        assert_eq!(lex(">>="), vec![(GT, ">"), (GT, ">"), (EQ, "=")]);
    }

    #[test]
    fn any_bytes_lex_losslessly() {
        let inputs: [&[u8]; 5] = [b"\xff\xfe\"\\u", b"'\\", b"\"\"\"\\{", b"/*", b"\\uuuu00"];
        for input in inputs {
            let lexed = tokenize(input, LanguageLevel::HIGHEST);
            assert_eq!(*lexed.starts.last().unwrap() as usize, input.len());
            assert!(lexed.starts.windows(2).all(|w| w[0] < w[1]));
        }
    }
}
