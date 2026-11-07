//! A hand-written port of `Starlark.flex` and `StarlarkIndentingLexer.kt` from the IntelliJ Bazel plugin.

use crate::token_sets::{CLOSE_BRACKETS, OPEN_BRACKETS, WHITESPACE};
use crate::tokens::TokenKind as T;
use psi_builder::lexer::{Lexed, char_at};

pub type StarlarkLexed = Lexed<T>;

/// A token and its byte range in the source.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Token {
    pub kind: T,
    pub start: usize,
    pub end: usize,
    plain: bool,
}

/// The Starlark lexer with IntelliJ's indentation and statement-break tokens.
pub fn tokenize(text: &[u8]) -> StarlarkLexed {
    let mut lexed = Lexed::default();
    tokenize_into(text, &mut lexed);
    lexed
}

/// Reuses `lexed` for `text`, as the syntax lexers do.
pub fn tokenize_into(text: &[u8], lexed: &mut StarlarkLexed) {
    let tokens = lex(text);
    lexed.begin(text.len());
    for token in tokens {
        lexed.push(token.kind, token.start as u32, token.plain);
    }
    lexed.end(text.len() as u32);
}

/// Lexes a Starlark source into source-backed tokens and zero-width indentation tokens.
pub fn lex(text: &[u8]) -> Vec<Token> {
    IndentingLexer::new(text).run()
}

fn base_lex(text: &[u8]) -> Vec<Token> {
    let mut tokens = Vec::with_capacity(text.len() / 3 + 1);
    let mut pos = 0;
    while pos < text.len() {
        let start = pos;
        let byte = text[pos];
        let (kind, end) = match byte {
            b' ' => (T::SPACE, pos + 1),
            b'\t' => (T::TAB, pos + 1),
            b'\n' => (T::LINE_BREAK, pos + 1),
            b'\\' if text.get(pos + 1) == Some(&b'\n') => (T::LINE_CONTINUATION, pos + 2),
            b'#' => {
                pos += 1;
                while text.get(pos).is_some_and(|b| !matches!(b, b'\r' | b'\n')) {
                    pos += char_at(text, pos).1;
                }
                (T::COMMENT, pos)
            }
            b'\'' | b'"' | b'r' | b'b' => {
                if let Some((kind, end)) = string_literal_end(text, pos) {
                    (kind, end)
                } else if let Some((kind, end)) = number_end(text, pos) {
                    (kind, end)
                } else if is_identifier_start(text, pos) {
                    identifier(text, pos)
                } else {
                    (T::BAD_CHARACTER, pos + 1)
                }
            }
            b'0'..=b'9' => number_end(text, pos).unwrap_or((T::BAD_CHARACTER, pos + 1)),
            b'.' if text.get(pos + 1).is_some_and(u8::is_ascii_digit) => {
                number_end(text, pos).unwrap_or((T::DOT, pos + 1))
            }
            _ if is_identifier_start(text, pos) => identifier(text, pos),
            _ => operator_or_bad_character(text, pos),
        };
        pos = end.max(start + 1);
        tokens.push(Token { kind, start, end: pos, plain: plain_token(kind) });
    }
    tokens
}

fn identifier(text: &[u8], start: usize) -> (T, usize) {
    let mut end = start;
    while end < text.len() && is_identifier_part(text, end) {
        end += char_at(text, end).1;
    }
    let word = std::str::from_utf8(&text[start..end]).unwrap_or("");
    (keyword(word).unwrap_or(T::IDENTIFIER), end)
}

fn is_identifier_start(text: &[u8], pos: usize) -> bool {
    let (ch, _) = char_at(text, pos);
    ch == b'_' as u32 || char::from_u32(ch).is_some_and(char::is_alphabetic)
}

fn is_identifier_part(text: &[u8], pos: usize) -> bool {
    let (ch, _) = char_at(text, pos);
    ch == b'_' as u32 || char::from_u32(ch).is_some_and(char::is_alphanumeric)
}

fn plain_token(kind: T) -> bool {
    !matches!(
        kind,
        T::SPACE
            | T::TAB
            | T::LINE_CONTINUATION
            | T::LINE_BREAK
            | T::INDENT
            | T::DEDENT
            | T::INCONSISTENT_DEDENT
            | T::STATEMENT_BREAK
            | T::COMMENT
    )
}

