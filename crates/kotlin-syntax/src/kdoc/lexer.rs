//! A port of `KDoc.flex` (generated as `_KDocLexer.java`) and of `KDocLexer`, the `MergingLexerAdapterBase` over it
//! that merges runs of text, code and whitespace tokens. It lexes the text of one doc comment (`/** ... */`).
//!
//! Every state of KDoc.flex is inclusive (`%state`), so the unprefixed rules (`"*"+ "/"` and the `[\s\S]` fallback)
//! compete in all of them. Each state lists its rules in spec order; the longest match wins, the earlier rule on a tie,
//! as in JFlex. Trailing context (`r1 / r2`) counts toward the match length and is not consumed.

use crate::lexer::{Token, char_at};
use crate::tokens::TokenKind;
use crate::tokens::TokenKind::*;
use crate::unicode::KDOC_JLETTER;
use psi_builder::lexer::in_ranges;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum State {
    Initial,
    LineBeginning,
    ContentsBeginning,
    TagBeginning,
    TagTextBeginning,
    Contents,
    CodeBlock,
    CodeBlockLineBeginning,
    CodeBlockContentsBeginning,
    IndentedCodeBlock,
    CodeSpanContents,
    CodeSpanLineBeginning,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum BlockType {
    Paragraph,
    Code,
}

/// The rules of KDoc.flex, in spec order.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Rule {
    /// `<YYINITIAL> "/**"`
    Start,
    /// `"*"+ "/"`
    End,
    /// `<LINE_BEGINNING> {WHITE_SPACE_CHAR}+`
    LineBeginningWhiteSpace,
    /// `<LINE_BEGINNING> "*"+`
    LineBeginningAsterisks,
    /// `<CONTENTS_BEGINNING> "@"{PLAIN_IDENTIFIER}`
    TagName,
    /// `<TAG_BEGINNING> {LINE_BREAK_CHAR}`
    TagBeginningLineBreak,
    /// `<TAG_BEGINNING> {WHITE_SPACE_CHAR}+`
    TagBeginningWhiteSpace,
    /// `<TAG_BEGINNING> {CODE_LINK}`
    TagBeginningCodeLink,
    /// `<TAG_BEGINNING> {QUALIFIED_NAME}`
    TagBeginningQualifiedName,
    /// `<TAG_BEGINNING> [^]`
    TagBeginningAny,
    /// `<TAG_TEXT_BEGINNING> {LINE_BREAK_CHAR}`
    TagTextLineBreak,
    /// `<TAG_TEXT_BEGINNING> {WHITE_SPACE_CHAR}+`
    TagTextWhiteSpace,
    /// `<TAG_TEXT_BEGINNING> {CODE_LINK}`
    TagTextCodeLink,
    /// `<TAG_TEXT_BEGINNING> {BACKTICK_STRING}`
    TagTextBackticks,
    /// `<TAG_TEXT_BEGINNING> [^]`
    TagTextAny,
    /// `<CONTENTS_BEGINNING> {CODE_FENCE_START} / {LINE_BREAK_CHAR}`
    CodeFenceStart,
    /// `<CONTENTS_BEGINNING, CONTENTS> {BACKTICK_STRING}`
    ContentsBackticks,
    /// `<CODE_SPAN_CONTENTS> {LINE_BREAK_CHAR}`
    CodeSpanLineBreak,
    /// `<CODE_SPAN_CONTENTS> {BACKTICK_STRING}`
    CodeSpanBackticks,
    /// `<CODE_SPAN_CONTENTS> [^]`
    CodeSpanAny,
    /// `<CODE_SPAN_LINE_BEGINNING> {WHITE_SPACE_CHAR}+`
    CodeSpanLineWhiteSpace,
    /// `<CODE_SPAN_LINE_BEGINNING> "*"+`
    CodeSpanLineAsterisks,
    /// `<LINE_BEGINNING, CONTENTS_BEGINNING, CONTENTS> {LINE_BREAK_CHAR}`
    ContentsLineBreak,
    /// `<LINE_BEGINNING, CONTENTS_BEGINNING, CONTENTS> {WHITE_SPACE_CHAR}+`
    ContentsWhiteSpace,
    /// `<LINE_BEGINNING, CONTENTS_BEGINNING, CONTENTS> {ESCAPED_CHARS}`
    EscapedChar,
    /// `<LINE_BEGINNING, CONTENTS_BEGINNING, CONTENTS> "("`
    LPar,
    /// `<LINE_BEGINNING, CONTENTS_BEGINNING, CONTENTS> ")"`
    RPar,
    /// `<LINE_BEGINNING, CONTENTS_BEGINNING, CONTENTS> {CODE_LINK} / [^\(\[]`
    ContentsCodeLink,
    /// `<LINE_BEGINNING, CONTENTS_BEGINNING, CONTENTS> [^]`
    ContentsAny,
    /// `<CODE_BLOCK_LINE_BEGINNING> {WHITE_SPACE_CHAR}+`
    CodeBlockLineWhiteSpace,
    /// `<CODE_BLOCK_LINE_BEGINNING> "*"+`
    CodeBlockLineAsterisks,
    /// `<CODE_BLOCK_LINE_BEGINNING, CODE_BLOCK_CONTENTS_BEGINNING> {CODE_FENCE_END} / [ \t\f]* [\n]`
    CodeFenceEnd,
    /// `<INDENTED_CODE_BLOCK, CODE_BLOCK_LINE_BEGINNING, CODE_BLOCK_CONTENTS_BEGINNING, CODE_BLOCK> {LINE_BREAK_CHAR}`
    CodeBlockLineBreak,
    /// `<INDENTED_CODE_BLOCK, ...> {WHITE_SPACE_CHAR}+`
    CodeBlockWhiteSpace,
    /// `<INDENTED_CODE_BLOCK, ...> [^]`
    CodeBlockAny,
    /// `[\s\S]`
    BadCharacter,
}

