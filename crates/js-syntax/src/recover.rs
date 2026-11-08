//! A file oxc's parse is fatal for, read by its bytes. oxc's parser recovers from few errors, and a fatal one empties
//! the program, so a file being edited (`foo(`, `if (x ==`, `return a.`), one with merge markers or with an IDE's test
//! markup (`<caret>`) would lose every declaration: each next round of the parse (`with_program`) turns one region around
//! the error into spaces (`blank`), newlines kept, so every offset of the text stays the file's. `trivia` and the
//! scanners are the same lexer's guess, for a consumer that reads a file no round recovers by its text.

use std::ops::Range;

/// Blank the region of the fatal error at `at`, the first of these that holds a byte that is no whitespace: every
/// merge-marker line of the file when `at` is on one; the inside of the innermost `{…}` around `at` when it holds no
/// other block (a body being written, `m() { this.x. }`, keeps its member); `at`'s line; the inside of that innermost
/// block whatever it holds. An error at the end of the text, after its last token, first closes the blocks left open
/// with `}` over its trailing whitespace (a `}` missing: the members before the end keep their class). False when
/// each region is blank already: no round can make progress.
pub fn blank(text: &mut [u8], at: usize) -> bool {
    let last = text.iter().rposition(|c| !c.is_ascii_whitespace());
    let at = match last {
        Some(last) if at > last => {
            let unclosed = open_blocks(text, text.len()).iter().filter(|&&(_, template)| !template).count();
            let mut closed = false;
            for c in text[last + 1..].iter_mut().rev().take(unclosed) {
                *c = b'}';
                closed = true;
            }
            if closed {
                return true;
            }
            last
        }
        _ => at,
    };
    let line = line_of(text, at);
    if is_merge_marker(&text[line.clone()]) {
        let mut start = 0;
        let mut any = false;
        while start < text.len() {
            let end = memchr::memchr(b'\n', &text[start..]).map_or(text.len(), |n| start + n);
            if is_merge_marker(&text[start..end]) {
                any |= fill(&mut text[start..end]);
            }
            start = end + 1;
        }
        return any;
    }
    let block = innermost_block(text, at);
    let leaf = block.clone().filter(|b| !text[b.clone()].contains(&b'{'));
    [leaf, Some(line), block].into_iter().flatten().any(|r| fill(&mut text[r]))
}

/// A line git writes around a conflict: seven of `<`, `|`, `=` or `>`.
fn is_merge_marker(line: &[u8]) -> bool {
    line.len() >= 7 && matches!(line[0], b'<' | b'|' | b'=' | b'>') && line[..7].iter().all(|&c| c == line[0])
}

/// Every byte of `region` that is no ASCII whitespace as a space (a multi-byte character byte by byte, so the text
/// stays UTF-8); whether one was.
fn fill(region: &mut [u8]) -> bool {
    let mut any = false;
    for c in region.iter_mut().filter(|c| !c.is_ascii_whitespace()) {
        *c = b' ';
        any = true;
    }
    any
}

/// The line holding `at`, without its `\n`.
fn line_of(text: &[u8], at: usize) -> Range<usize> {
    let at = at.min(text.len());
    let start = memchr::memrchr(b'\n', &text[..at]).map_or(0, |n| n + 1);
    let end = memchr::memchr(b'\n', &text[at..]).map_or(text.len(), |n| at + n);
    start..end
}

/// The inside of the innermost `{…}` around `at` (`at` may be its `}`), an unclosed one's to the end of the text.
fn innermost_block(text: &[u8], at: usize) -> Option<Range<usize>> {
    let mut open = Vec::new();
    if let Some(closed) = scan(text, at, &mut open) {
        return Some(closed);
    }
    open.iter().rev().find(|&&(o, template)| !template && o < at).map(|&(o, _)| o + 1..text.len())
}

/// The braces open at the end of `text`: (offset, whether a template's `${`), innermost last.
fn open_blocks(text: &[u8], at: usize) -> Vec<(usize, bool)> {
    let mut open = Vec::new();
    scan(text, at, &mut open);
    open
}

/// Scan `text` for its braces, `open` the ones open, innermost last: (offset, whether a template's `${`). Stops at
/// the `}` that closes the innermost block around `at` with that block's inside, or (None) where no brace open
/// before `at` remains open. Braces in strings, comments and template text are none, a `${…}` is no block. A
/// lexer's guess: a regular expression literal is text to it (a quote that does not close on its line opens no
/// string).
fn scan(text: &[u8], at: usize, open: &mut Vec<(usize, bool)>) -> Option<Range<usize>> {
    let mut i = 0;
    while i < text.len() {
        match text[i] {
            b'"' | b'\'' => i = string_end(text, i),
            b'/' if text.get(i + 1) == Some(&b'/') => {
                i = memchr::memchr(b'\n', &text[i..]).map_or(text.len(), |n| i + n)
            }
            b'/' if text.get(i + 1) == Some(&b'*') => {
                i = memchr::memmem::find(&text[i + 2..], b"*/").map_or(text.len(), |n| i + 2 + n + 1)
            }
            b'`' => i = template_end(text, i + 1, open),
            b'{' => open.push((i, false)),
            b'}' => match open.pop() {
                Some((o, false)) if o < at && at <= i => return Some(o + 1..i),
                Some((_, true)) => i = template_end(text, i + 1, open),
                _ => {}
            },
            _ => {}
        }
        if i >= at && open.iter().all(|&(o, _)| o >= at) {
            return None;
        }
        i += 1;
    }
    None
}

