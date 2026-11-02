//! The port of IntelliJ's `PsiBuilderImpl` (with `MarkerProduction`, `MarkerPool`, `MarkerOptionalData`) and of the
//! `WhitespacesAndCommentsBinder` contract, generic over the `Language` whose tokens it parses. A language's own
//! builder layers (Kotlin's `SemanticWhitespaceAwarePsiBuilderImpl`) and binders live in its crate.
//!
//! A builder holds the tokens of one text (a file, or the text of a lazily parsed element), a cursor that skips
//! whitespace and comments, and the production list of markers. `begin_bind` balances the markers' edges over
//! whitespace and comments (`balanceWhiteSpaces`) and appends the tree to the arena (`prepareLightTree` + `bind`).
//!
//! Where the Java code throws or only logs on a broken marker sequence (a parser bug), the port does nothing or keeps
//! the tree lossless instead; it never panics.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::counters::{Counter, add};
use crate::language::{ElementType, ElementTypeOf, Language};
use crate::tree::TreeData;

/// A marker (`PsiBuilder.Marker`): a handle into its builder's marker pool. Its methods take the builder, as the Java
/// `marker.done(type)` reaches its builder through the marker: `marker.done(builder, FUN)`.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct Marker(u32);

/// `WhitespacesAndCommentsBinder`: where a marker's edge goes among the whitespace and comments around it. A
/// language's binders are one `Copy` enum, the platform's `WhitespacesBinders` defaults among them.
pub trait WhitespacesAndCommentsBinder<T>: Copy + Eq + std::fmt::Debug + Send + 'static {
    /// `WhitespacesBinders.DEFAULT_LEFT_BINDER` (= `GREEDY_RIGHT_BINDER`): the element starts after them
    /// (`getEdgePosition` is `tokens.size()`).
    const DEFAULT_LEFT_BINDER: Self;
    /// `WhitespacesBinders.DEFAULT_RIGHT_BINDER` (= `GREEDY_LEFT_BINDER`): the element ends before them (0).
    const DEFAULT_RIGHT_BINDER: Self;

    /// `isRecursive()`.
    fn is_recursive(self) -> bool {
        false
    }

    /// `getEdgePosition(tokens, atStreamEdge, getter)`: the edge as an index into `tokens`; `getter(i)` is the text
    /// of `tokens[i]`.
    fn get_edge_position<'t>(self, tokens: &[T], at_stream_edge: bool, getter: &dyn Fn(usize) -> &'t [u8]) -> usize;

    /// What `getEdgePosition` returns on `RelativeTokenTypesView.configure(start, end)` with `start > end`: `size()`
    /// is `size`, negative, so `isEmpty()` is false, `indices` is empty, and `get(i)` reads `myLexTypes[start + i]`
    /// unchecked (`None` outside the tokens); `text(i)` is what `getter.get(i)` returns then (empty outside them).
    fn negative_size_edge<'t>(self, size: i32, get: impl Fn(i32) -> Option<T>, text: &dyn Fn(i32) -> &'t [u8]) -> i32;
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum MarkerKind {
    Start,
    Error,
}

/// A `ProductionMarker` (`StartMarker` or `ErrorItem`) with its `MarkerOptionalData`. The light tree's links are not
/// stored: `bind` reads the production in order instead (see there).
#[derive(Clone, Debug)]
struct MarkerData<T, N, B> {
    kind: MarkerKind,
    /// `myLexemeIndex`; -1 when the marker is free.
    lexeme_index: i32,
    /// `myDoneLexeme`; -1 while not done.
    done_lexeme: i32,
    element_type: Option<ElementType<T, N>>,
    /// The error message (`MarkerOptionalData.getDoneError` of a start marker, `ErrorItem.myMessage` of an error item)
    /// as its index + 1 in the builder's `messages`; 0 for none. Kept apart so the pool's records stay small.
    message: u32,
    left_binder: B,
    right_binder: B,
    collapsed: bool,
    /// The builder's nesting (`PsiBuilderImpl::nesting`) when the marker was collapsed: where a lazy block or lambda
    /// sits, for the chameleon that expands it (not in the Java).
    depth: u16,
}

const NONE: u32 = 0;

/// The head node of the production list (`PsiBuilderImpl::links`).
const HEAD: u32 = 0;
/// The links of a node not in the production list.
const UNLINKED: u32 = u32::MAX;
/// The bit of an extra node (`PsiBuilderImpl::extra_links`).
const EXTRA: u32 = 1 << 31;

/// The node of a marker's start entry in the production list; its done entry's is the next.
#[inline(always)]
fn start_node(marker_id: u32) -> u32 {
    marker_id << 1
}

impl<T, N, B: WhitespacesAndCommentsBinder<T>> MarkerData<T, N, B> {
    fn new(kind: MarkerKind) -> Self {
        MarkerData {
            kind,
            lexeme_index: -1,
            done_lexeme: -1,
            element_type: None,
            message: 0,
            left_binder: B::DEFAULT_LEFT_BINDER,
            right_binder: B::DEFAULT_RIGHT_BINDER,
            collapsed: false,
            depth: 0,
        }
    }

    /// `getLexemeIndex(done)`.
    fn get_lexeme_index(&self, done: bool) -> i32 {
        if done && self.kind == MarkerKind::Start { self.done_lexeme } else { self.lexeme_index }
    }

    /// `setLexemeIndex(lexemeIndex, done)`.
    fn set_lexeme_index(&mut self, lexeme_index: i32, done: bool) {
        if done && self.kind == MarkerKind::Start {
            self.done_lexeme = lexeme_index;
        } else {
            self.lexeme_index = lexeme_index;
        }
    }

    fn is_done(&self) -> bool {
        self.done_lexeme != -1
    }
}

/// A chameleon the tree still has to expand: its node, what parses it (`Language::Lazy`), its text range, and the
/// range of its tokens in the file's own token list when the builder's tokens are a slice of it.
#[derive(Copy, Clone, Debug)]
pub(crate) struct Pending<K> {
    pub node: u32,
    pub kind: K,
    pub start: usize,
    pub end: usize,
    pub file_tokens: Option<(usize, usize)>,
    /// The nesting of the parse where the chameleon sits (`MarkerData::depth`); 0 for a doc comment or a link.
    pub depth: u32,
}

/// How deep a parse may nest (not in the Java, whose recursion has no bound but the thread's stack): the levels of
/// the recursive entry points it is inside (`Nesting`), counted from the nesting of the chameleon it parses
/// (`NESTING_CHAMELEON` over where that sits). Past a limit, the construct being entered is an error element "Code is
/// too deeply nested" over the rest of it (`AbstractKotlinParsing::too_deeply_nested`). The default of
/// `Language::NESTING_LIMIT`, Kotlin's: a level takes at most about 560 bytes of stack in a release build (nested
/// `object A { … }`), so a parse fits a 1 MB stack (0.64 MB suffice for the deepest measured form). The deepest
/// community file reaches 62 levels, 5 nested type argument lists and 4 nested constructs parsed twice.
pub const NESTING_LIMIT: u32 = 1000;
/// How many type argument lists (`tryParseTypeArgumentList`) may nest in one builder (the default of
/// `Language::TYPE_ARGUMENTS_LIMIT`, Kotlin's and Java's). An expression `a<a<…` tries one
/// at every `<` and rolls it back, each nesting the lists of the `<`s after it: so a list that reaches the limit also
/// makes every list starting before that token too deep (`type_arguments_too_deep_before`), and each `<` costs
/// constant time on average, not the limit.
pub const TYPE_ARGUMENTS_LIMIT: u32 = 32;
/// How many constructs that are parsed twice may nest in one builder (the default of `Language::REPARSED_LIMIT`,
/// Kotlin's): a rolled-back attempt parses them and the parse after the rollback parses them again, so the work
/// doubles with each level. Kotlin's are an annotation (`parseAnnotation`: an annotated expression is a declaration's
/// modifiers first, `@A(@A(…))`) and a parenthesized type (`parseTypeRefContents`: then a function type,
/// `((…) -> A) -> A`). At the limit a construct takes 2^10 times its own (not its contents') work. One that reaches the
/// limit also makes every such construct starting before that token too deep (`reparsed_too_deep_before`): a
/// construct parsed again after a rollback of the one around it then costs constant time, not another 2^10 times.
pub const REPARSED_LIMIT: u32 = 10;
/// The message of the error element a construct nested past a limit becomes.
pub const TOO_DEEPLY_NESTED: &str = "Code is too deeply nested";
/// The levels a chameleon (a lazy block or lambda) adds to the nesting where it sits: each suspended frame keeps the
/// tail of its tokens, and each copies its own tokens in.
pub const NESTING_CHAMELEON: u32 = 4;

/// What `PsiBuilderImpl::enter_nesting` enters: a level of a recursive entry point (an expression,
/// `parsePrefixExpression`; a type, `parseTypeRefContents`; a member declaration, `parseMemberDeclaration`), a type
/// argument list, or a construct that is parsed twice; the last two are levels too.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Nesting {
    Level,
    TypeArguments,
    Reparsed,
}

/// For each token `i` of `types`, the first token at or after `i` that closes a bracket opened before `i` (by
/// `depth_change`: +1 opens, -1 closes), or `types.len()` when none does; `out` gets `types.len() + 1` entries. With
/// d[k] the depth before token k, the answer for `i` is the token before the first k > i with d[k] < d[i], found for
/// all i by one backward pass with a stack of increasing depths (as deep as the brackets nest).
pub(crate) fn closers_table<T: Copy>(types: &[T], depth_change: impl Fn(T) -> i64, out: &mut Vec<u32>) {
    let n = types.len();
    out.clear();
    out.resize(n + 1, n as u32);
    let mut d: i64 = types.iter().map(|&t| depth_change(t)).sum();
    let mut stack: Vec<(usize, i64)> = Vec::new();
    for k in (0..=n).rev() {
        if k < n {
            d -= depth_change(types[k]);
        }
        while stack.last().is_some_and(|&(_, top)| top >= d) {
            stack.pop();
        }
        if let Some(&(top, _)) = stack.last() {
            out[k] = (top - 1) as u32;
        }
        stack.push((k, d));
    }
}

/// `bind` drops the tokens before its position from a suspended frame (`drop_bound_tokens`) once they are at least
/// this many and at least half of what it holds.
const DROP_TOKENS_MIN: usize = 1 << 12;

/// Where `bind` is: its walk of the production, kept in the builder so it can stop at a chameleon and resume.
#[derive(Copy, Clone, Debug, Default)]
struct BindState {
    root_marker: u32,
    root_node: u32,
    /// The marker and node leaves and children go to. Their ancestors below the root are the node's parents in the
    /// tree, each open composite's marker stashed in its subtree end (`TreeData::stash`) until it is closed.
    cur_marker: u32,
    cur_node: u32,
    /// The next token to insert, and the end of the tokens to insert before `then`.
    lex_index: usize,
    insert_to: usize,
    /// `lastErrorIndex`.
    last_error_index: i32,
    /// The next production entry.
    i: usize,
    then: BindStep,
}

impl BindState {
    /// The current composite closed: its parent becomes current, with the marker stashed in it (the root's own).
    #[inline(always)]
    fn close_to_parent(&mut self, tree: &TreeData) {
        if self.cur_node != self.root_node {
            self.cur_node = tree.parent_of(self.cur_node);
            self.cur_marker =
                if self.cur_node == self.root_node { self.root_marker } else { tree.stashed(self.cur_node) };
        }
    }
}