use Rule::*;

/// The rules active in a state, in spec order.
fn rules(state: State) -> &'static [Rule] {
    match state {
        State::Initial => &[Start, End, BadCharacter],
        State::LineBeginning => &[
            End,
            LineBeginningWhiteSpace,
            LineBeginningAsterisks,
            ContentsLineBreak,
            ContentsWhiteSpace,
            EscapedChar,
            LPar,
            RPar,
            ContentsCodeLink,
            ContentsAny,
            BadCharacter,
        ],
        State::ContentsBeginning => &[
            End,
            TagName,
            CodeFenceStart,
            ContentsBackticks,
            ContentsLineBreak,
            ContentsWhiteSpace,
            EscapedChar,
            LPar,
            RPar,
            ContentsCodeLink,
            ContentsAny,
            BadCharacter,
        ],
        State::TagBeginning => &[
            End,
            TagBeginningLineBreak,
            TagBeginningWhiteSpace,
            TagBeginningCodeLink,
            TagBeginningQualifiedName,
            TagBeginningAny,
            BadCharacter,
        ],
        State::TagTextBeginning => {
            &[End, TagTextLineBreak, TagTextWhiteSpace, TagTextCodeLink, TagTextBackticks, TagTextAny, BadCharacter]
        }
        State::Contents => &[
            End,
            ContentsBackticks,
            ContentsLineBreak,
            ContentsWhiteSpace,
            EscapedChar,
            LPar,
            RPar,
            ContentsCodeLink,
            ContentsAny,
            BadCharacter,
        ],
        State::CodeSpanContents => &[End, CodeSpanLineBreak, CodeSpanBackticks, CodeSpanAny, BadCharacter],
        State::CodeSpanLineBeginning => &[End, CodeSpanLineWhiteSpace, CodeSpanLineAsterisks, BadCharacter],
        State::CodeBlockLineBeginning => &[
            End,
            CodeBlockLineWhiteSpace,
            CodeBlockLineAsterisks,
            CodeFenceEnd,
            CodeBlockLineBreak,
            CodeBlockWhiteSpace,
            CodeBlockAny,
            BadCharacter,
        ],
        State::CodeBlockContentsBeginning => {
            &[End, CodeFenceEnd, CodeBlockLineBreak, CodeBlockWhiteSpace, CodeBlockAny, BadCharacter]
        }
        State::IndentedCodeBlock | State::CodeBlock => {
            &[End, CodeBlockLineBreak, CodeBlockWhiteSpace, CodeBlockAny, BadCharacter]
        }
    }
}