fn keyword(word: &str) -> Option<T> {
    Some(match word {
        "and" => T::AND_KEYWORD,
        "break" => T::BREAK_KEYWORD,
        "continue" => T::CONTINUE_KEYWORD,
        "def" => T::DEF_KEYWORD,
        "elif" => T::ELIF_KEYWORD,
        "else" => T::ELSE_KEYWORD,
        "for" => T::FOR_KEYWORD,
        "if" => T::IF_KEYWORD,
        "in" => T::IN_KEYWORD,
        "lambda" => T::LAMBDA_KEYWORD,
        "load" => T::LOAD_KEYWORD,
        "not" => T::NOT_KEYWORD,
        "or" => T::OR_KEYWORD,
        "pass" => T::PASS_KEYWORD,
        "return" => T::RETURN_KEYWORD,
        "None" => T::NONE_KEYWORD,
        "True" => T::TRUE_KEYWORD,
        "False" => T::FALSE_KEYWORD,
        "as" => T::AS_KEYWORD,
        "assert" => T::ASSERT_KEYWORD,
        "async" => T::ASYNC_KEYWORD,
        "await" => T::AWAIT_KEYWORD,
        "class" => T::CLASS_KEYWORD,
        "del" => T::DEL_KEYWORD,
        "except" => T::EXCEPT_KEYWORD,
        "finally" => T::FINALLY_KEYWORD,
        "from" => T::FROM_KEYWORD,
        "global" => T::GLOBAL_KEYWORD,
        "import" => T::IMPORT_KEYWORD,
        "is" => T::IS_KEYWORD,
        "nonlocal" => T::NONLOCAL_KEYWORD,
        "raise" => T::RAISE_KEYWORD,
        "try" => T::TRY_KEYWORD,
        "while" => T::WHILE_KEYWORD,
        "with" => T::WITH_KEYWORD,
        "yield" => T::YIELD_KEYWORD,
        _ => return None,
    })
}

fn number_end(text: &[u8], start: usize) -> Option<(T, usize)> {
    let float_end = float_end(text, start);
    let integer_end = integer_end(text, start);
    match (float_end, integer_end) {
        (Some(float), Some(integer)) if float >= integer => Some((T::FLOAT, float)),
        (Some(float), None) => Some((T::FLOAT, float)),
        (_, Some(integer)) => Some((T::INT, integer)),
        _ => None,
    }
}

fn float_end(text: &[u8], start: usize) -> Option<usize> {
    let mut end = start;
    if text.get(end).is_some_and(u8::is_ascii_digit) {
        while text.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        let digits_end = end;
        if text.get(end) == Some(&b'.') {
            end += 1;
            while text.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
            if let Some(exponent) = exponent_end(text, end) {
                end = exponent;
            }
            return Some(end);
        }
        return exponent_end(text, digits_end);
    }
    if text.get(end) == Some(&b'.') && text.get(end + 1).is_some_and(u8::is_ascii_digit) {
        end += 1;
        while text.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        return Some(exponent_end(text, end).unwrap_or(end));
    }
    None
}

fn exponent_end(text: &[u8], start: usize) -> Option<usize> {
    if !matches!(text.get(start), Some(b'e' | b'E')) {
        return None;
    }
    let mut end = start + 1;
    if matches!(text.get(end), Some(b'+' | b'-')) {
        end += 1;
    }
    let digits_start = end;
    while text.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    (end > digits_start).then_some(end)
}

fn integer_end(text: &[u8], start: usize) -> Option<usize> {
    match text.get(start) {
        Some(b'0') if matches!(text.get(start + 1), Some(b'x' | b'X')) => {
            let mut end = start + 2;
            while text.get(end).is_some_and(u8::is_ascii_hexdigit) {
                end += 1;
            }
            Some(if end > start + 2 { end } else { start + 1 })
        }
        Some(b'0') if matches!(text.get(start + 1), Some(b'o' | b'O')) => {
            let mut end = start + 2;
            while text.get(end).is_some_and(|b| matches!(b, b'0'..=b'7')) {
                end += 1;
            }
            Some(if end > start + 2 { end } else { start + 1 })
        }
        Some(b'0') => Some(start + 1),
        Some(b'1'..=b'9') => {
            let mut end = start + 1;
            while text.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
            Some(end)
        }
        _ => None,
    }
}