/// What `bind` does once the tokens before it are inserted.
#[derive(Copy, Clone, Debug, Default)]
enum BindStep {
    /// Read the next production entry.
    #[default]
    Next,
    /// A done entry: close the current node.
    Close,
    /// A start entry: open its composite, or collapse it into a leaf.
    Open(u32),
    /// An error item: append its error element.
    Error(u32),
    /// The production is read: close what is still open, innermost first.
    CloseOpen,
    CloseOpenOne,
    /// Append the tokens after the root's done.
    RootTail,
    /// Set the root's end.
    Finish,
    Done,
}

/// The error message of marker `id` (`MarkerData::message`), moved out of `messages` (the tree is built once).
fn take_message<T, N, B>(
    markers: &[MarkerData<T, N, B>],
    messages: &mut [Cow<'static, str>],
    id: u32,
) -> Cow<'static, str> {
    match markers[id as usize].message {
        0 => Cow::Borrowed(""),
        i => std::mem::take(&mut messages[i as usize - 1]),
    }
}

/// A production entry as `bind` reads it (`PsiBuilderImpl::entries`): the entry, its balanced lexeme
/// (`getLexemeIndexAt` after `balanceWhiteSpaces`) and, for a start entry, what `bind` needs of its marker.
#[derive(Copy, Clone, Debug)]
struct Entry {
    /// A start marker's or error item's id, or the negated id of a done marker.
    id: i32,
    lex: i32,
    /// A start marker's element type (`ElementType::index`; `ERROR_ELEMENT`'s for none, as `bind` builds it).
    ty: u16,
    /// `ENTRY_ERROR_ITEM`, `ENTRY_COLLAPSED`.
    flags: u16,
}

/// `Entry::flags` of an error item.
const ENTRY_ERROR_ITEM: u16 = 1;
/// `Entry::flags` of a collapsed start marker.
const ENTRY_COLLAPSED: u16 = 2;

/// A builder's tokens as `balanceWhiteSpaces` reads them: types, starts (one more than types) and its text.
struct TokenView<'a, T> {
    types: &'a [T],
    starts: &'a [u32],
    text: &'a [u8],
}

impl<T: Copy> TokenView<'_, T> {
    /// `balanceWhiteSpaces()`' step for one production entry: its lexeme moved over the whitespace and comments
    /// around it as `binder` says, given `lastIndex` and the balanced lexeme of the entry before (`prev`).
    ///
    /// The two default binders take a shortcut, exact by their contract (`WhitespacesAndCommentsBinder`):
    /// `DEFAULT_LEFT_BINDER` answers `tokens.size()` for every view, so the edge is the whitespace's end (`wsEndIndex`)
    /// in all three of the Java's cases (a view of positive, negative or zero size); `DEFAULT_RIGHT_BINDER` answers 0,
    /// so the edge is `wsStartIndex` in all three.
    #[inline(always)]
    fn balanced_lexeme<L: Language<Token = T>>(
        &self,
        binder: L::Binder,
        lexeme_index: i32,
        last_index: i32,
        prev: i32,
    ) -> i32 {
        let count = self.types.len();
        let ws_end = |from: i32| -> i32 {
            let mut i = from.max(0) as usize;
            while i < count && L::is_whitespace_or_comment(self.types[i]) {
                i += 1;
            }
            i as i32
        };
        if binder == L::Binder::DEFAULT_LEFT_BINDER {
            return ws_end(lexeme_index);
        }
        let recursive = binder.is_recursive();
        let prev_production_lex_index = if recursive { 0 } else { prev };
        let mut ws_start_index = lexeme_index.max(last_index);
        while ws_start_index > prev_production_lex_index
            && (ws_start_index as usize) <= count
            && L::is_whitespace_or_comment(self.types[ws_start_index as usize - 1])
        {
            ws_start_index -= 1;
        }
        if binder == L::Binder::DEFAULT_RIGHT_BINDER {
            return ws_start_index;
        }
        self.custom_edge::<L>(binder, lexeme_index, ws_start_index, ws_end(lexeme_index))
    }

    /// The rest of `balanced_lexeme` for a binder that is not a default one.
    #[inline(never)]
    fn custom_edge<L: Language<Token = T>>(
        &self,
        binder: L::Binder,
        lexeme_index: i32,
        ws_start_index: i32,
        ws_end_index: i32,
    ) -> i32 {
        let count = self.types.len();
        if ws_start_index < ws_end_index {
            let start = ws_start_index as usize;
            let end = ws_end_index as usize;
            let at_end = ws_start_index == 0 || end == count;
            let tokens = &self.types[start..end];
            let (text, starts) = (self.text, self.starts);
            let getter = |k: usize| -> &[u8] { &text[starts[start + k] as usize..starts[start + k + 1] as usize] };
            let edge = binder.get_edge_position(tokens, at_end, &getter).min(end - start);
            ws_start_index + edge as i32
        } else if ws_start_index > ws_end_index {
            // An earlier edge moved past this marker's whitespace: Java configures the token view with a negative
            // size and the binder still answers (`WhitespacesAndCommentsBinder::negative_size_edge`).
            let size = ws_end_index - ws_start_index;
            let get = |index: i32| -> Option<T> {
                let i = ws_start_index + index;
                if i >= 0 && (i as usize) < count { Some(self.types[i as usize]) } else { None }
            };
            let text = |index: i32| -> &[u8] {
                let i = ws_start_index + index;
                if i >= 0 && (i as usize) < count {
                    &self.text[self.starts[i as usize] as usize..self.starts[i as usize + 1] as usize]
                } else {
                    &[]
                }
            };
            ws_start_index + binder.negative_size_edge(size, get, &text)
        } else if lexeme_index < ws_start_index {
            ws_start_index
        } else {
            lexeme_index
        }
    }
}

/// `PsiBuilderImpl`.
pub struct PsiBuilderImpl<L: Language> {
    text: Arc<[u8]>,
    /// The absolute offset of this builder's text in `text`, and its length.
    base: usize,
    len: usize,
    /// `myLexStarts`, relative to `base`, `lexeme_count + 1` entries.
    lex_starts: Vec<u32>,
    /// `myLexTypes`.
    lex_types: Vec<L::Token>,
    lexeme_count: usize,
    /// How many tokens `drop_bound_tokens` removed from the front of `lex_types` and `lex_starts` while binding:
    /// token `i` is at `i - token_shift` there (0 while parsing).
    token_shift: usize,
    /// The nesting of the parse (not in the Java; see `NESTING_LIMIT`): the levels it is inside, from the nesting the
    /// builder starts at, and of those the type argument lists and the constructs parsed twice, from 0.
    nesting: u32,
    nesting_limit: u32,
    type_arguments: u32,
    reparsed: u32,
    /// A type argument list starting before this token is too deeply nested: one reached `TYPE_ARGUMENTS_LIMIT` here,
    /// so the lists inside it that start before it would reach it too.
    type_arguments_too_deep_before: usize,
    /// A construct parsed twice starting before this token is too deeply nested: one reached `REPARSED_LIMIT` here.
    reparsed_too_deep_before: usize,
    /// Where token 0 is in the file's token list, when the tokens are an unmodified slice of it.
    file_token: Option<usize>,
    tokens_to_restore_on_rollback: BTreeMap<usize, L::Token>,
    /// `myCurrentLexeme`: it moves over whitespace and comments only where the Java's does, in `eof()` (once after a
    /// move, unless `myTokenTypeChecked` says not to) and in `mark()`.
    current_lexeme: usize,
    /// Where `eof()` puts `current_lexeme` (not in the Java, which skips whitespace there when `myTokenTypeChecked` is
    /// false): computed at every move (`refresh`), past the whitespace and comments when `skips` (a move cleared
    /// `myTokenTypeChecked`), `current_lexeme` itself when not (`rollbackTo` set it). So `eof()` and `getTokenType()`
    /// are a store and a read, without a branch on the flag.
    sig: usize,
    skips: bool,
    /// `myTokenTypeChecked`: whether `eof()` ran since the cursor moved (a store there, read only where a remap
    /// recomputes `sig`: a token remapped into whitespace is skipped only while the flag is false).
    checked: bool,
    /// The token at `sig` and what the language's layer derives from it (`Language::joined_token_type`; Kotlin's
    /// `getJoinedTokenType(raw, 1)`), computed with `sig` wherever `current_lexeme` moves or `lex_types` is written:
    /// `advance_lexer`, `raw_advance_lexer` (and `advance_to_eof` through it), `rollback_to` (which also restores
    /// remapped tokens), `remap_current_token`, `remap_tokens`, `skip_balanced_block` and `mark` when it steps over
    /// whitespace `eof()` would not. It lives here, not in the language's layer, because markers mutate this builder
    /// directly.
    tt_raw: Option<L::Token>,
    tt_joined: Option<L::Token>,
    /// What the language's layer reads as the current token (`current_token`): `tt_joined` while it joins tokens
    /// (`set_joined`, Kotlin's `joinComplexTokens` top), `tt_raw` otherwise, kept with them.
    joins: bool,
    tt_layer: Option<L::Token>,
    /// `MarkerPool.list`; id 0 is no marker.
    markers: Vec<MarkerData<L::Token, L::Node, L::Binder>>,
    free_start_markers: Vec<u32>,
    free_error_items: Vec<u32>,
    /// `MarkerProduction` while parsing, as a doubly linked list (not in the Java's array, whose inserts before a
    /// marker and removals move everything after it): `links[node]` is `[prev, next]`, where node `2 * id` is the
    /// start entry of marker `id` and `2 * id + 1` its done entry, and node 0 is the head whose `prev` is the last
    /// entry; `UNLINKED` for a node not in the list. So `indexOf` is the marker's own node, and `precede`, `doneBefore`,
    /// `errorBefore`, `drop` and `rollbackTo` touch only the entries they add or remove. A second done entry of the
    /// same marker id (a marker done twice, which Java only logs, or one done again after its id was reused while an
    /// old done entry stayed) is an extra node `EXTRA | k`, its links in `extra_links[k]`, its id in `extra_ids[k]`.
    links: Vec<[u32; 2]>,
    extra_links: Vec<[u32; 2]>,
    extra_ids: Vec<i32>,
    /// How many extra nodes are linked; while none, the last done entry of a marker is its own done node.
    extras_linked: u32,
    /// How many entries are linked.
    production_len: usize,
    /// The production as an array, made from the list by `prepare_light_tree` and balanced in the same walk
    /// (`flatten_and_balance`), with what `bind` reads of each entry's marker, so `bind` reads one contiguous array.
    entries: Vec<Entry>,
    /// The error messages markers refer to (`MarkerData::message`).
    messages: Vec<Cow<'static, str>>,
    bind: BindState,
    /// For each token `i`, the first token at or after it that closes a bracket opened before `i`
    /// (`skip_too_deeply_nested`); computed the first time a construct is too deeply nested, empty until then.
    closers: Vec<u32>,
    /// For each token `i`, the first token at or after it that closes a block of `block_pair` opened before `i`
    /// (`balanced_block_end`); computed the first time it is asked for, empty until then.
    block_closers: Vec<u32>,
    block_pair: Option<(L::Token, L::Token)>,
    /// How many tokens `balanced_block_end` has counted before making `block_closers`.
    block_scanned: usize,
    /// `block_closers` answers for the tokens from here on: a bracket remapped before it made it stale there.
    block_valid_from: usize,
    /// The file's `block_closers` for `Language::BLOCK_BRACES` and where token 0 is in the file's tokens, when the
    /// builder's tokens are a slice of the file's and it was given the table (`share_block_closers`).
    shared_block_closers: Option<(Arc<[u32]>, usize)>,
    /// The file's brace pairs for `Language::BLOCK_BRACES` (`Parser`'s `brace_pairs`: for each opening brace of the
    /// file's tokens the index of the one that closes it, `NO_PAIR` if none does) and where token 0 is in the file's
    /// tokens, when the builder's tokens are a slice of the file's (`share_brace_pairs`).
    brace_pairs: Option<(Arc<Vec<u32>>, usize)>,
    /// Whether a token of this builder was remapped from or to a brace of `Language::BLOCK_BRACES` (the pairs, made
    /// from the lexer's tokens, may then be wrong for it).
    brace_remapped: bool,
}