/// `KDocKnownTag.findByTagName`: `(isReferenceRequired, isSectionStart)` of a known tag name (with or without `@`).
pub(crate) fn find_known_tag(tag_name: &[u8]) -> Option<(bool, bool)> {
    let name = tag_name.strip_prefix(b"@").unwrap_or(tag_name);
    let upper: Vec<u8> = name.iter().map(u8::to_ascii_uppercase).collect();
    Some(match upper.as_slice() {
        b"AUTHOR" => (false, false),
        b"THROWS" => (true, false),
        b"EXCEPTION" => (true, false),
        b"PARAM" => (true, false),
        b"RECEIVER" => (false, false),
        b"RETURN" => (false, false),
        b"SEE" => (true, false),
        b"SINCE" => (false, false),
        b"CONSTRUCTOR" => (false, true),
        b"PROPERTY" => (true, true),
        b"SAMPLE" => (true, false),
        b"SUPPRESS" => (false, false),
        _ => return None,
    })
}

/// `Character.isWhitespace` of a code point (on a BMP char, as the Java code calls it on `char`s).
fn is_java_whitespace(cp: u32) -> bool {
    matches!(cp, 0x09..=0x0D | 0x1C..=0x20 | 0x1680 | 0x2000..=0x2006 | 0x2008..=0x200A | 0x2028 | 0x2029 | 0x205F | 0x3000)
}

/// `LETTER = [:jletter:]`.
fn is_jletter(cp: u32) -> bool {
    if cp < 0x80 {
        (cp as u8).is_ascii_alphabetic() || cp == b'$' as u32 || cp == b'_' as u32
    } else {
        in_ranges(&KDOC_JLETTER, cp)
    }
}

/// The raw `_KDocLexer`.
struct FlexKDocLexer<'a> {
    text: &'a [u8],
    pos: usize,
    state: State,
    consecutive_line_break_count: i32,
    code_fence_char: u8,
    code_fence_length: i32,
    last_block_type: Option<BlockType>,
    /// `FlexAdapter.myFailed`: the lexer threw; it returned the rest as one `BAD_CHARACTER` and stops.
    failed: bool,
}

impl<'a> FlexKDocLexer<'a> {
    fn new(text: &'a [u8]) -> Self {
        FlexKDocLexer {
            text,
            pos: 0,
            state: State::Initial,
            consecutive_line_break_count: 0,
            code_fence_char: 0,
            code_fence_length: -1,
            last_block_type: None,
            failed: false,
        }
    }

    fn at(&self, i: usize) -> Option<u8> {
        self.text.get(i).copied()
    }

    fn yybegin_and_update(&mut self, new_state: State) {
        self.consecutive_line_break_count = if matches!(new_state, State::LineBeginning | State::CodeBlockLineBeginning)
        {
            self.consecutive_line_break_count + 1
        } else {
            0
        };
        self.state = new_state;
    }

    fn run_of(&self, i: usize, pred: impl Fn(u8) -> bool) -> usize {
        let mut j = i;
        while self.at(j).is_some_and(&pred) {
            j += 1;
        }
        j - i
    }

    /// `{PLAIN_IDENTIFIER} = {LETTER} ({LETTER} | {DIGIT})*`: its length at `i`.
    fn plain_identifier(&self, i: usize) -> Option<usize> {
        if i >= self.text.len() {
            return None;
        }
        let (cp, len) = char_at(self.text, i);
        if !is_jletter(cp) {
            return None;
        }
        let mut j = i + len;
        while j < self.text.len() {
            let (cp, len) = char_at(self.text, j);
            if !(is_jletter(cp) || (b'0' as u32..=b'9' as u32).contains(&cp)) {
                break;
            }
            j += len;
        }
        Some(j - i)
    }