fn string_literal_end(text: &[u8], start: usize) -> Option<(T, usize)> {
    let (kind, quote_start) = if text.get(start..start + 2).is_some_and(|p| p == b"br" || p == b"rb")
        && matches!(text.get(start + 2), Some(b'\'' | b'"'))
    {
        (T::BYTES, start + 2)
    } else if text.get(start) == Some(&b'b') && matches!(text.get(start + 1), Some(b'\'' | b'"')) {
        (T::BYTES, start + 1)
    } else if text.get(start) == Some(&b'r') && matches!(text.get(start + 1), Some(b'\'' | b'"')) {
        (T::STRING, start + 1)
    } else if matches!(text.get(start), Some(b'\'' | b'"')) {
        (T::STRING, start)
    } else {
        return None;
    };
    let quote = text[quote_start];
    let triple = text.get(quote_start..quote_start + 3).is_some_and(|s| s == [quote; 3]);
    let opening = if triple { 3 } else { 1 };
    let mut end = quote_start + opening;
    while end < text.len() {
        if !triple && text[end] == b'\n' {
            return Some((kind, end));
        }
        if text[end] == b'\\' {
            if end + 1 == text.len() {
                return Some((kind, end));
            }
            end += 2;
        } else if triple && text.get(end..end + 3).is_some_and(|s| s == [quote; 3]) {
            return Some((kind, end + 3));
        } else if !triple && text[end] == quote {
            return Some((kind, end + 1));
        } else {
            end += char_at(text, end).1;
        }
    }
    Some((kind, end))
}

fn operator_or_bad_character(text: &[u8], start: usize) -> (T, usize) {
    const OPERATORS: &[(&[u8], T)] = &[
        (b"//=", T::FLOORDIVEQ),
        (b"<<=", T::LTLTEQ),
        (b">>=", T::GTGTEQ),
        (b"**", T::EXP),
        (b"//", T::FLOORDIV),
        (b"<<", T::LTLT),
        (b">>", T::GTGT),
        (b">=", T::GE),
        (b"<=", T::LE),
        (b"==", T::EQEQ),
        (b"!=", T::NE),
        (b"+=", T::PLUSEQ),
        (b"-=", T::MINUSEQ),
        (b"*=", T::MULTEQ),
        (b"/=", T::DIVEQ),
        (b"%=", T::PERCEQ),
        (b"&=", T::ANDEQ),
        (b"|=", T::OREQ),
        (b"^=", T::XOREQ),
        (b"+", T::PLUS),
        (b"-", T::MINUS),
        (b"*", T::MULT),
        (b"/", T::DIV),
        (b"%", T::PERC),
        (b"~", T::TILDE),
        (b"&", T::AND),
        (b"|", T::OR),
        (b"^", T::XOR),
        (b".", T::DOT),
        (b",", T::COMMA),
        (b"=", T::EQ),
        (b";", T::SEMICOLON),
        (b":", T::COLON),
        (b"(", T::LPAR),
        (b")", T::RPAR),
        (b"[", T::LBRACKET),
        (b"]", T::RBRACKET),
        (b"{", T::LBRACE),
        (b"}", T::RBRACE),
        (b"<", T::LT),
        (b">", T::GT),
    ];
    if let Some((text, kind)) = OPERATORS.iter().find(|(operator, _)| text[start..].starts_with(operator)) {
        (*kind, start + text.len())
    } else {
        (T::BAD_CHARACTER, start + char_at(text, start).1)
    }
}

#[derive(Copy, Clone)]
struct PendingToken {
    token: Token,
    comment_indent: Option<isize>,
}

struct IndentingLexer<'a> {
    text: &'a [u8],
    base: Vec<Token>,
    base_index: usize,
    indent_stack: Vec<usize>,
    brace_level: i32,
    line_has_significant_tokens: bool,
    last_new_line_indent: isize,
    queue: Vec<PendingToken>,
    line_break_before_first_comment_index: Option<usize>,
    process_special_tokens_pending: bool,
    output: Vec<Token>,
}