/// `PsiBuilderImpl::brace_pairs` of an opening brace that no brace closes.
pub(crate) const NO_PAIR: u32 = u32::MAX;

/// For each token of `types` that is `open`, the index of the `close` that closes it (the first after it where as
/// many have closed as opened since), `NO_PAIR` if none does; `NO_PAIR` for every other token.
pub(crate) fn brace_pairs<T: Copy + Eq>(types: &[T], open: T, close: T, pairs: &mut Vec<u32>, stack: &mut Vec<u32>) {
    pairs.clear();
    pairs.resize(types.len(), NO_PAIR);
    stack.clear();
    for (i, &t) in types.iter().enumerate() {
        if t == open {
            stack.push(i as u32);
        } else if t == close
            && let Some(o) = stack.pop()
        {
            pairs[o as usize] = i as u32;
        }
    }
}

impl<L: Language> Default for PsiBuilderImpl<L> {
    /// A builder over no text, to be `reset` before use.
    fn default() -> Self {
        let mut builder = PsiBuilderImpl {
            text: Arc::from(&[][..]),
            base: 0,
            len: 0,
            lex_starts: Vec::new(),
            lex_types: Vec::new(),
            lexeme_count: 0,
            token_shift: 0,
            nesting: 0,
            nesting_limit: L::NESTING_LIMIT,
            type_arguments: 0,
            reparsed: 0,
            type_arguments_too_deep_before: 0,
            reparsed_too_deep_before: 0,
            file_token: None,
            tokens_to_restore_on_rollback: BTreeMap::new(),
            current_lexeme: 0,
            sig: 0,
            skips: true,
            checked: false,
            tt_raw: None,
            tt_joined: None,
            joins: false,
            tt_layer: None,
            markers: Vec::new(),
            free_start_markers: Vec::new(),
            free_error_items: Vec::new(),
            links: Vec::new(),
            extra_links: Vec::new(),
            extra_ids: Vec::new(),
            extras_linked: 0,
            production_len: 0,
            entries: Vec::new(),
            messages: Vec::new(),
            bind: BindState::default(),
            closers: Vec::new(),
            block_closers: Vec::new(),
            block_pair: None,
            block_scanned: 0,
            block_valid_from: 0,
            shared_block_closers: None,
            brace_pairs: None,
            brace_remapped: false,
        };
        builder.reset(Arc::from(&[][..]), 0, 0, None);
        builder
    }
}

/// The `PsiBuilder` methods a marker needs: implemented by every builder layer, so `marker.done(builder, kind)` works
/// with any of them.
pub trait AsPsiBuilder {
    type Lang: Language;
    fn psi_builder(&mut self) -> &mut PsiBuilderImpl<Self::Lang>;
}

impl<L: Language> AsPsiBuilder for PsiBuilderImpl<L> {
    type Lang = L;
    fn psi_builder(&mut self) -> &mut PsiBuilderImpl<L> {
        self
    }
}

impl<L: Language> PsiBuilderImpl<L> {
    /// Makes this builder one over `text[base..base + len]` with the token types and starts its caller put in
    /// `tokens_mut()` (starts relative to `base`, the last being `len`): the marker pool, the production and every
    /// other buffer start afresh, kept from the builder before (cleared, not freed), so a `Parser` reuses its builders
    /// in place, never moving one.
    pub(crate) fn reset(&mut self, text: Arc<[u8]>, base: usize, len: usize, file_token: Option<usize>) {
        self.text = text;
        self.base = base;
        self.len = len;
        let lexeme_count = self.lex_types.len();
        self.lex_starts.resize(lexeme_count + 1, len as u32);
        self.lex_starts[lexeme_count] = len as u32;
        self.lexeme_count = lexeme_count;
        self.token_shift = 0;
        self.nesting = 0;
        self.nesting_limit = L::NESTING_LIMIT;
        self.type_arguments = 0;
        self.reparsed = 0;
        self.type_arguments_too_deep_before = 0;
        self.reparsed_too_deep_before = 0;
        self.file_token = file_token;
        self.tokens_to_restore_on_rollback.clear();
        self.current_lexeme = 0;
        self.checked = false;
        self.markers.clear();
        self.markers.push(MarkerData::new(MarkerKind::Start));
        self.free_start_markers.clear();
        self.free_error_items.clear();
        self.links.clear();
        // Marker 0 (no marker): its start node is the head, the empty list; its done node is never linked.
        self.links.push([HEAD, HEAD]);
        self.links.push([UNLINKED, UNLINKED]);
        self.extra_links.clear();
        self.extra_ids.clear();
        self.extras_linked = 0;
        self.production_len = 0;
        self.entries.clear();
        self.messages.clear();
        self.bind = BindState::default();
        self.closers.clear();
        self.block_closers.clear();
        self.block_pair = None;
        self.block_scanned = 0;
        self.block_valid_from = 0;
        self.shared_block_closers = None;
        self.brace_pairs = None;
        self.brace_remapped = false;
        self.refresh(true);
    }

    /// The token types and starts the next `reset` builds over (cleared by none: the caller fills them).
    pub(crate) fn tokens_mut(&mut self) -> (&mut Vec<L::Token>, &mut Vec<u32>) {
        (&mut self.lex_types, &mut self.lex_starts)
    }

    /// Lets go of the text once the tree is built (the tree holds its own reference).
    pub(crate) fn finish(&mut self) {
        self.text = Arc::from(&[][..]);
        self.shared_block_closers = None;
        self.brace_pairs = None;
    }

    /// Computes `sig` and the token there after `current_lexeme` moved or a token was remapped: past whitespace and
    /// comments when `skips` (the Java's `myTokenTypeChecked` is false), at `current_lexeme` when not.
    #[inline(always)]
    fn refresh(&mut self, skips: bool) {
        let current = self.current_lexeme;
        let mut sig = current;
        if skips {
            while sig < self.lexeme_count && L::is_whitespace_or_comment(self.lex_types[sig]) {
                sig += 1;
            }
        }
        self.sig = sig;
        self.skips = skips;
        let raw = if sig < self.lexeme_count { Some(self.lex_types[sig]) } else { None };
        // `joined_token_type` reads the raw tokens after the current one, which `eof()` will have moved to `sig`.
        self.current_lexeme = sig;
        self.tt_joined = L::joined_token_type(self, raw, 1);
        self.current_lexeme = current;
        self.tt_raw = raw;
        self.tt_layer = if self.joins { self.tt_joined } else { raw };
    }

    /// Whether the language's layer reads the joined type as the current token (`current_token`).
    #[inline(always)]
    pub fn set_joined(&mut self, joins: bool) {
        self.joins = joins;
        self.tt_layer = if joins { self.tt_joined } else { self.tt_raw };
    }

    /// `get_token_type()`, or `Language::joined_token_type(raw, 1)` while the layer joins tokens (`set_joined`): what
    /// Kotlin's `SemanticWhitespaceAwarePsiBuilderImpl.getTokenType()` returns (`getJoinedTokenType(raw, 1)` when it
    /// joins complex tokens), one read of the per-position cache (see `sig`).
    ///
    /// Unlike `get_token_type` it does not record `myTokenTypeChecked` (`checked`), a store on the hottest path (0.6 G
    /// instructions per Kotlin round): the flag decides only whether a token at the cursor remapped into whitespace or a
    /// comment is skipped, so a layer that reads the cursor through this never remaps a token into one (Kotlin remaps
    /// only to soft keywords and identifiers).
    #[inline(always)]
    pub fn current_token(&mut self) -> Option<L::Token> {
        self.current_lexeme = self.sig;
        self.tt_layer
    }

    /// Starts the nesting at `nesting`, with `limit` instead of `NESTING_LIMIT` (and no more than `limit` for the
    /// other two limits).
    pub fn set_nesting(&mut self, nesting: u32, limit: u32) {
        self.nesting = nesting;
        self.nesting_limit = limit;
    }

    /// Enters a nested construct (not in the Java): false, with nothing entered, when that would pass a limit. Each
    /// `true` is matched by `exit_nesting(kind)` when the construct is parsed.
    #[inline(always)]
    pub fn enter_nesting(&mut self, kind: Nesting) -> bool {
        if self.nesting >= self.nesting_limit {
            return false;
        }
        match kind {
            Nesting::Level => {}
            Nesting::TypeArguments => {
                if self.current_lexeme < self.type_arguments_too_deep_before {
                    return false;
                }
                if self.type_arguments >= L::TYPE_ARGUMENTS_LIMIT.min(self.nesting_limit) {
                    self.type_arguments_too_deep_before = self.current_lexeme + 1;
                    return false;
                }
                self.type_arguments += 1;
            }
            Nesting::Reparsed => {
                if self.current_lexeme < self.reparsed_too_deep_before {
                    return false;
                }
                if self.reparsed >= L::REPARSED_LIMIT.min(self.nesting_limit) {
                    self.reparsed_too_deep_before = self.current_lexeme + 1;
                    return false;
                }
                self.reparsed += 1;
            }
        }
        self.nesting += 1;
        true
    }

    #[inline(always)]
    pub fn exit_nesting(&mut self, kind: Nesting) {
        self.nesting -= 1;
        match kind {
            Nesting::Level => {}
            Nesting::TypeArguments => self.type_arguments -= 1,
            Nesting::Reparsed => self.reparsed -= 1,
        }
    }

    /// Where the rest of a construct nested too deeply ends when it starts at token `from` (not in the Java): the
    /// first token at or after `from` that closes a bracket (`Language::bracket_depth_change`; Kotlin's `(`, `[`, `{`,
    /// `${`, a string's quotes) opened before `from`, or the end. Constant time: a table of every token's answer is
    /// made, in one pass, the first time it is asked for, so a rolled-back attempt that reaches the limit costs no
    /// more than one that does not.
    fn unmatched_closer(&mut self, from: usize) -> usize {
        if self.closers.is_empty() {
            closers_table(&self.lex_types[..self.lexeme_count], L::bracket_depth_change, &mut self.closers);
        }
        self.closers.get(from).map_or(self.lexeme_count, |&c| c as usize)
    }

    /// Where a block of `open` and `close` that the current token is inside ends (not in the Java): the index of the
    /// first token at or after the current one that closes a block opened before it, or the token count when none
    /// does (`balanced_block_end_at` the current token).
    pub fn balanced_block_end(&mut self, open: L::Token, close: L::Token) -> usize {
        self.balanced_block_end_at(self.current_lexeme, open, close)
    }