    /// `{IDENTIFIER} = {PLAIN_IDENTIFIER} | `[^`\n]+``.
    fn identifier(&self, i: usize) -> Option<usize> {
        if self.at(i) == Some(b'`') {
            let mut j = i + 1;
            while let Some(b) = self.at(j) {
                if b == b'`' {
                    return (j > i + 1).then_some(j + 1 - i);
                }
                if b == b'\n' {
                    return None;
                }
                j += 1;
            }
            return None;
        }
        self.plain_identifier(i)
    }

    /// `{QUALIFIED_NAME} = {IDENTIFIER} ([\.] {IDENTIFIER}?)*`.
    fn qualified_name(&self, i: usize) -> Option<usize> {
        let mut j = i + self.identifier(i)?;
        while self.at(j) == Some(b'.') {
            j += 1;
            if let Some(len) = self.identifier(j) {
                j += len;
            }
        }
        Some(j - i)
    }

    /// `{CODE_LINK} = \[{QUALIFIED_NAME}\]`.
    fn code_link(&self, i: usize) -> Option<usize> {
        if self.at(i) != Some(b'[') {
            return None;
        }
        let name = self.qualified_name(i + 1)?;
        (self.at(i + 1 + name) == Some(b']')).then_some(name + 2)
    }

    /// `{CODE_FENCE_START} / {LINE_BREAK_CHAR}`: the fence length (without the context).
    fn code_fence_start(&self, i: usize) -> Option<usize> {
        let c = self.at(i)?;
        let fence = match c {
            b'`' => self.run_of(i, |b| b == b'`'),
            b'~' => self.run_of(i, |b| b == b'~'),
            _ => return None,
        };
        if fence < 3 {
            return None;
        }
        let rest = if c == b'`' {
            self.run_of(i + fence, |b| b != b'`' && b != b'\r' && b != b'\n')
        } else {
            self.run_of(i + fence, |b| b != b'\r' && b != b'\n')
        };
        matches!(self.at(i + fence + rest), Some(b'\r' | b'\n')).then_some(fence + rest)
    }

    /// `{CODE_FENCE_END} / [ \t\f]* [\n]`: `(token length, context length)`.
    fn code_fence_end(&self, i: usize) -> Option<(usize, usize)> {
        let c = self.at(i)?;
        if c != b'`' && c != b'~' {
            return None;
        }
        let fence = self.run_of(i, |b| b == c);
        let spaces = self.run_of(i + fence, |b| matches!(b, b' ' | b'\t' | 0x0C));
        (self.at(i + fence + spaces) == Some(b'\n')).then_some((fence, spaces + 1))
    }