/// The offset of the quote that closes the string opening at `i`, or `i` itself when none closes it on its line.
pub fn string_end(text: &[u8], i: usize) -> usize {
    let q = text[i];
    let mut j = i + 1;
    while j < text.len() {
        match text[j] {
            b'\\' => j += 1,
            b'\n' => return i,
            c if c == q => return j,
            _ => {}
        }
        j += 1;
    }
    i
}

/// The offset of the `/` that closes the regular expression opening at `i` (a `/` in a `[…]` class closes none), or
/// `i` itself when none closes it on its line: then it was a division.
pub fn regex_end(text: &[u8], i: usize) -> usize {
    let mut class = false;
    let mut j = i + 1;
    while j < text.len() {
        match text[j] {
            b'\\' => j += 1,
            b'\n' => return i,
            b'[' => class = true,
            b']' => class = false,
            b'/' if !class => return j,
            _ => {}
        }
        j += 1;
    }
    i
}

/// From `i` inside template text, the offset of its closing backtick, or of the `{` of a `${`, pushed to `open`.
fn template_end(text: &[u8], mut i: usize, open: &mut Vec<(usize, bool)>) -> usize {
    while i < text.len() {
        match text[i] {
            b'\\' => i += 1,
            b'`' => return i,
            b'$' if text.get(i + 1) == Some(&b'{') => {
                open.push((i + 1, true));
                return i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    i
}

/// The comments and the strings of `text` (template text a string, its `${…}` code), in order: (start, end, whether
/// a comment). The same lexer's guess as `scan`'s.
pub fn trivia(text: &[u8]) -> Vec<(usize, usize, bool)> {
    let mut out = Vec::new();
    // The `${` open in template literals, as depths of `{` inside them: a `}` at depth 0 returns to template text.
    let mut templates: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let start = i;
        match text[i] {
            b'"' | b'\'' => {
                i = string_end(text, i);
                if i > start {
                    out.push((start, i + 1, false));
                }
            }
            b'/' if text.get(i + 1) == Some(&b'/') => {
                i = memchr::memchr(b'\n', &text[i..]).map_or(text.len(), |n| i + n);
                out.push((start, i, true));
            }
            b'/' if text.get(i + 1) == Some(&b'*') => {
                i = memchr::memmem::find(&text[i + 2..], b"*/").map_or(text.len(), |n| i + 2 + n + 1);
                out.push((start, i + 1, true));
            }
            b'`' => i = template_text(text, i, &mut templates, &mut out),
            b'{' if !templates.is_empty() => *templates.last_mut().expect("not empty") += 1,
            b'}' if templates.last() == Some(&0) => {
                templates.pop();
                i = template_text(text, i, &mut templates, &mut out);
            }
            b'}' if !templates.is_empty() => *templates.last_mut().expect("not empty") -= 1,
            _ => {}
        }
        i += 1;
    }
    out
}

/// From the backtick or `}` at `i`, the template text up to its closing backtick (the returned offset) or to a `${`
/// (its `{`, a depth pushed to `templates`), pushed to `out` as a string.
fn template_text(text: &[u8], i: usize, templates: &mut Vec<usize>, out: &mut Vec<(usize, usize, bool)>) -> usize {
    let mut j = i + 1;
    while j < text.len() {
        match text[j] {
            b'\\' => j += 1,
            b'`' => break,
            b'$' if text.get(j + 1) == Some(&b'{') => {
                templates.push(0);
                out.push((i, j, false));
                return j + 1;
            }
            _ => {}
        }
        j += 1;
    }
    out.push((i, (j + 1).min(text.len()), false));
    j
}

/// A byte of a word: an identifier's, a keyword's or a number's (any byte of a non-ASCII character).
pub fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b >= 0x80
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blanked(src: &str, at: &str) -> String {
        let mut b = src.as_bytes().to_vec();
        assert!(blank(&mut b, src.find(at).unwrap()));
        String::from_utf8(b).unwrap()
    }

    #[test]
    fn the_region_of_an_error() {
        // A block holding no other is blanked inside, the line otherwise, the block when the line is blank.
        assert_eq!(blanked("class A {\n  m() { this.x. }\n}", " }"), "class A {\n  m() {         }\n}");
        assert_eq!(
            blanked("function f() {\n  if (a) { b() }\n  c(\n}", "c("),
            "function f() {\n  if (a) { b() }\n    \n}"
        );
        let mut b = b"f() {\n  { }\n  \n}".to_vec();
        assert!(blank(&mut b, 12));
        assert_eq!(b, b"f() {\n     \n  \n}");
        // A brace in a string, a comment or a template's text is none; a quote that does not close on its line opens
        // no string (a regular expression's).
        assert_eq!(innermost_block(b"f() { '}' /* } */ `}` x }", 22), Some(5..24));
        assert_eq!(innermost_block(b"s.replace(/'/g, ''); g() { x }", 28), Some(26..29));
        // Every merge marker line at once, both sides kept.
        assert_eq!(
            blanked("a()\n<<<<<<< HEAD\nb()\n=======\nc()\n>>>>>>> x\n", "<<<"),
            "a()\n            \nb()\n       \nc()\n         \n"
        );
        // Nothing left to blank: no progress.
        let mut b = b"  \n".to_vec();
        assert!(!blank(&mut b, 3));
        // At the end of the text, the open blocks close over its trailing whitespace first.
        let mut b = b"class A {\n  m() {}\n  ".to_vec();
        let end = b.len();
        assert!(blank(&mut b, end));
        assert_eq!(b, b"class A {\n  m() {}\n }");
        // A multi-byte character stays UTF-8.
        assert_eq!(blanked("x = 'é' +\n", "+"), "          \n");
    }
}