    /// The index of the first token at or after `from` that closes a block of `open` and `close` opened before it, or
    /// the token count when none does (not in the Java). It is what a loop finds that counts `open` and `close` from 1
    /// over the tokens from `from` and stops where the count is 0 (whitespace and comments are never `open` or
    /// `close`), in amortized constant time: the raw tokens are counted while the builder has counted fewer than it has
    /// tokens, and then a table of every token's answer is made in one pass, as for `unmatched_closer`. So a builder
    /// asked once (a chameleon with one nested block) scans only that block, and one asked for every level of nested
    /// blocks costs O(tokens). A token remapped from or to `open` or `close` makes the table stale up to it
    /// (`block_valid_from`): before it the tokens are counted. A builder asks for one pair only.
    #[inline]
    pub fn balanced_block_end_at(&mut self, from: usize, open: L::Token, close: L::Token) -> usize {
        debug_assert!(self.block_pair.is_none_or(|pair| pair == (open, close)), "one pair per builder");
        self.block_pair = Some((open, close));
        let (from, n) = (from.min(self.lexeme_count), self.lexeme_count);
        if let Some((pairs, first)) = &self.brace_pairs
            && !self.brace_remapped
            && L::BLOCK_BRACES == Some((open, close))
        {
            // Right after an `open` (whitespace and comments, never braces, between): the loop counting from there
            // stops at the brace that closes it, the file's pair when inside this builder's tokens. Both of Java's
            // callers ask there (`parseBlockLazy` and `braceMatchingBuilder`, after consuming the `{`).
            let mut q = from;
            while q > 0 && L::is_whitespace_or_comment(self.lex_types[q - 1]) {
                q -= 1;
            }
            if q > 0 && self.lex_types[q - 1] == open {
                let pair = pairs[first + q - 1];
                return if pair == NO_PAIR { n } else { (pair as usize - first).min(n) };
            }
        }
        if let Some((table, first)) = &self.shared_block_closers
            && L::BLOCK_BRACES == Some((open, close))
        {
            // The file's answer, unless it is past this builder's tokens: brackets nest, so the first token closing
            // the block is the same in the slice.
            return (table[first + from] as usize).saturating_sub(*first).min(n);
        }
        if self.block_closers.is_empty() && self.block_scanned >= n {
            let change = |t: L::Token| (t == open) as i64 - (t == close) as i64;
            closers_table(&self.lex_types[..n], change, &mut self.block_closers);
            self.block_valid_from = 0;
        }
        if !self.block_closers.is_empty() && from >= self.block_valid_from {
            return self.block_closers[from] as usize;
        }
        let mut count = 1i32;
        for (k, &t) in self.lex_types[from..n].iter().enumerate() {
            if t == open {
                count += 1;
            } else if t == close {
                count -= 1;
                if count == 0 {
                    self.block_scanned += k + 1;
                    return from + k;
                }
            }
        }
        self.block_scanned += n - from;
        n
    }

    /// The number of tokens (`myLexemeCount`).
    pub fn lexeme_count(&self) -> usize {
        self.lexeme_count
    }

    /// The first token of type `token` in `range` (not in the Java), searched over the raw tokens.
    pub fn find_raw_token(&self, range: std::ops::Range<usize>, token: L::Token) -> Option<usize> {
        let end = range.end.min(self.lexeme_count);
        let start = range.start.min(end);
        self.lex_types[start..end].iter().position(|&t| t == token).map(|i| start + i)
    }

    /// Remaps every token of type `from` in `range` to `to` (not in the Java): what `remapCurrentToken(to)` at each of
    /// them does, without moving the cursor.
    pub fn remap_tokens(&mut self, range: std::ops::Range<usize>, from: L::Token, to: L::Token) {
        let end = range.end.min(self.lexeme_count);
        let start = range.start.min(end);
        for i in start..end {
            if self.lex_types[i] == from {
                self.note_remap(i, from, to);
                self.lex_types[i] = to;
            }
        }
        self.refresh(!self.checked);
    }

    /// A remap of token `index` from `old` to `new`: if either is a bracket of `block_pair`, the block table is stale
    /// up to it.
    #[inline(always)]
    fn note_remap(&mut self, index: usize, old: L::Token, new: L::Token) {
        if let Some((open, close)) = L::BLOCK_BRACES
            && (old == open || old == close || new == open || new == close)
        {
            self.brace_remapped = true;
        }
        if let Some((open, close)) = self.block_pair
            && (old == open || old == close || new == open || new == close)
        {
            self.block_valid_from = self.block_valid_from.max(index + 1);
        }
    }

    /// `SyntaxBuilderUtil.parseBlockLazy`'s loop in constant time (not in the Java; `balanced_block_end`): with the
    /// block's `open` consumed, puts the cursor after the `close` that closes the block and returns true, or, when no
    /// `close` does before the first token starting at `stop_offset` or later (a layer's end of the stream) or before
    /// the end, puts it at the first token there that is not whitespace and returns false: where a loop of `eof()`,
    /// `getTokenType()` and `advanceLexer()` counting `open` and `close` from 1 stops, in the same state.
    pub fn skip_balanced_block_to(&mut self, open: L::Token, close: L::Token, stop_offset: Option<u32>) -> bool {
        let current = self.current_lexeme;
        let end = self.balanced_block_end(open, close);
        let stop = stop_offset.unwrap_or(u32::MAX);
        if end < self.lexeme_count && self.lex_starts[end] < stop {
            self.raw_advance_lexer((end + 1 - current) as i32);
            return true;
        }
        let at_stop = current + self.lex_starts[current..self.lexeme_count].partition_point(|&s| s < stop);
        self.raw_advance_lexer((at_stop - current) as i32);
        self.eof();
        false
    }

    /// Gives the builder the file's brace pairs for `Language::BLOCK_BRACES` (`brace_pairs`); its token 0 is the file's
    /// token `first`, and its tokens are a slice of the file's.
    pub(crate) fn share_brace_pairs(&mut self, pairs: Arc<Vec<u32>>, first: usize) {
        debug_assert!(first + self.lexeme_count <= pairs.len());
        self.brace_pairs = Some((pairs, first));
    }

    /// Gives the builder the file's table of `balanced_block_end` answers for `Language::BLOCK_BRACES`
    /// (`file_block_closers`); its token 0 is the file's token `first`, and its tokens are a slice of the file's.
    pub(crate) fn share_block_closers(&mut self, table: Arc<[u32]>, first: usize) {
        debug_assert!(first + self.lexeme_count < table.len());
        self.shared_block_closers = Some((table, first));
    }

    /// The start of token `index` relative to the builder's text, the text's length past the last token.
    pub fn token_start(&self, index: usize) -> u32 {
        self.lex_starts[index.min(self.lexeme_count)]
    }

    /// `getOriginalText()`: this builder's text.
    pub fn get_original_text(&self) -> &[u8] {
        &self.text[self.base..self.base + self.len]
    }

    /// `isWhitespaceOrComment(type)`: the parser definition's whitespace or comment tokens
    /// (`Language::is_whitespace_or_comment`).
    #[inline]
    pub fn is_whitespace_or_comment(&self, element_type: L::Token) -> bool {
        L::is_whitespace_or_comment(element_type)
    }

    /// `whitespaceOrComment(type)` (what Kotlin's `SemanticWhitespaceAwarePsiBuilderImpl.isWhitespaceOrComment` calls).
    pub fn whitespace_or_comment(&self, token: L::Token) -> bool {
        self.is_whitespace_or_comment(token)
    }

    /// `getTokenType()`: the current token after skipping whitespace and comments; `None` at the end.
    #[inline(always)]
    pub fn get_token_type(&mut self) -> Option<L::Token> {
        add(Counter::RawTokenType, 1);
        self.current_lexeme = self.sig;
        self.checked = true;
        self.tt_raw
    }

    /// `remapCurrentToken(type)`.
    pub fn remap_current_token(&mut self, element_type: L::Token) {
        if self.current_lexeme < self.lexeme_count {
            self.note_remap(self.current_lexeme, self.lex_types[self.current_lexeme], element_type);
            self.lex_types[self.current_lexeme] = element_type;
        }
        self.refresh(!self.checked);
    }

    /// `remapCurrentTokenAndRestoreOnRollback(type)`.
    pub fn remap_current_token_and_restore_on_rollback(&mut self, element_type: L::Token) {
        if self.current_lexeme < self.lexeme_count {
            self.tokens_to_restore_on_rollback
                .entry(self.current_lexeme)
                .or_insert(self.lex_types[self.current_lexeme]);
        }
        self.remap_current_token(element_type);
    }

    /// `lookAhead(steps)`: the token `steps` non-whitespace tokens ahead of the current one.
    #[inline(always)]
    pub fn look_ahead(&self, mut steps: i32) -> Option<L::Token> {
        add(Counter::LookAhead, 1);
        add(Counter::LookAheadSteps, steps.max(0) as u64);
        let mut cur = self.shift_over_whitespace_forward(self.current_lexeme);
        while steps > 0 {
            cur = self.shift_over_whitespace_forward(cur + 1);
            steps -= 1;
        }
        if cur < self.lexeme_count { Some(self.lex_types[cur]) } else { None }
    }

    #[inline]
    fn shift_over_whitespace_forward(&self, mut lex_index: usize) -> usize {
        while lex_index < self.lexeme_count && self.is_whitespace_or_comment(self.lex_types[lex_index]) {
            lex_index += 1;
        }
        lex_index
    }

    /// `rawLookup(steps)`: the raw token `steps` away from the current lexeme (whitespace and comments included).
    pub fn raw_lookup(&self, steps: i32) -> Option<L::Token> {
        add(Counter::RawLookup, 1);
        let cur = self.current_lexeme as i64 + steps as i64;
        if cur >= 0 && (cur as usize) < self.lexeme_count { Some(self.lex_types[cur as usize]) } else { None }
    }

    /// `rawTokenTypeStart(steps)`: the start offset of that raw token, -1 before the text, the text length after it.
    pub fn raw_token_type_start(&self, steps: i32) -> i32 {
        let cur = self.current_lexeme as i64 + steps as i64;
        if cur < 0 {
            return -1;
        }
        if cur as usize >= self.lexeme_count {
            return self.len as i32;
        }
        self.lex_starts[cur as usize] as i32
    }

    /// The raw tokens before the current lexeme, the starts of all tokens (relative to this builder's text, the text
    /// length past the last) and this builder's text (not in the Java): what `rawLookup(-i)` and
    /// `rawTokenTypeStart(-i)` read for i = 1, 2, …, for a layer that walks back over them.
    #[inline]
    pub fn raw_tokens_before(&self) -> (&[L::Token], &[u32], &[u8]) {
        let current = self.current_lexeme.min(self.lexeme_count);
        (&self.lex_types[..current], &self.lex_starts, &self.text[self.base..self.base + self.len])
    }

    /// `rawTokenIndex()`.
    pub fn raw_token_index(&self) -> i32 {
        self.current_lexeme as i32
    }

    /// `rawAdvanceLexer(steps)`; a negative count does nothing (Java throws).
    pub fn raw_advance_lexer(&mut self, steps: i32) {
        if steps <= 0 {
            return;
        }
        self.current_lexeme = (self.current_lexeme + steps as usize).min(self.lexeme_count);
        self.checked = false;
        self.refresh(true);
    }

    /// `advanceToEOF()`.
    pub fn advance_to_eof(&mut self) {
        let steps = self.lexeme_count - self.current_lexeme;
        self.raw_advance_lexer(steps as i32);
    }