    /// What `rule` matches at `i`: `(length for the longest-match comparison, token length)`.
    fn try_rule(&self, rule: Rule, i: usize) -> Option<(usize, usize)> {
        let one = |len: usize| Some((len, len));
        let c = self.at(i)?;
        match rule {
            Start => (self.text[i..].starts_with(b"/**")).then_some((3, 3)),
            End => {
                let stars = self.run_of(i, |b| b == b'*');
                (stars > 0 && self.at(i + stars) == Some(b'/')).then_some((stars + 1, stars + 1))
            }
            LineBeginningWhiteSpace
            | TagBeginningWhiteSpace
            | TagTextWhiteSpace
            | CodeSpanLineWhiteSpace
            | ContentsWhiteSpace
            | CodeBlockLineWhiteSpace
            | CodeBlockWhiteSpace => {
                let n = self.run_of(i, |b| matches!(b, b' ' | b'\t' | 0x0C));
                if n > 0 { one(n) } else { None }
            }
            LineBeginningAsterisks | CodeSpanLineAsterisks | CodeBlockLineAsterisks => {
                let n = self.run_of(i, |b| b == b'*');
                if n > 0 { one(n) } else { None }
            }
            TagName => {
                if c != b'@' {
                    return None;
                }
                self.plain_identifier(i + 1).map(|n| (n + 1, n + 1))
            }
            TagBeginningLineBreak | TagTextLineBreak | CodeSpanLineBreak | ContentsLineBreak | CodeBlockLineBreak => {
                if c == b'\r' || c == b'\n' {
                    one(1)
                } else {
                    None
                }
            }
            TagBeginningCodeLink | TagTextCodeLink => self.code_link(i).map(|n| (n, n)),
            TagBeginningQualifiedName => self.qualified_name(i).map(|n| (n, n)),
            TagBeginningAny | TagTextAny | CodeSpanAny | ContentsAny | CodeBlockAny | BadCharacter => {
                one(char_at(self.text, i).1)
            }
            TagTextBackticks | ContentsBackticks | CodeSpanBackticks => {
                let n = self.run_of(i, |b| b == b'`');
                if n > 0 { one(n) } else { None }
            }
            CodeFenceStart => self.code_fence_start(i).map(|n| (n + 1, n)),
            EscapedChar => {
                let escapable = |b: u8| b"!#$%&'()*+,-./:;<=>?@_`{|}~\"^[\\]".contains(&b);
                (c == b'\\' && self.at(i + 1).is_some_and(escapable)).then_some((2, 2))
            }
            LPar => (c == b'(').then_some((1, 1)),
            RPar => (c == b')').then_some((1, 1)),
            ContentsCodeLink => {
                let n = self.code_link(i)?;
                let next = self.at(i + n)?;
                (next != b'(' && next != b'[').then(|| (n + char_at(self.text, i + n).1, n))
            }
            CodeFenceEnd => self.code_fence_end(i).map(|(n, context)| (n + context, n)),
        }
    }

    /// `countRepeating(c)`: the run of `c` from the token start, within the token.
    fn count_repeating(&self, start: usize, end: usize, c: u8) -> i32 {
        let mut current = start;
        while current < end && self.text[current] == c {
            current += 1;
        }
        (current - start) as i32
    }

    /// Whether `countRepeating(c)` throws: it reads `zzBuffer.charAt(current)` before checking `current < zzMarkedPos`,
    /// so a token of `c`s that ends the buffer reads past it (`StringIndexOutOfBoundsException`).
    fn count_repeating_throws(&self, start: usize, end: usize, c: u8) -> bool {
        end == self.text.len() && self.count_repeating(start, end, c) as usize == end - start
    }

    /// `hasMatchingCloseFence(c, length)`: whether a run of exactly `length` `c`s follows before a paragraph break or
    /// the comment end.
    fn has_matching_close_fence(&self, from: usize, c: u8, length: i32) -> bool {
        #[derive(PartialEq)]
        enum LinePos {
            AfterNewline,
            AfterLeadingAsterisk,
            InContent,
        }
        let text = self.text;
        let end = text.len();
        let mut pos = from;
        let mut line_pos = LinePos::InContent;
        while pos < end {
            let ch = text[pos];
            if ch == b'\n' {
                if line_pos != LinePos::InContent {
                    return false;
                }
                line_pos = LinePos::AfterNewline;
                pos += 1;
            } else if is_java_whitespace(char_at(text, pos).0) {
                pos += char_at(text, pos).1;
            } else if line_pos == LinePos::AfterNewline && ch == b'*' {
                loop {
                    pos += 1;
                    if !(pos < end && text[pos] == b'*') {
                        break;
                    }
                }
                if pos < end && text[pos] == b'/' {
                    return false;
                }
                line_pos = LinePos::AfterLeadingAsterisk;
            } else if ch == c {
                let fence_start = pos;
                loop {
                    pos += 1;
                    if !(pos < end && text[pos] == ch) {
                        break;
                    }
                }
                if (pos - fence_start) as i32 == length {
                    return true;
                }
                line_pos = LinePos::InContent;
            } else {
                line_pos = LinePos::InContent;
                pos += char_at(text, pos).1;
            }
        }
        false
    }