impl<'a> IndentingLexer<'a> {
    fn new(text: &'a [u8]) -> IndentingLexer<'a> {
        IndentingLexer {
            text,
            base: base_lex(text),
            base_index: 0,
            indent_stack: vec![0],
            brace_level: 0,
            line_has_significant_tokens: false,
            last_new_line_indent: -1,
            queue: Vec::new(),
            line_break_before_first_comment_index: None,
            process_special_tokens_pending: false,
            output: Vec::new(),
        }
    }

    fn run(mut self) -> Vec<Token> {
        self.set_start_state();
        while let Some(token) = self.current_token() {
            self.output.push(token);
            self.advance();
        }
        self.output
    }

    fn set_start_state(&mut self) {
        self.adjust_brace_level();
        self.check_significant_tokens();
        if self.is_base_at(T::SPACE) {
            self.process_indent(0, Some(T::SPACE));
        }
    }

    fn current_token(&self) -> Option<Token> {
        self.queue.first().map(|pending| pending.token).or_else(|| self.base.get(self.base_index).copied())
    }

    fn advance(&mut self) {
        if !self.queue.is_empty() {
            self.queue.remove(0);
            if self.process_special_tokens_pending {
                self.process_special_tokens_pending = false;
                self.process_special_tokens();
            }
        } else {
            self.advance_base();
            self.process_special_tokens();
        }
        self.adjust_brace_level();
    }

    fn advance_base(&mut self) {
        if self.base_index < self.base.len() {
            self.base_index += 1;
        }
        if let Some(token) = self.base.get_mut(self.base_index) {
            token.plain = self.brace_level == 0 && self.indent_stack.len() == 1 && plain_token(token.kind);
        }
        self.check_significant_tokens();
    }

    fn adjust_brace_level(&mut self) {
        if let Some(token) = self.current_token() {
            if OPEN_BRACKETS.contains(token.kind) {
                self.brace_level += 1;
            } else if CLOSE_BRACKETS.contains(token.kind) {
                self.brace_level -= 1;
            }
        }
    }

    fn check_significant_tokens(&mut self) {
        if let Some(token) = self.base.get(self.base_index)
            && !WHITESPACE.contains(token.kind)
            && token.kind != T::COMMENT
        {
            self.line_has_significant_tokens = true;
        }
    }

    fn process_special_tokens(&mut self) {
        if self.is_base_at(T::LINE_BREAK) {
            let start = self.base[self.base_index].start;
            self.process_line_break(start);
            if self.is_base_at(T::COMMENT) {
                self.process_comment();
            }
        } else if self.is_base_at(T::SPACE) {
            self.process_space();
        }
        if self.base.get(self.base_index).is_none() {
            let end = self.text.len();
            self.push_token(T::STATEMENT_BREAK, end, end);
        }
    }

    fn process_space(&mut self) {
        let start = self.base[self.base_index].start;
        let mut end = self.base[self.base_index].end;
        while self.is_base_at(T::SPACE) {
            end = self.base[self.base_index].end;
            self.advance_base();
        }
        if self.is_base_at(T::LINE_BREAK) {
            self.process_line_break(start);
        } else {
            self.push_token(T::SPACE, start, end);
        }
    }

    fn process_line_break(&mut self, start: usize) {
        if self.brace_level == 0 {
            if self.line_has_significant_tokens {
                self.push_token(T::STATEMENT_BREAK, start, start);
            }
            self.line_has_significant_tokens = false;
            self.advance_base();
            self.process_indent(start, Some(T::LINE_BREAK));
        } else {
            self.process_insignificant_line_break(start);
        }
    }

    fn process_insignificant_line_break(&mut self, start: usize) {
        let mut end = self.base[self.base_index].end;
        self.advance_base();
        while self.base.get(self.base_index).is_some_and(|token| WHITESPACE.contains(token.kind)) {
            end = self.base[self.base_index].end;
            self.advance_base();
        }
        self.push_token(T::LINE_BREAK, start, end);
        self.process_special_tokens_pending = true;
    }

    fn process_comment(&mut self) {
        self.line_break_before_first_comment_index = Some(self.queue.len().saturating_sub(1));
        while self.is_base_at(T::COMMENT) {
            let comment = self.base[self.base_index];
            self.queue.push(PendingToken { token: comment, comment_indent: Some(self.last_new_line_indent) });
            self.advance_base();
            if self.is_base_at(T::LINE_BREAK) {
                let start = self.base[self.base_index].start;
                self.process_line_break(start);
            } else if self.base.get(self.base_index).is_none() {
                self.close_dangling_suites_with_comments(0, comment.end);
            } else {
                break;
            }
        }
        self.line_break_before_first_comment_index = None;
    }

    fn process_indent(&mut self, whitespace_start: usize, whitespace_kind: Option<T>) {
        let last_indent = *self.indent_stack.last().unwrap_or(&0);
        let indent = self.get_next_line_indent();
        self.last_new_line_indent = indent as isize;
        let indent = if self.is_base_at(T::COMMENT) { last_indent } else { indent };
        let whitespace_end = self.base.get(self.base_index).map_or(self.text.len(), |token| token.start);
        if indent > last_indent {
            self.indent_stack.push(indent);
            self.push_optional_token(whitespace_kind, whitespace_start, whitespace_end);
            let insert_index = self.skip_preceding_comments_with_indent(indent, self.queue.len() - 1);
            let indent_offset =
                if insert_index == self.queue.len() { whitespace_end } else { self.queue[insert_index].token.start };
            self.insert_token(insert_index, T::INDENT, indent_offset, indent_offset);
        } else if indent < last_indent {
            self.close_dangling_suites_with_comments(indent, whitespace_start);
            self.push_optional_token(whitespace_kind, whitespace_start, whitespace_end);
        } else {
            self.push_optional_token(whitespace_kind, whitespace_start, whitespace_end);
        }
        self.mark_current_plain();
    }

    fn close_dangling_suites_with_comments(&mut self, indent: usize, whitespace_start: usize) {
        let mut last_indent = *self.indent_stack.last().unwrap_or(&0);
        let mut insert_index = self.line_break_before_first_comment_index.unwrap_or(self.queue.len());
        while indent < last_indent {
            let last_suite_indent = self.indent_stack.pop().unwrap_or(0);
            last_indent = *self.indent_stack.last().unwrap_or(&0);
            let mut dedent_offset = whitespace_start;
            if indent > last_indent {
                self.push_token(T::INCONSISTENT_DEDENT, whitespace_start, whitespace_start);
                insert_index = self.queue.len();
            } else {
                insert_index =
                    self.skip_preceding_comments_with_same_indent_on_suite_close(last_suite_indent, insert_index);
            }
            if insert_index != self.queue.len() {
                dedent_offset = self.queue[insert_index].token.start;
            }
            self.insert_token(insert_index, T::DEDENT, dedent_offset, dedent_offset);
            insert_index += 1;
        }
    }

    fn skip_preceding_comments_with_indent(&self, indent: usize, index: usize) -> usize {
        let mut index = index;
        let mut found_comment = false;
        while index > 0 && self.queue[index - 1].comment_indent.is_some() {
            if self.queue[index - 1].comment_indent != Some(indent as isize) {
                break;
            }
            found_comment = true;
            index -= 1;
            if self.is_preceded_by_comment_with_line_break(index) {
                index -= 1;
            }
        }
        if found_comment { index } else { self.queue.len() }
    }

    fn skip_preceding_comments_with_same_indent_on_suite_close(&self, indent: usize, anchor_index: usize) -> usize {
        let mut result = anchor_index;
        for (index, token) in self.queue.iter().enumerate().skip(anchor_index) {
            if let Some(comment_indent) = token.comment_indent {
                if comment_indent < indent as isize {
                    break;
                }
                result = index + 1;
            }
        }
        result
    }

    fn get_next_line_indent(&mut self) -> usize {
        let mut indent = 0;
        while let Some(token) = self.base.get(self.base_index).copied().filter(|token| WHITESPACE.contains(token.kind))
        {
            match token.kind {
                T::TAB => indent = (indent / 8 + 1) * 8,
                T::SPACE => indent += 1,
                T::LINE_BREAK => indent = 0,
                _ => {}
            }
            self.advance_base();
        }
        if self.base.get(self.base_index).is_none() { 0 } else { indent }
    }

    fn is_base_at(&self, kind: T) -> bool {
        self.base.get(self.base_index).is_some_and(|token| token.kind == kind)
    }

    fn push_token(&mut self, kind: T, start: usize, end: usize) {
        self.queue.push(PendingToken { token: self.make_token(kind, start, end), comment_indent: None });
    }

    fn push_optional_token(&mut self, kind: Option<T>, start: usize, end: usize) {
        if let Some(kind) = kind {
            self.push_token(kind, start, end);
        }
    }

    fn insert_token(&mut self, index: usize, kind: T, start: usize, end: usize) {
        self.queue.insert(index, PendingToken { token: self.make_token(kind, start, end), comment_indent: None });
    }

    fn make_token(&self, kind: T, start: usize, end: usize) -> Token {
        Token { kind, start, end, plain: false }
    }

    fn mark_current_plain(&mut self) {
        let plain_state = self.brace_level == 0 && self.indent_stack.len() == 1;
        if let Some(token) = self.base.get_mut(self.base_index) {
            token.plain = plain_state && plain_token(token.kind);
        }
    }

    fn is_preceded_by_comment_with_line_break(&self, index: usize) -> bool {
        index > 1 && self.queue[index - 1].token.kind == T::LINE_BREAK && self.queue[index - 2].comment_indent.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(source: &str) -> Vec<&'static str> {
        lex(source.as_bytes()).into_iter().map(|token| token.kind.debug_name()).collect()
    }

    #[test]
    fn simple_rule_line() {
        assert_eq!(names("x = 0"), ["IDENTIFIER", "SPACE", "=", "SPACE", "INT", "STATEMENT_BREAK"]);
    }

    #[test]
    fn block_indentation_and_dedent() {
        assert_eq!(
            names("if a:\n  b\nc"),
            [
                "if",
                "SPACE",
                "IDENTIFIER",
                ":",
                "STATEMENT_BREAK",
                "LINE_BREAK",
                "INDENT",
                "IDENTIFIER",
                "STATEMENT_BREAK",
                "DEDENT",
                "LINE_BREAK",
                "IDENTIFIER",
                "STATEMENT_BREAK",
            ]
        );
    }

    #[test]
    fn newline_inside_brackets_is_insignificant() {
        assert_eq!(names("(a,\n b)"), ["(", "IDENTIFIER", ",", "LINE_BREAK", "IDENTIFIER", ")", "STATEMENT_BREAK"]);
    }

    #[test]
    fn comments_and_blank_lines_do_not_change_suite_depth() {
        assert_eq!(
            names("if a:\n  b\n  # comment\n\n  c\nd"),
            [
                "if",
                "SPACE",
                "IDENTIFIER",
                ":",
                "STATEMENT_BREAK",
                "LINE_BREAK",
                "INDENT",
                "IDENTIFIER",
                "STATEMENT_BREAK",
                "LINE_BREAK",
                "COMMENT",
                "LINE_BREAK",
                "IDENTIFIER",
                "STATEMENT_BREAK",
                "DEDENT",
                "LINE_BREAK",
                "IDENTIFIER",
                "STATEMENT_BREAK",
            ]
        );
    }

    #[test]
    fn literals_keywords_and_operators() {
        assert_eq!(
            names("x = br'bytes'; y = 0x2a + .5e2 and True"),
            [
                "IDENTIFIER",
                "SPACE",
                "=",
                "SPACE",
                "BYTES",
                ";",
                "SPACE",
                "IDENTIFIER",
                "SPACE",
                "=",
                "SPACE",
                "INT",
                "SPACE",
                "+",
                "SPACE",
                "FLOAT",
                "SPACE",
                "and",
                "SPACE",
                "True",
                "STATEMENT_BREAK",
            ]
        );
    }

    #[test]
    fn line_continuation_does_not_create_statement_break() {
        assert_eq!(
            names("x = 1 + \\\n 2"),
            [
                "IDENTIFIER",
                "SPACE",
                "=",
                "SPACE",
                "INT",
                "SPACE",
                "+",
                "SPACE",
                "LINE_CONTINUATION",
                "SPACE",
                "INT",
                "STATEMENT_BREAK",
            ]
        );
    }

    #[test]
    fn incomplete_strings_and_bad_characters_are_lossless() {
        let source = "x = \"unfinished\nworld\n@";
        let tokens = lex(source.as_bytes());
        assert!(tokens.iter().any(|token| token.kind == T::STRING));
        assert!(tokens.iter().any(|token| token.kind == T::BAD_CHARACTER));
        assert_lossless(source.as_bytes(), &tokens);
    }

    #[test]
    fn seeded_inputs_are_lossless_and_ordered() {
        let alphabet = b"abcXYZ_0129 #()[]{}:+-*/=\\\n\t'\".\r";
        let mut state = 0x5eed_u32;
        for len in 0..256 {
            let mut source = Vec::with_capacity(len);
            for _ in 0..len {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                source.push(alphabet[(state as usize) % alphabet.len()]);
            }
            let tokens = lex(&source);
            assert_lossless(&source, &tokens);
        }
    }

    fn assert_lossless(source: &[u8], tokens: &[Token]) {
        let mut rebuilt = Vec::new();
        let mut previous_end = 0;
        for token in tokens {
            assert!(token.start >= previous_end, "out-of-order token {token:?}");
            assert!(token.end >= token.start, "negative token range {token:?}");
            rebuilt.extend_from_slice(&source[token.start..token.end]);
            previous_end = token.end;
        }
        assert_eq!(rebuilt, source);
    }
}