    /// `advanceLexer()`.
    pub fn advance_lexer(&mut self) {
        add(Counter::Advance, 1);
        if self.eof() {
            return;
        }
        self.current_lexeme += 1;
        self.checked = false;
        self.refresh(true);
    }

    /// A balanced block skipped over the raw tokens (not in the Java; Kotlin's `KotlinParsing.advanceBalancedBlock()`
    /// for `SemanticWhitespaceAwarePsiBuilder::skip_balanced_block`): counts `open` and `close` from the current lexeme
    /// (the block's `open` consumed) and puts the cursor after the `close` that closes the block, or at the end.
    /// Whitespace and comments are never braces, so the braces counted and the lexeme the cursor stops at are those of
    /// a loop of `advanceLexer()` whose token types are never `open` or `close` but where a raw token is one (Kotlin's
    /// joined `?.`/`?:`/`!!` are two lexemes neither of which is a brace).
    pub fn skip_balanced_block(&mut self, open: L::Token, close: L::Token) {
        let start = self.current_lexeme;
        let mut brace_count = 1i32;
        let mut i = start;
        while i < self.lexeme_count {
            let t = self.lex_types[i];
            i += 1;
            if t == open {
                brace_count += 1;
            } else if t == close {
                brace_count -= 1;
                if brace_count == 0 {
                    break;
                }
            }
        }
        if cfg!(feature = "counters") {
            let stepped = self.lex_types[start..i].iter().filter(|&&t| !L::is_whitespace_or_comment(t)).count();
            add(Counter::BalancedSkipTokens, stepped as u64);
        }
        self.current_lexeme = i;
        self.checked = false;
        self.refresh(true);
    }

    /// Moves the cursor over the rest of a construct nested too deeply (not in the Java): from the current token to the
    /// first that closes a bracket opened before it (`unmatched_closer`), or to the end, and not past the first token
    /// starting at `eof_offset` or later when a layer ends the stream there (Kotlin's truncation); over one token at
    /// least, so that the callers' loops go on. The tokens stay raw (no layer joins them). The caller checks `eof()`
    /// first.
    #[cold]
    pub fn skip_too_deeply_nested(&mut self, eof_offset: Option<u32>) {
        let current = self.current_lexeme;
        let mut end = self.unmatched_closer(current).max(current + 1);
        if let Some(eof) = eof_offset {
            let starts = &self.lex_starts[..self.lexeme_count];
            end = end.min(current + starts[current..].partition_point(|&s| s < eof));
        }
        self.raw_advance_lexer((end - current) as i32);
    }

    /// `skipWhitespace()` as `mark()` runs it: to `sig` when that is past the whitespace already; after `rollbackTo`
    /// (no skip pending) the step is made here and `sig` follows it, since `eof()` then would not move.
    #[inline(always)]
    fn skip_whitespace(&mut self) {
        if self.skips {
            self.current_lexeme = self.sig;
            return;
        }
        let current = self.current_lexeme;
        let mut i = current;
        while i < self.lexeme_count && L::is_whitespace_or_comment(self.lex_types[i]) {
            i += 1;
        }
        if i != current {
            self.current_lexeme = i;
            self.refresh(false);
        }
    }

    /// The index of the first token starting at `offset` or later, the token count when none does; `usize::MAX` past
    /// the text's end (not in the Java): a layer that ends the stream at an offset (Java's stopping builder) compares
    /// the cursor with it (`skipped_lexeme`) instead of the current offset with the offset.
    pub fn first_token_at(&self, offset: i32) -> usize {
        if offset as i64 > self.len as i64 {
            return usize::MAX;
        }
        self.lex_starts[..self.lexeme_count].partition_point(|&s| (s as i64) < offset as i64)
    }

    /// The cursor after `eof()`'s skip of whitespace and comments, with `eof()`'s effects (not in the Java):
    /// `getCurrentOffset() >= offset` is `skipped_lexeme() >= first_token_at(offset)` for an offset within the text.
    #[inline(always)]
    pub fn skipped_lexeme(&mut self) -> usize {
        self.current_lexeme = self.sig;
        self.checked = true;
        self.sig
    }

    /// `getCurrentOffset()`: the start of the current token, the text length at the end.
    #[inline(always)]
    pub fn get_current_offset(&mut self) -> i32 {
        if self.eof() {
            return self.len as i32;
        }
        self.lex_starts[self.current_lexeme] as i32
    }

    /// `getTokenText()`: the current token's text, `None` at the end.
    pub fn get_token_text(&mut self) -> Option<&str> {
        let bytes = self.get_token_bytes()?;
        Some(std::str::from_utf8(bytes).unwrap_or("\u{FFFD}"))
    }

    /// `getTokenText()` as bytes.
    #[inline(always)]
    pub fn get_token_bytes(&mut self) -> Option<&[u8]> {
        add(Counter::TokenText, 1);
        if self.eof() {
            return None;
        }
        let start = self.base + self.lex_starts[self.current_lexeme] as usize;
        let end = self.base + self.lex_starts[self.current_lexeme + 1] as usize;
        Some(&self.text[start..end])
    }

    /// `mark()`.
    #[inline(always)]
    pub fn mark(&mut self) -> Marker {
        add(Counter::Mark, 1);
        if self.production_len != 0 {
            self.skip_whitespace();
        }
        let marker = self.create_marker(self.current_lexeme as i32);
        self.link_before(start_node(marker.0), HEAD);
        marker
    }

    #[inline(always)]
    fn create_marker(&mut self, lexeme_index: i32) -> Marker {
        let id = self.allocate(MarkerKind::Start);
        self.markers[id as usize].lexeme_index = lexeme_index;
        Marker(id)
    }

    #[inline(always)]
    fn allocate(&mut self, kind: MarkerKind) -> u32 {
        let free = match kind {
            MarkerKind::Start => self.free_start_markers.pop(),
            MarkerKind::Error => self.free_error_items.pop(),
        };
        match free {
            Some(id) => id,
            None => {
                self.markers.push(MarkerData::new(kind));
                self.links.push([UNLINKED, UNLINKED]);
                self.links.push([UNLINKED, UNLINKED]);
                (self.markers.len() - 1) as u32
            }
        }
    }

    #[inline(always)]
    fn free_marker(&mut self, id: u32) {
        let kind = self.markers[id as usize].kind;
        self.markers[id as usize] = MarkerData::new(kind);
        match kind {
            MarkerKind::Start => self.free_start_markers.push(id),
            MarkerKind::Error => self.free_error_items.push(id),
        }
    }

    #[inline(always)]
    fn marker(&self, id: u32) -> &MarkerData<L::Token, L::Node, L::Binder> {
        &self.markers[id as usize]
    }

    /// Stores `message` as the marker's error message.
    fn set_message(&mut self, id: u32, message: Cow<'static, str>) {
        self.messages.push(message);
        self.markers[id as usize].message = self.messages.len() as u32;
    }