    /// The next raw token.
    fn advance(&mut self) -> Option<Token> {
        let start = self.pos;
        if start >= self.text.len() || self.failed {
            return None;
        }
        let mut best: Option<(Rule, usize, usize)> = None;
        for &rule in rules(self.state) {
            if let Some((match_len, token_len)) = self.try_rule(rule, start)
                && best.is_none_or(|(_, best_len, _)| match_len > best_len)
            {
                best = Some((rule, match_len, token_len));
            }
        }
        let (rule, token_len) = match best {
            Some((rule, _, token_len)) => (rule, token_len),
            None => (BadCharacter, 1),
        };
        let end = start + token_len;
        self.pos = end;
        let calls_count_repeating =
            matches!(rule, TagTextBackticks | ContentsBackticks | CodeSpanBackticks | CodeFenceStart | CodeFenceEnd);
        if calls_count_repeating && self.count_repeating_throws(start, end, self.text[start]) {
            // FlexAdapter.locateToken catches it: the rest of the buffer is one BAD_CHARACTER, then nothing.
            self.failed = true;
            self.pos = self.text.len();
            return Some(Token { kind: BAD_CHARACTER, start, end: self.text.len() });
        }
        let kind = self.action(rule, start, end);
        Some(Token { kind, start, end })
    }