    /// The marker's error message, moved out (the tree is built once).
    fn take_message(&mut self, id: u32) -> Cow<'static, str> {
        take_message(&self.markers, &mut self.messages, id)
    }

    /// `eof()`: skips whitespace and comments the first time after the cursor moved (to `sig`).
    #[inline(always)]
    pub fn eof(&mut self) -> bool {
        self.current_lexeme = self.sig;
        self.checked = true;
        self.sig >= self.lexeme_count
    }

    /// `error(messageText)`: an error element at the current token, unless the last production is an error item there.
    #[cold]
    pub fn error(&mut self, message_text: impl Into<Cow<'static, str>>) {
        let last = self.node_id(self.link(HEAD)[0]);
        if last > 0 {
            let last_marker = self.marker(last as u32);
            if last_marker.kind == MarkerKind::Error && last_marker.lexeme_index == self.current_lexeme as i32 {
                return;
            }
        }
        let id = self.allocate(MarkerKind::Error);
        self.set_message(id, message_text.into());
        self.markers[id as usize].lexeme_index = self.current_lexeme as i32;
        self.link_before(start_node(id), HEAD);
    }

    /// `hasErrorsAfter(marker)`: whether an error item or error marker was added after `marker` and not dropped.
    pub fn has_errors_after(&self, marker: Marker) -> bool {
        let Some(node) = self.index_of(marker.0) else { return false };
        let mut node = self.link(node)[1];
        while node != HEAD {
            let id = self.node_id(node);
            if id > 0 {
                let m = self.marker(id as u32);
                if m.kind == MarkerKind::Error
                    || (m.element_type == Some(ElementType::Node(L::ERROR_ELEMENT)) && m.message != 0)
                {
                    return true;
                }
            }
            node = self.link(node)[1];
        }
        false
    }

    /// `lastDoneMarker` (platform/syntax `SyntaxTreeBuilder`): the marker of the last done entry of the production.
    pub fn last_done_marker(&self) -> Option<Marker> {
        let mut node = self.link(HEAD)[0];
        while node != HEAD && node != UNLINKED {
            let id = self.node_id(node);
            if id < 0 {
                return Some(Marker(id.unsigned_abs()));
            }
            node = self.link(node)[0];
        }
        None
    }

    /// `Marker.getNodeType()`: the element type the marker was done (or collapsed) with, `None` while it is open.
    pub fn marker_node_type(&self, marker: Marker) -> Option<ElementTypeOf<L>> {
        self.markers.get(marker.0 as usize).and_then(|m| m.element_type)
    }

    /// `Marker.getStartTokenIndex()`: the index of the marker's first token (-1 for a dropped marker).
    pub fn marker_start_token_index(&self, marker: Marker) -> i32 {
        self.markers.get(marker.0 as usize).map_or(-1, |m| m.lexeme_index)
    }

    /// `Marker.getEndTokenIndex()`: the index of the token after a done marker (-1 while it is open).
    pub fn marker_end_token_index(&self, marker: Marker) -> i32 {
        self.markers.get(marker.0 as usize).map_or(-1, |m| m.done_lexeme)
    }

    /// `Marker.getStartOffset()`: where the marker's first token starts in this builder's text.
    pub fn marker_start_offset(&self, marker: Marker) -> i32 {
        self.lexeme_offset(self.marker_start_token_index(marker))
    }

    /// `Marker.getEndOffset()`: where a done marker's last token ends in this builder's text.
    pub fn marker_end_offset(&self, marker: Marker) -> i32 {
        self.lexeme_offset(self.marker_end_token_index(marker))
    }

    /// The start of token `lexeme` relative to this builder's text (`lexStart`), the text length past the last one.
    fn lexeme_offset(&self, lexeme: i32) -> i32 {
        let index = (lexeme.max(0) as usize).min(self.lexeme_count);
        self.lex_starts.get(index.saturating_sub(self.token_shift)).map_or(self.len as i32, |&s| s as i32)
    }

    // MarkerProduction

    #[inline(always)]
    fn link(&self, node: u32) -> [u32; 2] {
        if node & EXTRA == 0 { self.links[node as usize] } else { self.extra_links[(node & !EXTRA) as usize] }
    }

    #[inline(always)]
    fn link_mut(&mut self, node: u32) -> &mut [u32; 2] {
        if node & EXTRA == 0 { &mut self.links[node as usize] } else { &mut self.extra_links[(node & !EXTRA) as usize] }
    }

    /// The production entry a node is: a start marker's id, or the negated id of a done marker.
    #[inline(always)]
    fn node_id(&self, node: u32) -> i32 {
        if node & EXTRA != 0 {
            return self.extra_ids[(node & !EXTRA) as usize];
        }
        let id = (node >> 1) as i32;
        if node & 1 == 0 { id } else { -id }
    }

    #[inline(always)]
    fn is_linked(&self, node: u32) -> bool {
        self.link(node)[0] != UNLINKED
    }

    /// Links `node` into the production right before `anchor` (`HEAD`: at the end).
    #[inline(always)]
    fn link_before(&mut self, node: u32, anchor: u32) {
        let prev = self.link(anchor)[0];
        *self.link_mut(node) = [prev, anchor];
        self.link_mut(prev)[1] = node;
        self.link_mut(anchor)[0] = node;
        self.production_len += 1;
        if node & EXTRA != 0 {
            self.extras_linked += 1;
        }
    }

    #[inline(always)]
    fn unlink(&mut self, node: u32) {
        let [prev, next] = self.link(node);
        self.link_mut(prev)[1] = next;
        self.link_mut(next)[0] = prev;
        *self.link_mut(node) = [UNLINKED, UNLINKED];
        self.production_len -= 1;
        if node & EXTRA != 0 {
            self.extras_linked -= 1;
        }
    }

    /// `MarkerProduction.indexOf`: the production entry of a start marker, its own node when linked.
    #[inline(always)]
    fn index_of(&self, marker_id: u32) -> Option<u32> {
        let node = start_node(marker_id);
        self.is_linked(node).then_some(node)
    }

    /// `MarkerProduction.rollbackTo`: removes the entries from the marker's to the end, freeing the markers whose
    /// start entries they are, from the last entry back (the free lists' order).
    fn production_rollback_to(&mut self, marker_id: u32) {
        let Some(first) = self.index_of(marker_id) else { return };
        // The tail from `first` on is cut off at once; its nodes are unlinked from the last back.
        let before = self.link(first)[0];
        let mut node = self.link(HEAD)[0];
        loop {
            let prev = self.link(node)[0];
            let id = self.node_id(node);
            *self.link_mut(node) = [UNLINKED, UNLINKED];
            self.production_len -= 1;
            if node & EXTRA != 0 {
                self.extras_linked -= 1;
            }
            if id > 0 {
                self.free_marker(id as u32);
            }
            if node == first {
                break;
            }
            node = prev;
        }
        self.link_mut(before)[1] = HEAD;
        self.link_mut(HEAD)[0] = before;
    }

    /// `MarkerProduction.dropMarker`: removes the last done entry of the marker (when done) and its start entry.
    fn drop_marker(&mut self, marker_id: u32) {
        if self.marker(marker_id).is_done()
            && let Some(node) = self.last_done_entry(marker_id)
        {
            self.unlink(node);
        }
        if let Some(node) = self.index_of(marker_id) {
            self.unlink(node);
        }
        self.free_marker(marker_id);
    }

    /// `lastIndexOf(-id)`: the marker's done node, unless an extra node is linked, which needs a walk back from the
    /// end.
    fn last_done_entry(&self, marker_id: u32) -> Option<u32> {
        if self.extras_linked == 0 {
            let node = start_node(marker_id) + 1;
            return self.is_linked(node).then_some(node);
        }
        let id = -(marker_id as i32);
        let mut node = self.link(HEAD)[0];
        while node != HEAD {
            if self.node_id(node) == id {
                return Some(node);
            }
            node = self.link(node)[0];
        }
        None
    }

    /// `MarkerProduction.addDone`: a done entry at the end, or before `anchor_before`'s start entry.
    fn add_done(&mut self, marker_id: u32, anchor_before: Option<u32>) {
        let anchor = match anchor_before {
            None => Some(HEAD),
            Some(anchor) => self.index_of(anchor),
        };
        if let Some(anchor) = anchor {
            let mut node = start_node(marker_id) + 1;
            if self.is_linked(node) {
                node = EXTRA | self.extra_links.len() as u32;
                self.extra_links.push([UNLINKED, UNLINKED]);
                self.extra_ids.push(-(marker_id as i32));
            }
            self.link_before(node, anchor);
        }
    }

    /// `MarkerProduction.addBefore`: the marker's start entry before `anchor`'s.
    fn add_before(&mut self, marker_id: u32, anchor: u32) {
        if let Some(anchor) = self.index_of(anchor) {
            self.link_before(start_node(marker_id), anchor);
        }
    }

    /// The production in order as `entries`, balanced (`balanceWhiteSpaces`) and with what `bind` reads of each
    /// entry's marker. `balance` is false when the first entry is not a start marker's: `bind` then reads none.
    ///
    /// The Java balances after making the production an array, reading each entry's lexeme from its marker
    /// (`getLexemeIndexAt`) before any is balanced. One walk does both when no extra done node is linked: then each
    /// lexeme field of a marker belongs to one entry (a start marker's `lexeme_index` to its start entry, its
    /// `done_lexeme` to its done node, an error item's to its start entry), so the entry reads it before balancing
    /// writes it. With an extra node linked, two done entries share a `done_lexeme`, and the array is made first
    /// (`flatten_then_balance`).
    fn flatten_and_balance(&mut self, balance: bool) {
        let size = self.production_len;
        if !balance || self.extras_linked != 0 || size < 2 {
            self.flatten_then_balance(balance);
            return;
        }
        let mut entries = std::mem::take(&mut self.entries);
        entries.clear();
        entries.resize(size, Entry { id: 0, lex: 0, ty: 0, flags: 0 });
        let error_index = ElementType::<L::Token, L::Node>::Node(L::ERROR_ELEMENT).index() as u16;
        let view = TokenView {
            types: &self.lex_types[..self.lexeme_count],
            starts: &self.lex_starts,
            text: &self.text[self.base..self.base + self.len],
        };
        let links = &self.links;
        let markers = &mut self.markers;
        // `balanceWhiteSpaces`' `lastIndex` and the balanced lexeme of the entry before.
        let (mut last_index, mut prev) = (0i32, 0i32);
        // No extra node is linked: every node is a marker's own, `2 * id` its start, `2 * id + 1` its done.
        let mut node = links[HEAD as usize][1];
        let last = size - 1;
        for (i, slot) in entries.iter_mut().enumerate() {
            let item_id = node >> 1;
            let done = node & 1 != 0;
            let m = &mut markers[item_id as usize];
            let (mut lex, binder, ty, flags);
            if m.kind == MarkerKind::Error {
                (lex, binder, ty, flags) = (m.lexeme_index, L::Binder::DEFAULT_RIGHT_BINDER, 0, ENTRY_ERROR_ITEM);
            } else if done {
                (lex, binder, ty, flags) = (m.done_lexeme, m.right_binder, 0, 0);
            } else {
                ty = m.element_type.map_or(error_index, |t| t.index() as u16);
                flags = if m.collapsed { ENTRY_COLLAPSED } else { 0 };
                (lex, binder) = (m.lexeme_index, m.left_binder);
            }
            if i != 0 && i != last {
                let balanced = view.balanced_lexeme::<L>(binder, lex, last_index, prev);
                if balanced != lex {
                    lex = balanced;
                    m.set_lexeme_index(lex, done);
                }
                last_index = lex;
            }
            prev = lex;
            let id = if done { -(item_id as i32) } else { item_id as i32 };
            *slot = Entry { id, lex, ty, flags };
            node = links[node as usize][1];
        }
        self.entries = entries;
    }

    /// `flatten_and_balance` in two passes, as the Java: the production made into the array, every lexeme read from
    /// its marker, then balanced (when `balance`).
    fn flatten_then_balance(&mut self, balance: bool) {
        let size = self.production_len;
        let mut entries = std::mem::take(&mut self.entries);
        entries.clear();
        entries.reserve(size);
        let error_index = ElementType::<L::Token, L::Node>::Node(L::ERROR_ELEMENT).index() as u16;
        let mut node = self.link(HEAD)[1];
        while node != HEAD {
            let id = self.node_id(node);
            let done = id < 0;
            let m = &self.markers[id.unsigned_abs() as usize];
            let lex = m.get_lexeme_index(done);
            let (mut ty, mut flags) = (0u16, 0u16);
            if m.kind == MarkerKind::Error {
                flags = ENTRY_ERROR_ITEM;
            } else if !done {
                ty = m.element_type.map_or(error_index, |t| t.index() as u16);
                if m.collapsed {
                    flags = ENTRY_COLLAPSED;
                }
            }
            entries.push(Entry { id, lex, ty, flags });
            node = self.link(node)[1];
        }
        if balance && size >= 2 {
            let view = TokenView {
                types: &self.lex_types[..self.lexeme_count],
                starts: &self.lex_starts,
                text: &self.text[self.base..self.base + self.len],
            };
            let mut last_index = 0;
            for i in 1..size - 1 {
                let Entry { id, lex, flags, .. } = entries[i];
                let done = id < 0;
                let item = &mut self.markers[id.unsigned_abs() as usize];
                let binder = if flags & ENTRY_ERROR_ITEM != 0 {
                    L::Binder::DEFAULT_RIGHT_BINDER
                } else if done {
                    item.right_binder
                } else {
                    item.left_binder
                };
                let balanced = view.balanced_lexeme::<L>(binder, lex, last_index, entries[i - 1].lex);
                if balanced != lex {
                    item.set_lexeme_index(balanced, done);
                    entries[i].lex = balanced;
                }
                last_index = balanced;
            }
        }
        self.entries = entries;
    }

    // StartMarker

    /// `Marker.precede()`.
    #[inline(always)]
    pub fn precede(&mut self, marker: Marker) -> Marker {
        add(Counter::Precede, 1);
        let lexeme_index = self.marker(marker.0).lexeme_index.max(0);
        let pre = self.create_marker(lexeme_index);
        self.add_before(pre.0, marker.0);
        pre
    }

    /// `Marker.drop()`.
    pub fn drop(&mut self, marker: Marker) {
        add(Counter::Drop, 1);
        self.drop_marker(marker.0);
    }

    /// `Marker.rollbackTo()`.
    pub fn rollback_to(&mut self, marker: Marker) {
        let lexeme_index = self.marker(marker.0).lexeme_index;
        if lexeme_index < 0 {
            return;
        }
        add(Counter::Rollback, 1);
        add(Counter::RollbackLexemes, self.current_lexeme.saturating_sub(lexeme_index as usize) as u64);
        if !self.tokens_to_restore_on_rollback.is_empty() {
            let restored = self.tokens_to_restore_on_rollback.split_off(&(lexeme_index as usize));
            for (index, element_type) in restored {
                self.note_remap(index, self.lex_types[index], element_type);
                self.lex_types[index] = element_type;
            }
        }
        self.current_lexeme = lexeme_index as usize;
        self.checked = true;
        self.refresh(false);
        self.production_rollback_to(marker.0);
    }

    /// `Marker.done(type)`.
    #[inline]
    pub fn done(&mut self, marker: Marker, element_type: impl Into<ElementTypeOf<L>>) {
        self.markers[marker.0 as usize].element_type = Some(element_type.into());
        self.process_done(marker.0, None);
    }

    /// `Marker.collapse(type)`: done, and the tree gets one leaf of `type` over the marker's tokens.
    pub fn collapse(&mut self, marker: Marker, element_type: impl Into<ElementTypeOf<L>>) {
        self.done(marker, element_type);
        let m = &mut self.markers[marker.0 as usize];
        m.collapsed = true;
        m.depth = self.nesting.min(u16::MAX as u32) as u16;
    }

    /// `Marker.doneBefore(type, before)`.
    pub fn done_before(&mut self, marker: Marker, element_type: impl Into<ElementTypeOf<L>>, before: Marker) {
        self.markers[marker.0 as usize].element_type = Some(element_type.into());
        self.process_done(marker.0, Some(before.0));
    }

    /// `Marker.doneBefore(type, before, errorMessage)`: also an error element right before `before`.
    #[cold]
    pub fn done_before_with_error(
        &mut self,
        marker: Marker,
        element_type: impl Into<ElementTypeOf<L>>,
        before: Marker,
        error_message: impl Into<Cow<'static, str>>,
    ) {
        let id = self.allocate(MarkerKind::Error);
        self.set_message(id, error_message.into());
        self.markers[id as usize].lexeme_index = self.marker(before.0).lexeme_index;
        self.add_before(id, before.0);
        self.done_before(marker, element_type, before);
    }

    /// `Marker.error(message)`: done as an error element.
    #[cold]
    pub fn error_marker(&mut self, marker: Marker, message: impl Into<Cow<'static, str>>) {
        self.markers[marker.0 as usize].element_type = Some(ElementType::Node(L::ERROR_ELEMENT));
        self.set_message(marker.0, message.into());
        self.process_done(marker.0, None);
    }

    /// `Marker.errorBefore(message, before)`.
    #[cold]
    pub fn error_before(&mut self, marker: Marker, message: impl Into<Cow<'static, str>>, before: Marker) {
        self.markers[marker.0 as usize].element_type = Some(ElementType::Node(L::ERROR_ELEMENT));
        self.set_message(marker.0, message.into());
        self.process_done(marker.0, Some(before.0));
    }

    /// `Marker.setCustomEdgeTokenBinders(left, right)`; `None` keeps the current binder.
    pub fn set_custom_edge_token_binders(&mut self, marker: Marker, left: Option<L::Binder>, right: Option<L::Binder>) {
        let m = &mut self.markers[marker.0 as usize];
        if let Some(left) = left {
            m.left_binder = left;
        }
        if let Some(right) = right {
            m.right_binder = right;
        }
    }

    /// `processDone`; an error marker's message is set before (`error` and `errorBefore` pass it to the Java's, which
    /// sets it first), so the common `done` passes none.
    #[inline(always)]
    fn process_done(&mut self, marker_id: u32, before: Option<u32>) {
        add(Counter::Done, 1);
        // doValidityChecks: "Marker already done." is only logged in Java.
        let done_lexeme = match before {
            None => self.current_lexeme as i32,
            Some(before) => self.marker(before).lexeme_index,
        };
        let m = self.marker(marker_id);
        let left_bound = matches!(m.element_type, Some(ElementType::Node(n)) if L::is_left_bound(n));
        if left_bound && self.is_empty(m.lexeme_index, done_lexeme) {
            self.set_custom_edge_token_binders(Marker(marker_id), Some(L::Binder::DEFAULT_RIGHT_BINDER), None);
        }
        // Done twice (Java only logs it): the earlier done entry reads the new done lexeme too, as Java's does, since
        // `flatten_and_balance` reads every entry's lexeme from its marker.
        self.markers[marker_id as usize].done_lexeme = done_lexeme;
        self.add_done(marker_id, before);
    }

    fn is_empty(&self, start_idx: i32, end_idx: i32) -> bool {
        let start = start_idx.max(0) as usize;
        let end = (end_idx.max(0) as usize).min(self.lexeme_count);
        (start..end).all(|i| self.is_whitespace_or_comment(self.lex_types[i]))
    }

    // Building the tree

    /// `getTreeBuilt()`, first part: balances the edges and starts appending this builder's tree to `tree`, its root
    /// as the node `root` when given (the chameleon being expanded) or as a new node; returns the root. Then
    /// `resume_bind` appends the rest.
    pub(crate) fn begin_bind(&mut self, tree: &mut TreeData, root: Option<u32>) -> u32 {
        let root_marker = self.prepare_light_tree();
        let base = self.base as u32;
        let root_type = if root_marker == NONE {
            ElementType::Node(L::FILE)
        } else {
            self.marker(root_marker).element_type.unwrap_or(ElementType::Node(L::FILE))
        };
        let root_node = match root {
            Some(node) => node,
            None => tree.add_composite(u32::MAX, root_type, base),
        };
        // At most a leaf per token and a node per production entry (a start or an error).
        tree.reserve(self.lexeme_count + self.entries.len());
        self.bind = if root_marker == NONE {
            // "Parser produced no markers": keep the tree lossless.
            BindState {
                root_marker,
                root_node,
                cur_marker: root_marker,
                cur_node: root_node,
                lex_index: 0,
                last_error_index: -1,
                i: 0,
                insert_to: self.lexeme_count,
                then: BindStep::Finish,
            }
        } else {
            let lex_index = self.marker(root_marker).lexeme_index.max(0) as usize;
            BindState {
                root_marker,
                root_node,
                cur_marker: root_marker,
                cur_node: root_node,
                lex_index,
                last_error_index: -1,
                i: 1,
                insert_to: lex_index,
                then: BindStep::Next,
            }
        };
        root_node
    }

    /// `getTreeBuilt()`, the rest: appends nodes in document order until a chameleon (a doc comment, a markdown link,
    /// a collapsed lazy block or lambda) has been appended, and returns it; the caller appends the chameleon's own
    /// subtree right after it (so node ids are in preorder) and calls this again. `None` when the tree is complete.
    pub(crate) fn resume_bind(&mut self, tree: &mut TreeData) -> Option<Pending<L::Lazy>> {
        self.resume_bind_steps(tree)
    }

    /// `prepareLightTree()`: balances the edges (`balanceWhiteSpaces`, in `flatten_and_balance`); returns the root
    /// marker (0 if there are no markers). The Java then links the markers into a tree, which `bind` instead reads
    /// straight from the production.
    fn prepare_light_tree(&mut self) -> u32 {
        let first = self.link(HEAD)[1];
        let root_marker = if first == HEAD { NONE } else { self.node_id(first).max(0) as u32 };
        self.flatten_and_balance(root_marker != NONE);
        root_marker
    }

    /// `bind()` with `createRootAST`, `insertLeaves`, `collapseLeaves`: appends the leaves and composites to `tree`,
    /// resumable at each chameleon (see `resume_bind`).
    ///
    /// Java walks the light tree `prepareLightTree` links; this walks the production it is linked from, in the same
    /// order, with the same rules: after the root, a start is a child of the current marker and becomes current
    /// (a collapsed one gets its leaf, and its entries up to the done that closes it are skipped), any done closes the
    /// current marker (at its own done lexeme, not the done entry's) and makes its parent current, so entries after the
    /// root's done are the root's children, and an error item is a child unless it is at the same lexeme as the last
    /// error item before it anywhere in the production (`lastErrorIndex`, collapsed markers' items included). Markers
    /// still open at the end are closed innermost first, then the root.
    ///
    /// Every leaf goes to the current node, so the walk is a loop of two steps: insert the tokens up to `insert_to`
    /// as leaves of the current node, then do `then`. A leaf that is a chameleon stops it there (`resume_bind`).
    fn resume_bind_steps(&mut self, tree: &mut TreeData) -> Option<Pending<L::Lazy>> {
        // The state lives in a local while the walk runs (kept in registers), and goes back on a stop.
        let mut st = self.bind;
        let chameleon = self.bind_steps(&mut st, tree);
        self.bind = st;
        if chameleon.is_some() {
            self.drop_bound_tokens(st.lex_index);
        }
        chameleon
    }

    /// Drops the tokens before `lex_index`, which `bind` has passed, while this builder waits for a chameleon (not in
    /// the Java): suspended frames then hold only the tails of their tokens, so nested chameleons over T tokens take
    /// O(T) memory, not O(depth·T). Amortized: only when the tokens dropped are at least `DROP_TOKENS_MIN` and at
    /// least as many as those kept, so each token is moved at most once on average.
    ///
    /// `bind` reads no token before `lex_index` again but the first token of a collapsed marker it has not reached
    /// (`create_leaf` reads its start; its end is clamped to it or after): the drop keeps everything from the smallest
    /// lexeme of a start entry still ahead.
    fn drop_bound_tokens(&mut self, lex_index: usize) {
        let held = self.lex_types.len();
        let worth = |k: usize| k >= DROP_TOKENS_MIN && k * 2 >= held;
        if !worth(lex_index.saturating_sub(self.token_shift).min(held)) {
            return;
        }
        let mut keep = lex_index;
        let ahead = self.bind.i.min(self.entries.len());
        for e in &self.entries[ahead..] {
            if e.id > 0 {
                keep = keep.min(e.lex.max(0) as usize);
            }
        }
        let k = keep.saturating_sub(self.token_shift).min(held);
        if !worth(k) {
            return;
        }
        self.lex_types.drain(..k);
        self.lex_starts.drain(..k);
        self.token_shift += k;
        if self.lex_types.capacity() > 4 * self.lex_types.len() + DROP_TOKENS_MIN {
            self.lex_types.shrink_to_fit();
            self.lex_starts.shrink_to_fit();
        }
    }

    #[inline(always)]
    fn bind_steps(&mut self, st: &mut BindState, tree: &mut TreeData) -> Option<Pending<L::Lazy>> {
        loop {
            while st.lex_index < st.insert_to {
                let to = st.insert_to;
                let stop = self.insert_plain_leaves(st.lex_index, to, st.cur_node, tree);
                if stop == to {
                    st.lex_index = to;
                    break;
                }
                st.lex_index = stop + 1;
                let leaf = ElementType::Token(self.lex_types[stop - self.token_shift]);
                if let Some(chameleon) = self.create_leaf(leaf, stop, stop + 1, st.cur_node, 0, tree) {
                    return Some(chameleon);
                }
            }
            match st.then {
                BindStep::Next => self.bind_entries(st, tree),
                BindStep::Close => {
                    tree.set_end(st.cur_node, self.offset_of(st.lex_index));
                    st.close_to_parent(tree);
                    st.then = BindStep::Next;
                }
                BindStep::Open(id) => {
                    st.then = BindStep::Next;
                    if !self.marker(id).collapsed {
                        st.cur_node = self.create_composite(id, st.cur_node, st.lex_index, tree);
                        tree.stash(st.cur_node, id);
                        st.cur_marker = id;
                        continue;
                    }
                    // collapseLeaves; the collapsed marker's own entries are skipped
                    let size = self.entries.len();
                    let mut depth = 1;
                    while st.i < size && depth > 0 {
                        let inner = self.entries[st.i];
                        st.i += 1;
                        if inner.id < 0 {
                            depth -= 1;
                        } else if inner.flags & ENTRY_ERROR_ITEM == 0 {
                            depth += 1;
                        } else {
                            st.last_error_index = inner.lex;
                        }
                    }
                    let m = self.marker(id);
                    let (start, element_type, depth) = (m.lexeme_index.max(0) as usize, m.element_type, m.depth);
                    let end = (m.done_lexeme.max(0) as usize).clamp(start, self.lexeme_count);
                    st.lex_index = end;
                    st.insert_to = end;
                    if let Some(element_type) = element_type
                        && let Some(chameleon) =
                            self.create_leaf(element_type, start, end, st.cur_node, depth as u32, tree)
                    {
                        return Some(chameleon);
                    }
                }
                BindStep::Error(id) => {
                    st.then = BindStep::Next;
                    self.append_error_item(id, st.cur_node, st.lex_index, tree);
                }
                BindStep::CloseOpen => {
                    if st.cur_node == st.root_node {
                        st.insert_to = self.leaf_end(self.marker(st.root_marker).done_lexeme);
                        st.then = BindStep::RootTail;
                    } else {
                        st.insert_to = self.leaf_end(self.marker(st.cur_marker).done_lexeme);
                        st.then = BindStep::CloseOpenOne;
                    }
                }
                BindStep::CloseOpenOne => {
                    tree.set_end(st.cur_node, self.offset_of(st.lex_index));
                    st.close_to_parent(tree);
                    st.then = BindStep::CloseOpen;
                }
                BindStep::RootTail => {
                    // Tokens after the root's end are not inserted by PsiBuilderImpl (it logs "Tokens ... were not
                    // inserted into the tree"); appending them keeps the tree lossless.
                    st.cur_node = st.root_node;
                    st.insert_to = self.lexeme_count;
                    st.then = BindStep::Finish;
                }
                BindStep::Finish => {
                    tree.set_end(st.root_node, (self.base + self.len) as u32);
                    st.then = BindStep::Done;
                    return None;
                }
                BindStep::Done => return None,
            }
        }
    }

    /// `bind_steps`' walk of the entries from `st.i`: each inserts the leaves before it and opens or closes a
    /// composite, or appends an error element. It returns with `st.then` set to what the state machine does next: the
    /// end (`CloseOpen`), a collapsed marker (`Open`, its leaves to insert first), or the step an entry was at when a
    /// chameleon leaf stopped its leaves (`st.insert_to` their end). The fields it reads are borrowed apart, so the
    /// walk keeps its state in registers.
    #[inline(never)]
    fn bind_entries(&mut self, st: &mut BindState, tree: &mut TreeData) {
        let error_index = ElementType::<L::Token, L::Node>::Node(L::ERROR_ELEMENT).index() as u16;
        let done_own = self.extras_linked == 0;
        let (count, shift, base) = (self.lexeme_count, self.token_shift, self.base as u32);
        let PsiBuilderImpl { entries, lex_types, lex_starts, markers, messages, .. } = self;
        let (entries, types, starts) = (&entries[..], &lex_types[..], &lex_starts[..]);
        let leaf_end = |lexeme: i32| (lexeme.max(0) as usize).min(count);
        let offset = |k: usize| base + starts[k.min(count) - shift];
        let (mut i, mut lex, mut cur_node, mut cur_marker) = (st.i, st.lex_index, st.cur_node, st.cur_marker);
        let (root_node, root_marker) = (st.root_node, st.root_marker);
        let mut last_error = st.last_error_index;
        st.then = loop {
            let Some(&e) = entries.get(i) else { break BindStep::CloseOpen };
            i += 1;
            let (to, then) = if e.id < 0 {
                // done marker; an unbalanced one is only reported in Java
                if cur_node == root_node {
                    continue;
                }
                // It closes the current marker at that marker's done lexeme: the entry's own, when it is that
                // marker's only done entry.
                let done_lexeme = if done_own && e.id.unsigned_abs() == cur_marker {
                    e.lex
                } else {
                    markers[cur_marker as usize].done_lexeme
                };
                (leaf_end(done_lexeme), BindStep::Close)
            } else if e.flags == 0 {
                (leaf_end(e.lex), BindStep::Open(e.id as u32))
            } else if e.flags & ENTRY_COLLAPSED != 0 {
                st.insert_to = leaf_end(e.lex);
                break BindStep::Open(e.id as u32);
            } else if e.lex != last_error {
                // adding only the first (deepest) error from the same lexeme offset
                last_error = e.lex;
                (leaf_end(e.lex), BindStep::Error(e.id as u32))
            } else {
                continue;
            };
            // insertLeaves, but empty tokens (DANGLING_NEWLINE); a chameleon leaf stops the walk there
            let mut stopped = false;
            while lex < to {
                let (s0, s1) = (starts[lex - shift], starts[lex + 1 - shift]);
                if s0 < s1 {
                    let token = types[lex - shift];
                    if L::is_chameleon_token(token) {
                        stopped = true;
                        break;
                    }
                    tree.add_token_leaf(cur_node, token, base + s0, base + s1);
                }
                lex += 1;
            }
            if stopped {
                st.insert_to = to;
                break then;
            }
            match then {
                BindStep::Close => {
                    tree.set_end(cur_node, offset(lex));
                    cur_node = tree.parent_of(cur_node);
                    cur_marker = if cur_node == root_node { root_marker } else { tree.stashed(cur_node) };
                }
                BindStep::Open(id) => {
                    let node = tree.push_composite(cur_node, e.ty, offset(lex));
                    tree.stash(node, id);
                    if e.ty == error_index {
                        tree.set_error(node, take_message(markers, messages, id));
                    }
                    cur_node = node;
                    cur_marker = id;
                }
                _ => {
                    let start = offset(lex);
                    let error = tree.add_composite(cur_node, ElementTypeOf::<L>::Node(L::ERROR_ELEMENT), start);
                    tree.set_end(error, start);
                    tree.set_error(error, take_message(markers, messages, e.id as u32));
                }
            }
        };
        (st.i, st.lex_index, st.cur_node, st.cur_marker, st.last_error_index) =
            (i, lex, cur_node, cur_marker, last_error);
    }

    /// An error item's element: empty, at `lex_index`, a child of `cur_node`.
    fn append_error_item(&mut self, id: u32, cur_node: u32, lex_index: usize, tree: &mut TreeData) {
        let start = self.offset_of(lex_index);
        let error = tree.add_composite(cur_node, ElementTypeOf::<L>::Node(L::ERROR_ELEMENT), start);
        tree.set_end(error, start);
        let message = self.take_message(id);
        tree.set_error(error, message);
    }

    /// `insertLeaves(curToken, lastIdx, curNode)` up to the first token that is a chameleon (a doc comment or a
    /// markdown link), whose index it returns (`to` if none): the tokens `from..to` as leaves of `cur_node`, but empty
    /// ones (DANGLING_NEWLINE), which are not inserted.
    #[inline(always)]
    fn insert_plain_leaves(&self, from: usize, to: usize, cur_node: u32, tree: &mut TreeData) -> usize {
        let base = self.base as u32;
        let shift = self.token_shift;
        let types = &self.lex_types[from - shift..to - shift];
        let starts = &self.lex_starts[from - shift..=to - shift];
        for (i, (&token, edges)) in types.iter().zip(starts.windows(2)).enumerate() {
            if edges[0] < edges[1] {
                if L::is_chameleon_token(token) {
                    return from + i;
                }
                tree.add_token_leaf(cur_node, token, base + edges[0], base + edges[1]);
            }
        }
        to
    }

    /// `insertLeaves(curToken, lastIdx, curNode)` as a step of `bind`: where the tokens to insert next into the
    /// current node end.
    fn leaf_end(&self, last_idx: i32) -> usize {
        (last_idx.max(0) as usize).min(self.lexeme_count)
    }

    fn offset_of(&self, lex_index: usize) -> u32 {
        (self.base + self.lex_starts[lex_index.min(self.lexeme_count) - self.token_shift] as usize) as u32
    }

    fn create_composite(&mut self, marker: u32, parent: u32, lex_index: usize, tree: &mut TreeData) -> u32 {
        let element_type = self.marker(marker).element_type.unwrap_or(ElementType::Node(L::ERROR_ELEMENT));
        let node = tree.add_composite(parent, element_type, self.offset_of(lex_index));
        if element_type == ElementType::Node(L::ERROR_ELEMENT) {
            let message = self.take_message(marker);
            tree.set_error(node, message);
        }
        node
    }

    /// `createLeaf(type, start, end)` over the tokens `first..last`: a leaf, or a chameleon composite (a doc comment, a
    /// markdown link, a collapsed lazy block or lambda, at nesting `depth`), returned for the caller to expand before
    /// the walk goes on.
    fn create_leaf(
        &self,
        element_type: ElementTypeOf<L>,
        first: usize,
        last: usize,
        cur_node: u32,
        depth: u32,
        tree: &mut TreeData,
    ) -> Option<Pending<L::Lazy>> {
        let start = self.base + self.lex_starts[first - self.token_shift] as usize;
        let end = self.base + self.lex_starts[last - self.token_shift] as usize;
        match L::chameleon(element_type) {
            Some(kind) if start < end => {
                let node = tree.add_composite(cur_node, element_type, start as u32);
                tree.set_end(node, end as u32);
                let file_tokens = if L::is_body(kind) { self.file_token.map(|f| (f + first, f + last)) } else { None };
                Some(Pending { node, kind, start, end, file_tokens, depth })
            }
            _ => {
                tree.add_leaf(cur_node, element_type, start as u32, end as u32);
                None
            }
        }
    }
}

impl Marker {
    /// `precede()`.
    pub fn precede(self, b: &mut (impl AsPsiBuilder + ?Sized)) -> Marker {
        b.psi_builder().precede(self)
    }

    /// `drop()`.
    pub fn drop(self, b: &mut (impl AsPsiBuilder + ?Sized)) {
        b.psi_builder().drop(self);
    }

    /// `rollbackTo()`.
    pub fn rollback_to(self, b: &mut (impl AsPsiBuilder + ?Sized)) {
        b.psi_builder().rollback_to(self);
    }

    /// `done(type)`.
    pub fn done<B: AsPsiBuilder + ?Sized>(self, b: &mut B, element_type: impl Into<ElementTypeOf<B::Lang>>) {
        b.psi_builder().done(self, element_type);
    }

    /// `collapse(type)`.
    pub fn collapse<B: AsPsiBuilder + ?Sized>(self, b: &mut B, element_type: impl Into<ElementTypeOf<B::Lang>>) {
        b.psi_builder().collapse(self, element_type);
    }

    /// `doneBefore(type, before)`.
    pub fn done_before<B: AsPsiBuilder + ?Sized>(
        self,
        b: &mut B,
        element_type: impl Into<ElementTypeOf<B::Lang>>,
        before: Marker,
    ) {
        b.psi_builder().done_before(self, element_type, before);
    }