    /// The action of `rule` that matched `start..end`.
    fn action(&mut self, rule: Rule, start: usize, end: usize) -> TokenKind {
        let text = self.text;
        match rule {
            Start => {
                self.yybegin_and_update(State::ContentsBeginning);
                KDOC_START
            }
            End => {
                if end == text.len() {
                    KDOC_END
                } else {
                    KDOC_TEXT
                }
            }
            LineBeginningWhiteSpace
            | TagBeginningWhiteSpace
            | TagTextWhiteSpace
            | CodeSpanLineWhiteSpace
            | CodeBlockLineWhiteSpace => WHITE_SPACE,
            LineBeginningAsterisks => {
                self.state = State::ContentsBeginning;
                KDOC_LEADING_ASTERISK
            }
            TagName => {
                self.last_block_type = Some(BlockType::Paragraph);
                let reference_required = find_known_tag(&text[start..end]).is_some_and(|(required, _)| required);
                self.yybegin_and_update(if reference_required { State::TagBeginning } else { State::TagTextBeginning });
                KDOC_TAG_NAME
            }
            TagBeginningLineBreak | TagTextLineBreak | ContentsLineBreak => {
                self.yybegin_and_update(State::LineBeginning);
                WHITE_SPACE
            }
            TagBeginningCodeLink | TagBeginningQualifiedName => {
                self.yybegin_and_update(State::TagTextBeginning);
                KDOC_MARKDOWN_LINK
            }
            TagBeginningAny | TagTextAny => {
                self.yybegin_and_update(State::Contents);
                KDOC_TEXT
            }
            TagTextCodeLink => {
                self.yybegin_and_update(State::Contents);
                KDOC_MARKDOWN_LINK
            }
            TagTextBackticks | ContentsBackticks => {
                self.code_fence_char = text[start];
                self.code_fence_length = self.count_repeating(start, end, self.code_fence_char);
                if self.has_matching_close_fence(end, self.code_fence_char, self.code_fence_length) {
                    self.yybegin_and_update(State::CodeSpanContents);
                }
                KDOC_TEXT
            }
            CodeFenceStart => {
                self.last_block_type = Some(BlockType::Code);
                self.code_fence_char = text[start];
                self.code_fence_length = self.count_repeating(start, end, self.code_fence_char);
                self.yybegin_and_update(State::CodeBlockLineBeginning);
                KDOC_TEXT
            }
            CodeSpanLineBreak => {
                self.yybegin_and_update(State::CodeSpanLineBeginning);
                WHITE_SPACE
            }
            CodeSpanBackticks => {
                let ch = text[start];
                let length = self.count_repeating(start, end, ch);
                if length == self.code_fence_length && ch == self.code_fence_char {
                    self.code_fence_length = -1;
                    self.code_fence_char = 0;
                    self.yybegin_and_update(State::Contents);
                    KDOC_TEXT
                } else {
                    KDOC_CODE_SPAN_TEXT
                }
            }
            CodeSpanAny => KDOC_CODE_SPAN_TEXT,
            CodeSpanLineAsterisks => {
                self.yybegin_and_update(State::CodeSpanContents);
                KDOC_LEADING_ASTERISK
            }
            ContentsWhiteSpace => {
                let state = self.state;
                let len = end - start;
                if state == State::ContentsBeginning
                    && (len >= 4 || text[start] == b'\t' || text[end - 1] == b'\t')
                    && (self.last_block_type != Some(BlockType::Paragraph) || self.consecutive_line_break_count >= 2)
                {
                    self.state = State::IndentedCodeBlock;
                    self.last_block_type = Some(BlockType::Code);
                    return KDOC_CODE_BLOCK_TEXT;
                }
                self.state = if state == State::ContentsBeginning { State::ContentsBeginning } else { State::Contents };
                KDOC_TEXT
            }
            EscapedChar => {
                self.last_block_type = Some(BlockType::Paragraph);
                self.yybegin_and_update(State::Contents);
                KDOC_MARKDOWN_ESCAPED_CHAR
            }
            LPar => {
                self.last_block_type = Some(BlockType::Paragraph);
                self.yybegin_and_update(State::Contents);
                KDOC_LPAR
            }
            RPar => {
                self.last_block_type = Some(BlockType::Paragraph);
                self.yybegin_and_update(State::Contents);
                KDOC_RPAR
            }
            ContentsCodeLink => {
                self.last_block_type = Some(BlockType::Paragraph);
                self.yybegin_and_update(State::Contents);
                KDOC_MARKDOWN_LINK
            }
            ContentsAny => {
                self.last_block_type = Some(BlockType::Paragraph);
                self.yybegin_and_update(State::Contents);
                KDOC_TEXT
            }
            CodeBlockLineAsterisks => {
                self.state = State::CodeBlockContentsBeginning;
                KDOC_LEADING_ASTERISK
            }
            CodeFenceEnd => {
                let ch = text[start];
                let length = self.count_repeating(start, end, ch);
                if length == self.code_fence_length && ch == self.code_fence_char {
                    self.code_fence_char = 0;
                    self.code_fence_length = -1;
                    self.yybegin_and_update(State::Contents);
                    KDOC_TEXT
                } else {
                    KDOC_CODE_BLOCK_TEXT
                }
            }
            CodeBlockLineBreak => {
                self.yybegin_and_update(if self.state == State::IndentedCodeBlock {
                    State::LineBeginning
                } else {
                    State::CodeBlockLineBeginning
                });
                WHITE_SPACE
            }
            CodeBlockWhiteSpace => KDOC_CODE_BLOCK_TEXT,
            CodeBlockAny => {
                self.yybegin_and_update(if self.state == State::IndentedCodeBlock {
                    State::IndentedCodeBlock
                } else {
                    State::CodeBlock
                });
                KDOC_CODE_BLOCK_TEXT
            }
            BadCharacter => {
                self.last_block_type = Some(BlockType::Paragraph);
                BAD_CHARACTER
            }
        }
    }
}

/// `KDocLexer`: the merged tokens of a doc comment's text.
pub struct KDocLexer<'a> {
    original: FlexKDocLexer<'a>,
    /// The original lexer's current token (`getOriginal().getTokenType()` after the last advance).
    current: Option<Token>,
}

impl<'a> KDocLexer<'a> {
    pub fn new(text: &'a [u8]) -> Self {
        let mut original = FlexKDocLexer::new(text);
        let current = original.advance();
        KDocLexer { original, current }
    }