    /// `doneBefore(type, before, errorMessage)`.
    pub fn done_before_with_error<B: AsPsiBuilder + ?Sized>(
        self,
        b: &mut B,
        element_type: impl Into<ElementTypeOf<B::Lang>>,
        before: Marker,
        error_message: impl Into<Cow<'static, str>>,
    ) {
        b.psi_builder().done_before_with_error(self, element_type, before, error_message);
    }

    /// `error(message)`.
    pub fn error(self, b: &mut (impl AsPsiBuilder + ?Sized), message: impl Into<Cow<'static, str>>) {
        b.psi_builder().error_marker(self, message);
    }

    /// `errorBefore(message, before)`.
    pub fn error_before(
        self,
        b: &mut (impl AsPsiBuilder + ?Sized),
        message: impl Into<Cow<'static, str>>,
        before: Marker,
    ) {
        b.psi_builder().error_before(self, message, before);
    }

    /// `setCustomEdgeTokenBinders(left, right)` (`null` = `None`).
    pub fn set_custom_edge_token_binders<B: AsPsiBuilder + ?Sized>(
        self,
        b: &mut B,
        left: Option<<B::Lang as Language>::Binder>,
        right: Option<<B::Lang as Language>::Binder>,
    ) {
        b.psi_builder().set_custom_edge_token_binders(self, left, right);
    }
}