    fn advance_original(&mut self) {
        self.current = self.original.advance();
    }

    /// The next merged token (`MergingLexerAdapterBase` with `KDocLexerMergeFunction`).
    pub fn advance(&mut self) -> Option<Token> {
        let first = self.current?;
        self.advance_original();
        let mut kind = first.kind;
        let next = self.current;
        let text = self.original.text;
        if kind == KDOC_CODE_BLOCK_TEXT
            && let Some(next) = next
            && next.kind == KDOC_TEXT
            && is_valid_code_fence(&text[next.start..next.end])
        {
            // Don't treat the trailing line as a part of a code block
            self.advance_original();
            kind = KDOC_TEXT;
        } else if matches!(kind, KDOC_CODE_BLOCK_TEXT | KDOC_CODE_SPAN_TEXT | KDOC_TEXT | WHITE_SPACE) {
            while self.current.is_some_and(|t| t.kind == kind) {
                self.advance_original();
            }
        }
        let end = self.current.map_or(text.len(), |t| t.start);
        Some(Token { kind, start: first.start, end })
    }
}

/// `KDocLexer.isValidCodeFence`.
fn is_valid_code_fence(s: &[u8]) -> bool {
    if s.len() < 3 {
        return false;
    }
    let first = s[0];
    if first != b'`' && first != b'~' {
        return false;
    }
    s[1..].iter().all(|&b| b == first)
}

/// The merged KDoc tokens of `text` (kinds and start offsets plus the end), as the PsiBuilder of a doc comment sees them.
pub fn tokenize(text: &[u8]) -> (Vec<TokenKind>, Vec<u32>) {
    let (mut types, mut starts) = (Vec::new(), Vec::new());
    tokenize_into(text, &mut types, &mut starts);
    (types, starts)
}

/// `tokenize` into `types` and `starts`, which are cleared first (their buffers are reused).
pub fn tokenize_into(text: &[u8], types: &mut Vec<TokenKind>, starts: &mut Vec<u32>) {
    types.clear();
    starts.clear();
    let mut lexer = KDocLexer::new(text);
    while let Some(token) = lexer.advance() {
        types.push(token.kind);
        starts.push(token.start as u32);
    }
    starts.push(text.len() as u32);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(text: &str) -> Vec<(TokenKind, &str)> {
        let mut lexer = KDocLexer::new(text.as_bytes());
        let mut out = Vec::new();
        while let Some(t) = lexer.advance() {
            out.push((t.kind, &text[t.start..t.end]));
        }
        out
    }

    #[test]
    fn markdown_and_tags() {
        assert_eq!(
            lex("/**\n * [WikiLink] \\[x\n * @param a b\n */"),
            [
                (KDOC_START, "/**"),
                (WHITE_SPACE, "\n "),
                (KDOC_LEADING_ASTERISK, "*"),
                (KDOC_TEXT, " "),
                (KDOC_MARKDOWN_LINK, "[WikiLink]"),
                (KDOC_TEXT, " "),
                (KDOC_MARKDOWN_ESCAPED_CHAR, "\\["),
                (KDOC_TEXT, "x"),
                (WHITE_SPACE, "\n "),
                (KDOC_LEADING_ASTERISK, "*"),
                (KDOC_TEXT, " "),
                (KDOC_TAG_NAME, "@param"),
                (WHITE_SPACE, " "),
                (KDOC_MARKDOWN_LINK, "a"),
                (WHITE_SPACE, " "),
                (KDOC_TEXT, "b"),
                (WHITE_SPACE, "\n "),
                (KDOC_END, "*/"),
            ]
        );
        assert_eq!(
            lex("/** `code` */"),
            [
                (KDOC_START, "/**"),
                (KDOC_TEXT, " `"),
                (KDOC_CODE_SPAN_TEXT, "code"),
                (KDOC_TEXT, "` "),
                (KDOC_END, "*/")
            ]
        );
    }
}
