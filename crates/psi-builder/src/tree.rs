//! The lossless syntax tree: one arena of nodes with `u32` ids, each a kind and a byte range, children in order,
//! tokens as leaves; whitespace and comments are leaves where PSI puts them. `Parser::parse` lexes, parses and builds
//! it, expanding every lazy element (`Language::chameleon`) as PSI does when the tree is walked: Kotlin's doc
//! comments, markdown links, collapsed blocks and lambdas, each reparsed in place where the tree meets it (a stack of
//! builders on the heap, not recursion), so node ids are in preorder.

use std::borrow::Cow;
use std::marker::PhantomData;
use std::ops::Range;
use std::sync::Arc;

use crate::builder::{NESTING_CHAMELEON, Pending, PsiBuilderImpl, TOO_DEEPLY_NESTED, brace_pairs, closers_table};

/// The nesting from which a body reparsed from the file's tokens shares the file's table of block ends
/// (`Language::BLOCK_BRACES`): below it, each frame counts its own blocks (most files never reach it, and pay for no
/// table); above it, every frame of a deep chain of bodies would count again the tokens the frame around it counted.
pub const SHARED_BLOCK_DEPTH: u32 = 64;
use crate::counters::{Counter, add};
use crate::language::{ElementType, ElementTypeOf, Language, NodeType, TokenType, TreeLanguage};
use crate::lexer::Lexed;
use crate::stack::{StackGuard, StackUse};

/// A node of a `Tree`.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct NodeId(pub u32);

const NONE: u32 = u32::MAX;

/// `kinds` bit of a leaf (a token; no children).
const LEAF: u16 = 0x8000;
/// `kinds` bit of a block or lambda left unexpanded (`ParseOptions::expand_bodies`).
const UNEXPANDED: u16 = 0x4000;
/// `kinds` bit of a node that belongs to the token before it rather than to the one after it: Roslyn's trailing trivia
/// (`TreeBuilder::set_trailing`). Languages whose whitespace and comments PSI places by binders never set it.
const TRAILING: u16 = 0x2000;
/// `kinds` bit of a token the parser inserted where one was expected (Roslyn's `IsMissing`): an empty leaf
/// (`TreeBuilder::set_missing`). Kotlin and Java never set it (PSI reports an error element instead).
const MISSING: u16 = 0x1000;
/// `kinds` bits of the element type's `ElementType::index`.
const KIND_INDEX: u16 = 0x0FFF;

/// The arena a builder appends to, nodes in preorder: a node's subtree is the ids `id..subtree_end[id]`, so its first
/// child is `id + 1` when it has one and the next sibling of a child is its `subtree_end` while that is inside the
/// parent's. While building, a node is one record (`nodes`: one push per node); a finished tree (`take_exact`) is three
/// columns (kinds, ranges, links), exactly sized.
#[derive(Default, Debug)]
pub(crate) struct TreeData {
    /// `ElementType::index` of each node, with `LEAF` on a leaf.
    kinds: Vec<u16>,
    /// `[start, end]` byte offsets.
    ranges: Vec<[u32; 2]>,
    /// `[parent, subtree_end]`.
    links: Vec<[u32; 2]>,
    /// The message of each error node, sorted by node id.
    errors: Vec<(u32, Cow<'static, str>)>,
    /// The nesting (`Pending::depth`) of each unexpanded body sitting at a nonzero one, sorted by node id, for
    /// `parse_body` to start where the full parse would.
    body_depths: Vec<(u32, u32)>,
    /// The nodes being built (empty in a finished tree).
    nodes: Vec<BuildNode>,
}

/// A node being built: what the three columns of a finished tree hold of it.
#[derive(Copy, Clone, Debug)]
struct BuildNode {
    kind: u16,
    start: u32,
    end: u32,
    parent: u32,
    subtree_end: u32,
}

impl TreeData {
    #[inline(always)]
    fn push(&mut self, parent: u32, kind: u16, start: u32, end: u32) -> u32 {
        let id = self.nodes.len() as u32;
        self.nodes.push(BuildNode { kind, start, end, parent, subtree_end: id + 1 });
        id
    }

    /// A composite node starting at `start` as the next node, a child of `parent` (`u32::MAX`: a root); its end and
    /// its subtree's are set when it is done (`set_end`), after its descendants are appended.
    pub(crate) fn add_composite<T: TokenType, N: NodeType>(
        &mut self,
        parent: u32,
        kind: ElementType<T, N>,
        start: u32,
    ) -> u32 {
        self.push(parent, kind.index() as u16, start, start)
    }

    /// `add_composite` of the element type whose `ElementType::index` is `kind`.
    #[inline(always)]
    pub(crate) fn push_composite(&mut self, parent: u32, kind: u16, start: u32) -> u32 {
        self.push(parent, kind, start, start)
    }

    /// A leaf as the next node, a child of `parent`.
    pub(crate) fn add_leaf<T: TokenType, N: NodeType>(
        &mut self,
        parent: u32,
        kind: ElementType<T, N>,
        start: u32,
        end: u32,
    ) -> u32 {
        self.push(parent, kind.index() as u16 | LEAF, start, end)
    }

    /// A token leaf as the next node, a child of `parent`.
    #[inline(always)]
    pub(crate) fn add_token_leaf<T: TokenType>(&mut self, parent: u32, token: T, start: u32, end: u32) {
        self.push(parent, token.index() | LEAF, start, end);
    }

    /// Closes a composite: its byte end, and its subtree ends at the last node appended.
    #[inline(always)]
    pub(crate) fn set_end(&mut self, node: u32, end: u32) {
        let len = self.nodes.len() as u32;
        let n = &mut self.nodes[node as usize];
        n.end = end.max(n.start);
        n.subtree_end = len;
    }

    /// The parent of a node being built.
    #[inline(always)]
    pub(crate) fn parent_of(&self, node: u32) -> u32 {
        self.nodes[node as usize].parent
    }

    /// Keeps `value` in an open composite's subtree end until `set_end` writes it (`bind` keeps the marker of each
    /// open composite there, instead of a stack of them).
    #[inline(always)]
    pub(crate) fn stash(&mut self, node: u32, value: u32) {
        self.nodes[node as usize].subtree_end = value;
    }

    /// What `stash` kept in an open composite.
    #[inline(always)]
    pub(crate) fn stashed(&self, node: u32) -> u32 {
        self.nodes[node as usize].subtree_end
    }

    /// Marks a chameleon block or lambda as left unexpanded.
    fn set_unexpanded(&mut self, node: u32) {
        self.nodes[node as usize].kind |= UNEXPANDED;
    }

    /// Room for `additional` more nodes.
    pub(crate) fn reserve(&mut self, additional: usize) {
        self.nodes.reserve(additional);
    }

    pub(crate) fn set_error(&mut self, node: u32, message: Cow<'static, str>) {
        self.errors.push((node, message));
    }

    fn clear(&mut self) {
        self.nodes.clear();
        self.errors.clear();
        self.body_depths.clear();
    }

    /// The heap bytes of a builder's buffers (`nodes`, `errors`, `body_depths`) by capacity; the columns of a
    /// finished tree are `Tree::heap_bytes`.
    fn build_bytes(&self) -> usize {
        use crate::buffers::vec_bytes;
        vec_bytes(&self.nodes) + vec_bytes(&self.errors) + vec_bytes(&self.body_depths)
    }

    /// Empties a builder's buffers, each shrunk to its share of `shrink`.
    fn shrink(&mut self, shrink: crate::buffers::Shrink) {
        shrink.vec(&mut self.nodes);
        shrink.vec(&mut self.errors);
        shrink.vec(&mut self.body_depths);
    }

    /// The finished tree: the nodes as three exactly sized columns; the errors are taken (most files have none: their
    /// list never allocates; the others' is shrunk to fit).
    fn take_exact(&mut self) -> TreeData {
        let mut errors = std::mem::take(&mut self.errors);
        errors.shrink_to_fit();
        TreeData {
            kinds: self.nodes.iter().map(|n| n.kind).collect(),
            ranges: self.nodes.iter().map(|n| [n.start, n.end]).collect(),
            links: self.nodes.iter().map(|n| [n.parent, n.subtree_end]).collect(),
            errors,
            body_depths: self.body_depths.as_slice().to_vec(),
            nodes: Vec::new(),
        }
    }
}

/// A parsed file of language `L`.
pub struct Tree<L: TreeLanguage> {
    text: Arc<[u8]>,
    file_kind: L::FileKind,
    data: TreeData,
    root: u32,
    language: PhantomData<fn() -> L>,
}

/// How `Parser::parse_with_options` builds the tree.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ParseOptions {
    /// Whether the bodies PSI parses lazily (`Language::is_body`: Kotlin's block, `BLOCK`, or lambda,
    /// `LAMBDA_EXPRESSION`, that the parser skipped as a balanced `{...}`) are parsed and expanded. When false they
    /// stay childless nodes with their kind and range (`Tree::is_unexpanded`), and `Tree::parse_body` parses one on
    /// demand into exactly the subtree the full parse has there, because PSI too builds such a body from its text
    /// alone. Other chameleons (doc comments) are expanded either way. True by default.
    pub expand_bodies: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        ParseOptions { expand_bodies: true }
    }
}

/// A reusable parser: owns the buffers every builder of a parse works in (the file's tokens, a chameleon's tokens, the
/// marker pools, the productions, the state stacks), cleared and reused from one file to the next instead of freed.
/// Keep one per thread; `Tree::parse` makes a fresh one per call.
pub struct Parser<L: Language> {
    /// The file's tokens as the lexer made them (a chameleon reuses a slice of them).
    file: Lexed<L::Token>,
    /// A chameleon's text lexed afresh.
    relexed: Lexed<L::Token>,
    /// The frames of the builders still binding, `frames[..depth]`: the file's, then each chameleon's being expanded
    /// inside the one before it. The frames past `depth` are finished ones, reset in place for the next builders
    /// (a frame is a builder with all its buffers: none is moved or rebuilt per chameleon).
    frames: Vec<L::Frame>,
    depth: usize,
    /// The tree being built; copied into exactly sized buffers when done.
    data: TreeData,
    options: ParseOptions,
    /// `NESTING_LIMIT`, or what `set_nesting_limit` set.
    nesting_limit: Option<u32>,
    /// The table of block ends of the file's tokens (`PsiBuilderImpl::balanced_block_end` over `self.file`), made for
    /// the first body deeper than `SHARED_BLOCK_DEPTH` and shared by every such body of the file.
    file_block_closers: Option<Arc<[u32]>>,
    /// The file's brace pairs (`builder::brace_pairs` over `self.file` for `Language::BLOCK_BRACES`), made once per
    /// file and shared by every builder over a slice of the file's tokens; its buffer is refilled in place once no
    /// builder holds it. `pair_stack` is the scratch of the pass.
    file_brace_pairs: Arc<Vec<u32>>,
    pair_stack: Vec<u32>,
    /// What `set_retained_bytes` set (`buffers::RETAINED_BYTES` by default).
    retained_bytes: usize,
    /// `heap_bytes` when the last parse ended, before it shrank to `retained_bytes`.
    grown_bytes: usize,
    /// The most the builders of the last parse took of the stack (`stack_use`).
    stack_use: StackUse,
}

impl<L: Language> Default for Parser<L> {
    fn default() -> Self {
        Parser {
            file: Lexed::default(),
            relexed: Lexed::default(),
            frames: Vec::new(),
            depth: 0,
            data: TreeData::default(),
            options: ParseOptions::default(),
            nesting_limit: None,
            file_block_closers: None,
            file_brace_pairs: Arc::new(Vec::new()),
            pair_stack: Vec::new(),
            retained_bytes: crate::buffers::RETAINED_BYTES,
            grown_bytes: 0,
            stack_use: StackUse::default(),
        }
    }
}

/// A chameleon being expanded, for `Language::parse_chameleon`: its text and the builders it can be parsed with.
pub struct Chameleon<'a, L: Language> {
    text: &'a Arc<[u8]>,
    pending: Pending<L::Lazy>,
    file: &'a Lexed<L::Token>,
    relexed: &'a mut Lexed<L::Token>,
    /// `Parser::file_block_closers`.
    file_block_closers: &'a mut Option<Arc<[u32]>>,
    /// `Parser::file_brace_pairs`, when the language has `BLOCK_BRACES`.
    file_brace_pairs: Option<&'a Arc<Vec<u32>>>,
    /// The frame it is parsed on: one of the methods below makes its builder.
    pub frame: &'a mut L::Frame,
    /// Its nesting (`NESTING_CHAMELEON` over where it sits) and the parser's limit.
    nesting: u32,
    limit: u32,
    /// Where its parse starts on the stack: in `Parser::begin_chameleon`, the same frame for a body parsed in the
    /// file's parse and by `parse_body`, so both stop at the same levels.
    stack: StackGuard,
    /// How the file it is in is parsed.
    file_kind: L::FileKind,
}

impl<'a, L: Language> Chameleon<'a, L> {
    /// How the file the chameleon is in is parsed (what `Language::tokenize_into` and `parse_file` got for it).
    pub fn file_kind(&self) -> L::FileKind {
        self.file_kind
    }

    /// The chameleon's text.
    pub fn text(&self) -> &'a [u8] {
        let text: &'a Arc<[u8]> = self.text;
        &text[self.pending.start..self.pending.end]
    }

    /// The token types and starts `builder` makes the builder over (relative to the chameleon's start, one more
    /// start than types), for the language to fill.
    pub fn token_buffers(&mut self) -> (&mut Vec<L::Token>, &mut Vec<u32>) {
        L::frame_builder(self.frame).tokens_mut()
    }

    /// The frame's builder over the tokens in `token_buffers`, at nesting 0 with the parser's limit: a doc comment or
    /// a link holds no body, so its frame is bounded by its own tokens wherever it sits.
    pub fn builder(&mut self) -> &mut PsiBuilderImpl<L> {
        let p = &self.pending;
        let builder = L::frame_builder(self.frame);
        builder.reset(self.text.clone(), p.start, p.end - p.start, None);
        builder.set_nesting(0, self.limit, self.stack);
        builder
    }

    /// The frame's builder over the chameleon's text lexed afresh with `Language::tokenize_into`, at nesting 0 with
    /// the parser's limit.
    pub fn relexed_builder(&mut self) -> &mut PsiBuilderImpl<L> {
        let p = &self.pending;
        L::tokenize_into(&self.text[p.start..p.end], self.file_kind, self.relexed);
        add(Counter::RelexedTokens, self.relexed.len() as u64);
        let (types, starts) = L::frame_builder(self.frame).tokens_mut();
        types.clear();
        types.extend_from_slice(&self.relexed.types);
        starts.clear();
        starts.extend_from_slice(&self.relexed.starts);
        self.builder()
    }

    /// The frame's builder over a body's tokens (`Language::is_body`) at its nesting: the file's own tokens when they
    /// lex alone the same (`reusable_tokens`), its text lexed afresh otherwise.
    pub fn body_builder(&mut self) -> &mut PsiBuilderImpl<L> {
        let p = &self.pending;
        let (types, starts) = L::frame_builder(self.frame).tokens_mut();
        let file_token = match reusable_tokens(self.file, p) {
            Some((first, last)) => {
                add(Counter::ReusedTokens, (last - first) as u64);
                types.clear();
                types.extend_from_slice(&self.file.types[first..last]);
                starts.clear();
                starts.extend(self.file.starts[first..=last].iter().map(|s| s - p.start as u32));
                Some(first)
            }
            None => {
                L::tokenize_into(&self.text[p.start..p.end], self.file_kind, self.relexed);
                add(Counter::RelexedTokens, self.relexed.len() as u64);
                types.clear();
                types.extend_from_slice(&self.relexed.types);
                starts.clear();
                starts.extend_from_slice(&self.relexed.starts);
                None
            }
        };
        let delegate = L::frame_builder(self.frame);
        delegate.reset(self.text.clone(), p.start, p.end - p.start, file_token);
        delegate.set_nesting(self.nesting, self.limit, self.stack);
        if let (Some(first), Some(pairs)) = (file_token, self.file_brace_pairs) {
            delegate.share_brace_pairs(pairs.clone(), first);
        }
        if let (Some(first), Some((open, close))) = (file_token, L::BLOCK_BRACES)
            && p.depth >= SHARED_BLOCK_DEPTH
        {
            let table = self.file_block_closers.get_or_insert_with(|| {
                let mut table = Vec::new();
                closers_table(&self.file.types, |t| (t == open) as i64 - (t == close) as i64, &mut table);
                table.into()
            });
            delegate.share_block_closers(table.clone(), first);
        }
        delegate
    }
}

impl<L: Language> Parser<L> {
    pub fn new() -> Self {
        Self::default()
    }

    /// A parser for one parse, which it never shrinks after (its buffers go with it).
    fn once() -> Self {
        Parser { retained_bytes: usize::MAX, ..Self::default() }
    }

    /// Parses with `limit` instead of `Language::NESTING_LIMIT` (tests: a small limit puts the too-deep paths on ordinary
    /// inputs). Not part of the consumer contract.
    #[doc(hidden)]
    pub fn set_nesting_limit(&mut self, limit: u32) {
        self.nesting_limit = Some(limit);
    }

    fn limit(&self) -> u32 {
        self.nesting_limit.unwrap_or(L::NESTING_LIMIT)
    }

    /// The deepest level a builder of the last parse (or `parse_body`) counted and the most stack the levels of one
    /// took from where it started (see `crate::stack`; measurements and tests).
    #[doc(hidden)]
    pub fn stack_use(&self) -> StackUse {
        self.stack_use
    }

    /// The heap bytes the parser's buffers hold between parses, by capacity: the file's tokens, a chameleon's, every
    /// frame's builder (tokens, markers, production, entries, tables), the tree being built and the file's tables.
    pub fn heap_bytes(&self) -> usize {
        use crate::buffers::vec_bytes;
        let frames: usize = self.frames.iter().map(|f| L::frame_builder_ref(f).heap_bytes()).sum();
        self.file.heap_bytes()
            + self.relexed.heap_bytes()
            + vec_bytes(&self.frames)
            + frames
            + self.data.build_bytes()
            + self.file_block_closers.as_ref().map_or(0, |t| t.len() * size_of::<u32>())
            + vec_bytes(&self.file_brace_pairs)
            + vec_bytes(&self.pair_stack)
    }

    /// Shrinks the buffers to at most `bytes` when they hold more (`heap_bytes`): each is emptied and keeps the same
    /// share of its capacity (`buffers::Shrink`), the frames past that share are dropped. `shrink_to(0)` frees them
    /// all. A parse after it grows what it needs again.
    pub fn shrink_to(&mut self, bytes: usize) {
        let total = self.heap_bytes();
        if total <= bytes {
            return;
        }
        // the frames keep their first (`Shrink::nested`) past their share: its inline bytes are kept out of the
        // others' share, so the buffers hold at most `bytes`
        let first_frame = if bytes == 0 || self.frames.is_empty() { 0 } else { size_of::<L::Frame>() };
        let shrink = crate::buffers::Shrink::new(bytes.saturating_sub(first_frame), total);
        self.file.shrink(shrink);
        self.relexed.shrink(shrink);
        shrink.nested(&mut self.frames, |f| L::frame_builder(f).shrink(shrink));
        self.data.shrink(shrink);
        self.file_block_closers = None;
        match Arc::get_mut(&mut self.file_brace_pairs) {
            Some(pairs) => shrink.vec(pairs),
            None => self.file_brace_pairs = Arc::new(Vec::new()),
        }
        shrink.vec(&mut self.pair_stack);
    }

    /// The most the buffers keep from one file to the next (`buffers::RETAINED_BYTES` by default): after a parse that
    /// leaves more than `bytes` in them, they are shrunk to half of it (`shrink_to(buffers::retained_after(bytes))`);
    /// after one that leaves `bytes` or less, they are kept as they are. `usize::MAX` keeps all, `0` frees all after
    /// every file.
    pub fn set_retained_bytes(&mut self, bytes: usize) {
        self.retained_bytes = bytes;
    }

    /// `heap_bytes` when the last parse ended, before it shrank to the retained bytes (measurements).
    #[doc(hidden)]
    pub fn grown_bytes(&self) -> usize {
        self.grown_bytes
    }

    /// The end of a parse: the bytes it grew to; past the retained bytes, the buffers shrunk to half of them
    /// (`buffers::retained_after`).
    fn retain(&mut self) {
        self.grown_bytes = self.heap_bytes();
        if self.grown_bytes > self.retained_bytes {
            self.shrink_to(crate::buffers::retained_after(self.retained_bytes));
        }
    }

    /// Parses `text` as a file of `file_kind` (see `Tree::parse`).
    pub fn parse(&mut self, text: impl AsRef<[u8]>, file_kind: L::FileKind) -> Tree<L> {
        self.parse_with_options(text, file_kind, ParseOptions::default())
    }

    /// `parse` with `options` (e.g. bodies left unexpanded for a pass that reads only declarations).
    pub fn parse_with_options(
        &mut self,
        text: impl AsRef<[u8]>,
        file_kind: L::FileKind,
        options: ParseOptions,
    ) -> Tree<L> {
        self.options = options;
        let tree = self.parse_with(text.as_ref(), file_kind, |frame| L::parse_file(frame, file_kind));
        self.options = ParseOptions::default();
        tree
    }

    /// The subtree of the body `id` of `tree` that `ParseOptions { expand_bodies: false }` left unexpanded
    /// (`Tree::is_unexpanded`), as its own tree over the same source: its root is a node of the body's kind and
    /// range, its ids start at 0, its ranges are offsets in `tree.source()`. Bodies inside it are expanded as
    /// `options` says. `None` when `id` is not an unexpanded body.
    pub fn parse_body(&mut self, tree: &Tree<L>, id: NodeId, options: ParseOptions) -> Option<Tree<L>> {
        if !tree.is_unexpanded(id) {
            return None;
        }
        let kind = tree.node_kind(id).and_then(|n| L::chameleon(ElementType::Node(n))).filter(|&k| L::is_body(k))?;
        let range = tree.range(id);
        self.options = options;
        let mut data = std::mem::take(&mut self.data);
        data.clear();
        let root = data.add_composite(NONE, tree.kind(id), range.start as u32);
        data.set_end(root, range.end as u32);
        let depth = match tree.data.body_depths.binary_search_by_key(&id.0, |&(node, _)| node) {
            Ok(i) => tree.data.body_depths[i].1,
            Err(_) => 0,
        };
        let chameleon = Pending { node: root, kind, start: range.start, end: range.end, file_tokens: None, depth };
        self.depth = 0;
        self.stack_use = StackUse::default();
        self.begin_chameleon(&tree.text, &mut data, chameleon, tree.file_kind);
        self.bind_frames(&tree.text, &mut data, tree.file_kind);
        let body = data.take_exact();
        self.data = data;
        self.options = ParseOptions::default();
        self.retain();
        Some(Tree { text: tree.text.clone(), file_kind: tree.file_kind, data: body, root, language: PhantomData })
    }

    /// Lexes `text`, runs `parse` on the frame whose builder holds its tokens, builds and expands the tree.
    /// `parse_with_options` runs `Language::parse_file`; tests run their own.
    #[doc(hidden)]
    pub fn parse_with(&mut self, text: &[u8], file_kind: L::FileKind, parse: impl FnOnce(&mut L::Frame)) -> Tree<L> {
        let text: Arc<[u8]> = Arc::from(&text[..text.len().min(u32::MAX as usize - 1)]);
        L::tokenize_into(&text, file_kind, &mut self.file);
        self.file_block_closers = None;
        if let Some((open, close)) = L::BLOCK_BRACES {
            // Refilled in place: the builders of the file before let go of it in `finish`.
            match Arc::get_mut(&mut self.file_brace_pairs) {
                Some(pairs) => brace_pairs(&self.file.types, open, close, pairs, &mut self.pair_stack),
                None => {
                    let mut pairs = Vec::new();
                    brace_pairs(&self.file.types, open, close, &mut pairs, &mut self.pair_stack);
                    self.file_brace_pairs = Arc::new(pairs);
                }
            }
        }
        add(Counter::FileTokens, self.file.len() as u64);
        let mut data = std::mem::take(&mut self.data);
        data.clear();

        if self.frames.is_empty() {
            self.frames.push(L::Frame::default());
        }
        let limit = self.limit();
        let frame = &mut self.frames[0];
        let file = L::frame_builder(frame);
        let (types, starts) = file.tokens_mut();
        types.clear();
        types.extend_from_slice(&self.file.types);
        starts.clear();
        starts.extend_from_slice(&self.file.starts);
        file.reset(text.clone(), 0, text.len(), Some(0));
        file.set_nesting(0, limit, StackGuard::here());
        if L::BLOCK_BRACES.is_some() {
            file.share_brace_pairs(self.file_brace_pairs.clone(), 0);
        }
        parse(frame);
        self.stack_use = L::frame_builder(frame).stack_use();
        let root = L::frame_builder(frame).begin_bind(&mut data, None);
        self.depth = 1;
        self.bind_frames(&text, &mut data, file_kind);

        let tree = data.take_exact();
        self.data = data;
        self.retain();
        Tree { text, file_kind, data: tree, root, language: PhantomData }
    }

    /// Runs the builders on `frames` to the end: the top one appends its nodes until it appends a chameleon, whose
    /// builder then goes on top and appends the chameleon's subtree right after it. So node ids are in preorder, and
    /// nesting takes heap, not stack.
    fn bind_frames(&mut self, text: &Arc<[u8]>, data: &mut TreeData, file_kind: L::FileKind) {
        while self.depth > 0 {
            let top = self.depth - 1;
            match L::frame_builder(&mut self.frames[top]).resume_bind(data) {
                Some(chameleon) if !self.options.expand_bodies && L::is_body(chameleon.kind) => {
                    data.set_unexpanded(chameleon.node);
                    if chameleon.depth > 0 {
                        data.body_depths.push((chameleon.node, chameleon.depth));
                    }
                }
                Some(chameleon) => self.begin_chameleon(text, data, chameleon, file_kind),
                None => {
                    L::frame_builder(&mut self.frames[top]).finish();
                    self.depth = top;
                }
            }
        }
    }

    /// Parses one chameleon's text with the next frame's builder (`Language::parse_chameleon`) and starts appending
    /// its subtree under its node, that frame on top. A body (`Language::is_body`) whose nesting (`NESTING_CHAMELEON`
    /// over where it sits) passes the limit is not parsed: its subtree is appended whole, an error element "Code is
    /// too deeply nested" over its tokens, and no frame is added. Never inlined: its frame is where a chameleon's
    /// parse starts on the stack (`Chameleon::stack`), the same under `bind_frames` and `parse_body`.
    #[inline(never)]
    fn begin_chameleon(&mut self, text: &Arc<[u8]>, data: &mut TreeData, p: Pending<L::Lazy>, file_kind: L::FileKind) {
        let nesting = p.depth.saturating_add(NESTING_CHAMELEON);
        if L::is_body(p.kind) && nesting > self.limit() {
            self.append_too_deep(text, data, &p, file_kind);
            return;
        }
        let limit = self.limit();
        if self.depth == self.frames.len() {
            self.frames.push(L::Frame::default());
        }
        let frame = &mut self.frames[self.depth];
        let mut chameleon = Chameleon {
            text,
            pending: p,
            file: &self.file,
            relexed: &mut self.relexed,
            file_block_closers: &mut self.file_block_closers,
            file_brace_pairs: L::BLOCK_BRACES.map(|_| &self.file_brace_pairs),
            frame,
            nesting,
            limit,
            stack: StackGuard::here(),
            file_kind,
        };
        L::parse_chameleon(p.kind, &mut chameleon);
        let builder = L::frame_builder(&mut self.frames[self.depth]);
        self.stack_use = self.stack_use.max(builder.stack_use());
        builder.begin_bind(data, Some(p.node));
        self.depth += 1;
    }

    /// The subtree of a body nested too deeply to parse: under its node, one error element "Code is too deeply nested"
    /// with its tokens as leaves (as the lexer makes them; an empty token is not a leaf, a doc comment is not
    /// expanded).
    fn append_too_deep(&mut self, text: &Arc<[u8]>, data: &mut TreeData, p: &Pending<L::Lazy>, file_kind: L::FileKind) {
        let (types, starts, base): (&[L::Token], &[u32], u32) = match reusable_tokens(&self.file, p) {
            Some((first, last)) => (&self.file.types[first..last], &self.file.starts[first..=last], 0),
            None => {
                L::tokenize_into(&text[p.start..p.end], file_kind, &mut self.relexed);
                (&self.relexed.types, &self.relexed.starts, p.start as u32)
            }
        };
        let error = data.add_composite(p.node, ElementTypeOf::<L>::Node(L::ERROR_ELEMENT), p.start as u32);
        for (&token, edges) in types.iter().zip(starts.windows(2)) {
            if edges[0] < edges[1] {
                data.add_token_leaf(error, token, base + edges[0], base + edges[1]);
            }
        }
        data.set_end(error, p.end as u32);
        data.set_error(error, Cow::Borrowed(TOO_DEEPLY_NESTED));
        data.set_end(p.node, p.end as u32);
    }
}

impl<L: Language> Tree<L> {
    /// Parses `text` as a file of `file_kind` (Kotlin: a `.kt` file or a script). Any bytes parse; invalid UTF-8 lexes
    /// as `BAD_CHARACTER`s. At most `u32::MAX - 1` bytes are parsed. A fresh `Parser` per call: to parse many files,
    /// keep a `Parser` and call `Parser::parse`.
    pub fn parse(text: impl AsRef<[u8]>, file_kind: L::FileKind) -> Tree<L> {
        Parser::once().parse(text, file_kind)
    }

    /// `Parser::parse_body` on a fresh parser, every body inside expanded.
    pub fn parse_body(&self, id: NodeId) -> Option<Tree<L>> {
        Parser::once().parse_body(self, id, ParseOptions::default())
    }
}

impl<L: TreeLanguage> Tree<L> {
    /// The parsed text.
    pub fn source(&self) -> &[u8] {
        &self.text
    }

    pub fn file_kind(&self) -> L::FileKind {
        self.file_kind
    }

    /// The file node (Kotlin's `KtFile`).
    pub fn root(&self) -> NodeId {
        NodeId(self.root)
    }

    /// The number of nodes. Ids are `0..len()` in document order (preorder).
    pub fn len(&self) -> usize {
        self.data.kinds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.kinds.is_empty()
    }

    /// The heap bytes this tree holds: its copy of the text, the node columns and the error list with their owned
    /// messages, by capacity. What a consumer pays to keep the tree; `ksdump --bench` reports it per source MB.
    pub fn heap_bytes(&self) -> usize {
        let d = &self.data;
        let messages: usize = d
            .errors
            .iter()
            .map(|(_, m)| match m {
                Cow::Borrowed(_) => 0,
                Cow::Owned(s) => s.capacity(),
            })
            .sum();
        self.text.len()
            + d.kinds.capacity() * size_of::<u16>()
            + (d.ranges.capacity() + d.links.capacity()) * size_of::<[u32; 2]>()
            + d.errors.capacity() * size_of::<(u32, Cow<'static, str>)>()
            + d.body_depths.capacity() * size_of::<(u32, u32)>()
            + messages
    }

    /// The element type: a node type for a composite, a token type for a leaf. An expanded chameleon token (Kotlin's
    /// doc comment, `DOC_COMMENT`, "KDoc", and markdown link, `KDOC_MARKDOWN_LINK`) is a composite with a token type,
    /// as its PSI is.
    #[inline]
    pub fn kind(&self, id: NodeId) -> ElementTypeOf<L> {
        let index = (self.data.kinds[id.0 as usize] & KIND_INDEX) as usize;
        ElementType::from_index(index).unwrap_or(ElementType::Node(L::ERROR_ELEMENT))
    }

    /// The token type of a leaf.
    pub fn token_kind(&self, id: NodeId) -> Option<L::Token> {
        if self.is_leaf(id) { self.kind(id).token() } else { None }
    }

    /// The node type of a composite.
    pub fn node_kind(&self, id: NodeId) -> Option<L::Node> {
        self.kind(id).node()
    }

    /// Whether the node is a body (Kotlin's block or lambda) that `ParseOptions { expand_bodies: false }` left
    /// unexpanded: it has no children, `parse_body` gives them.
    pub fn is_unexpanded(&self, id: NodeId) -> bool {
        self.data.kinds[id.0 as usize] & UNEXPANDED != 0
    }

    /// Whether the node is a leaf (a token; it has no children).
    #[inline]
    pub fn is_leaf(&self, id: NodeId) -> bool {
        self.data.kinds[id.0 as usize] & LEAF != 0
    }

    /// Whether the node belongs to the token before it (Roslyn's trailing trivia of the token before it, structured
    /// or not), not to the one after it. Always false for Kotlin and Java.
    #[inline]
    pub fn is_trailing(&self, id: NodeId) -> bool {
        self.data.kinds[id.0 as usize] & TRAILING != 0
    }

    /// Whether the leaf is a token the parser inserted where one was expected (Roslyn's `IsMissing`); its range is
    /// empty. Always false for Kotlin and Java.
    #[inline]
    pub fn is_missing(&self, id: NodeId) -> bool {
        self.data.kinds[id.0 as usize] & MISSING != 0
    }

    /// The byte range in `source()`.
    #[inline]
    pub fn range(&self, id: NodeId) -> Range<usize> {
        let i = id.0 as usize;
        let [start, end] = self.data.ranges[i];
        start as usize..end as usize
    }

    /// The node's text.
    pub fn text(&self, id: NodeId) -> &[u8] {
        &self.text[self.range(id)]
    }

    /// The node's text as a string, None when it is not UTF-8.
    pub fn utf8_text(&self, id: NodeId) -> Option<&str> {
        std::str::from_utf8(self.text(id)).ok()
    }

    /// A declaration's own range, without what the language's binders put inside it at its edges: the doc comment and
    /// comments before its first token and after its last one (`is_trivia`), and a separator its last child may be
    /// (`separator`: Kotlin's enum entry `A,`, Java's enum constant). From its first token to its last; an empty
    /// element (an error marked at a position) is none of its text. A node with no such token is its whole range.
    pub fn own_range(
        &self,
        id: NodeId,
        is_trivia: impl Fn(NodeId) -> bool,
        separator: impl Fn(L::Token) -> bool,
    ) -> Range<usize> {
        match (self.own_edge(id, false, &is_trivia, &separator), self.own_edge(id, true, &is_trivia, &separator)) {
            (Some(first), Some(last)) => self.range(first).start..self.range(last).end,
            (Some(first), None) => self.range(first),
            _ => self.range(id),
        }
    }

    /// `own_range(id, is_trivia, separator).end`, from the descent to the last token alone: the descent to the first
    /// one takes a level per node on the left edge, so a node over a left-deep chain (C#'s `a.f().f()…`, 100 000 calls
    /// each a level below the next) costs its depth there, and the end of every call of the chain the square of it.
    pub fn own_end(
        &self,
        id: NodeId,
        is_trivia: impl Fn(NodeId) -> bool,
        separator: impl Fn(L::Token) -> bool,
    ) -> usize {
        // `own_range` ends at its last token, else at the end of its first one, else at the node's end
        match self
            .own_edge(id, true, &is_trivia, &separator)
            .or_else(|| self.own_edge(id, false, &is_trivia, &separator))
        {
            Some(token) => self.range(token).end,
            None => self.range(id).end,
        }
    }

    /// The first (or `last`) token of `own_range`: from `id` down, at each level the first (last) child that is neither
    /// trivia nor empty, at the top no separator either; None when `id` has no such child.
    fn own_edge(
        &self,
        id: NodeId,
        last: bool,
        is_trivia: &impl Fn(NodeId) -> bool,
        separator: &impl Fn(L::Token) -> bool,
    ) -> Option<NodeId> {
        let edge = |n: NodeId, top: bool| -> Option<NodeId> {
            let mut kids = self.children(n).filter(|&c| !is_trivia(c) && !self.range(c).is_empty());
            if last { kids.filter(|&c| !top || !self.token_kind(c).is_some_and(separator)).last() } else { kids.next() }
        };
        let mut n = edge(id, true)?;
        while let Some(c) = edge(n, false) {
            n = c;
        }
        Some(n)
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        let parent = self.data.links[id.0 as usize][0];
        (parent != NONE).then_some(NodeId(parent))
    }

    /// The end of the subtree at `id`: its nodes are the ids `id..subtree_end(id)`.
    #[inline]
    pub fn subtree_end(&self, id: NodeId) -> NodeId {
        NodeId(self.data.links[id.0 as usize][1])
    }

    pub fn first_child(&self, id: NodeId) -> Option<NodeId> {
        (self.data.links[id.0 as usize][1] > id.0 + 1).then_some(NodeId(id.0 + 1))
    }

    /// The last child: the ancestor of the subtree's last node that is a child of `id` (a walk up from it).
    pub fn last_child(&self, id: NodeId) -> Option<NodeId> {
        let end = self.data.links[id.0 as usize][1];
        if end <= id.0 + 1 {
            return None;
        }
        let mut child = end - 1;
        loop {
            let parent = self.data.links[child as usize][0];
            if parent == id.0 {
                return Some(NodeId(child));
            }
            child = parent;
        }
    }

    pub fn next_sibling(&self, id: NodeId) -> Option<NodeId> {
        let parent = self.data.links[id.0 as usize][0];
        if parent == NONE {
            return None;
        }
        let next = self.data.links[id.0 as usize][1];
        (next < self.data.links[parent as usize][1]).then_some(NodeId(next))
    }

    /// The sibling before `id`, None for a first child and the root: the child of `id`'s parent whose subtree holds the
    /// id just before `id` (ids are preorder), found by a climb from that id, so a step costs the depth of the previous
    /// sibling's last node and never a scan of the children before it.
    pub fn previous_sibling(&self, id: NodeId) -> Option<NodeId> {
        let parent = self.parent(id)?;
        let mut n = NodeId(id.0 - 1);
        while n != parent {
            let up = self.parent(n)?;
            if up == parent {
                return Some(n);
            }
            n = up;
        }
        None
    }

    /// The innermost of `id` and its ancestors that `accept` takes, `id` first, the root last.
    pub fn ancestor_or_self(&self, id: NodeId, accept: impl Fn(NodeId) -> bool) -> Option<NodeId> {
        let mut n = id;
        loop {
            if accept(n) {
                return Some(n);
            }
            n = self.parent(n)?;
        }
    }

    /// The deepest node whose range holds `range`: the last such in preorder, so an empty range between two nodes is
    /// held by the one that starts there. None for a range past the text.
    pub fn deepest_covering(&self, range: Range<usize>) -> Option<NodeId> {
        self.descendants(self.root()).rev().find(|&n| {
            let r = self.range(n);
            r.start <= range.start && range.end <= r.end
        })
    }

    /// The children in order.
    pub fn children(&self, id: NodeId) -> Children<'_> {
        Children { links: &self.data.links, next: id.0 + 1, end: self.data.links[id.0 as usize][1] }
    }

    /// The first child of the given kind.
    pub fn first_child_of_kind(&self, id: NodeId, kind: impl Into<ElementTypeOf<L>>) -> Option<NodeId> {
        let kind = kind.into();
        self.children(id).find(|&c| self.kind(c) == kind)
    }

    /// The message of an error element (`PsiErrorElement.getErrorDescription()`).
    pub fn error_message(&self, id: NodeId) -> Option<&str> {
        let i = self.data.errors.binary_search_by_key(&id.0, |(node, _)| *node).ok()?;
        Some(&self.data.errors[i].1)
    }

    /// The error elements in document order, each with its message: a scan of the error list, not of the tree.
    pub fn errors(&self) -> impl ExactSizeIterator<Item = (NodeId, &str)> + '_ {
        self.data.errors.iter().map(|(node, message)| (NodeId(*node), message.as_ref()))
    }

    /// The nodes of the subtree at `id` in document order (preorder), with their depth below `id`: a scan of the ids
    /// `id..subtree_end(id)`.
    pub fn preorder(&self, id: NodeId) -> Preorder<'_> {
        Preorder { links: &self.data.links, root: id.0, next: id.0, end: self.data.links[id.0 as usize][1], depth: 0 }
    }

    /// The leaves (tokens, whitespace, comments) in document order.
    pub fn tokens(&self) -> impl Iterator<Item = NodeId> + '_ {
        (0..self.len() as u32).map(NodeId).filter(|&id| self.is_leaf(id))
    }

    /// The subtree at `id`, `id` first, in document order: a scan of the ids `id..subtree_end(id)`, so "every call
    /// under X" is a pass over one column (`descendants(x).filter(|&n| tree.node_kind(n) == Some(CALL_EXPRESSION))`).
    pub fn descendants(&self, id: NodeId) -> impl DoubleEndedIterator<Item = NodeId> + ExactSizeIterator + use<L> {
        (id.0..self.data.links[id.0 as usize][1]).map(NodeId)
    }

    /// `descendants(id)` without the subtree of each node below `id` that `prune` takes, that node included: a
    /// declaration's own nodes without its nested declarations, a type's names without its arguments.
    pub fn descendants_pruned<'t>(
        &'t self,
        id: NodeId,
        prune: impl Fn(NodeId) -> bool + 't,
    ) -> impl Iterator<Item = NodeId> + 't {
        let end = self.data.links[id.0 as usize][1];
        let mut next = id.0;
        std::iter::from_fn(move || {
            while next < end {
                let n = NodeId(next);
                if n != id && prune(n) {
                    next = self.data.links[next as usize][1];
                    continue;
                }
                next += 1;
                return Some(n);
            }
            None
        })
    }

    /// The leaves in document order, indexed for the leaf at an offset.
    pub fn leaf_index(&self) -> LeafIndex<'_, L> {
        LeafIndex { tree: self, leaves: self.tokens().collect() }
    }

    /// Each token leaf's type and byte range, in document order: a scan of the columns. Their ranges tile the text,
    /// but for the ranges of unexpanded bodies. This, not the lexer, is what exact highlighting
    /// reads: the parser remaps soft keywords (`IDENTIFIER` to `by`, `get`, `data`, ...) and joins `?.`, `?:` and `!!`
    /// from two lexer tokens, and both show only in the tree's leaves.
    pub fn leaves(&self) -> impl Iterator<Item = (L::Token, Range<usize>)> + '_ {
        let d = &self.data;
        d.kinds.iter().enumerate().filter_map(move |(i, &k)| {
            if k & LEAF == 0 {
                return None;
            }
            let token = ElementTypeOf::<L>::from_index((k & KIND_INDEX) as usize)?.token()?;
            Some((token, d.ranges[i][0] as usize..d.ranges[i][1] as usize))
        })
    }
}

/// Builds a `Tree` in preorder for a language whose parser does not run on `PsiBuilderImpl` (C#'s, which builds
/// Roslyn's green nodes and flattens them): each node is opened before its descendants and closed after them, leaves
/// in document order. The builder writes the tree's three columns in place, so `finish` moves them into the tree
/// (shrunk to fit) instead of copying a buffer of build records out; the next tree starts with empty columns. A
/// builder that knows each node's id when it meets it writes them by id (`writer`), without a length or capacity
/// update per node.
pub struct TreeBuilder<L: TreeLanguage> {
    /// The columns of the tree being built: a node's kind, start and parent when it is opened, its end and subtree end
    /// when it is closed.
    kinds: Vec<u16>,
    ranges: Vec<[u32; 2]>,
    links: Vec<[u32; 2]>,
    /// The message of each error node, sorted by node id.
    errors: Vec<(u32, Cow<'static, str>)>,
    language: PhantomData<fn() -> L>,
}

impl<L: TreeLanguage> Default for TreeBuilder<L> {
    fn default() -> Self {
        TreeBuilder {
            kinds: Vec::new(),
            ranges: Vec::new(),
            links: Vec::new(),
            errors: Vec::new(),
            language: PhantomData,
        }
    }
}

impl<L: TreeLanguage> TreeBuilder<L> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Room for `additional` more nodes.
    pub fn reserve(&mut self, additional: usize) {
        self.kinds.reserve(additional);
        self.ranges.reserve(additional);
        self.links.reserve(additional);
    }

    /// The heap bytes of the builder's buffers, by capacity: the columns of a tree being built (`finish` hands them to
    /// the tree, so between trees they hold nothing).
    pub fn heap_bytes(&self) -> usize {
        use crate::buffers::vec_bytes;
        vec_bytes(&self.kinds) + vec_bytes(&self.ranges) + vec_bytes(&self.links) + vec_bytes(&self.errors)
    }

    /// Empties the builder, each buffer shrunk to its share of `shrink` (a language parser's `shrink_to`).
    pub fn shrink(&mut self, shrink: crate::buffers::Shrink) {
        shrink.vec(&mut self.kinds);
        shrink.vec(&mut self.ranges);
        shrink.vec(&mut self.links);
        shrink.vec(&mut self.errors);
    }

    /// The number of nodes so far; the next node's id.
    #[inline(always)]
    pub fn len(&self) -> u32 {
        self.kinds.len() as u32
    }

    pub fn is_empty(&self) -> bool {
        self.kinds.is_empty()
    }

    #[inline(always)]
    fn push(&mut self, parent: u32, kind: u16, start: u32, end: u32) -> u32 {
        let id = self.kinds.len() as u32;
        self.kinds.push(kind);
        self.ranges.push([start, end]);
        self.links.push([parent, id + 1]);
        id
    }

    /// Opens a composite of `kind` starting at `start`, a child of `parent` (`None`: the root, the first node).
    #[inline]
    pub fn open(&mut self, parent: Option<NodeId>, kind: ElementTypeOf<L>, start: u32) -> NodeId {
        NodeId(self.push(parent.map_or(NONE, |p| p.0), kind.index() as u16, start, start))
    }

    /// Closes a composite opened with `open`: it ends at byte `end`, its subtree at the last node added.
    #[inline]
    pub fn close(&mut self, node: NodeId, end: u32) {
        let len = self.kinds.len() as u32;
        let r = &mut self.ranges[node.0 as usize];
        r[1] = end.max(r[0]);
        self.links[node.0 as usize][1] = len;
    }

    /// A leaf of `token` over `start..end`, a child of `parent`.
    #[inline]
    pub fn leaf(&mut self, parent: NodeId, token: L::Token, start: u32, end: u32) -> NodeId {
        NodeId(self.push(parent.0, token.index() | LEAF, start, end))
    }

    /// `leaf` and then `set_trailing` when `trailing`, `set_missing` when `missing`, in one push.
    #[inline]
    pub fn leaf_with(
        &mut self,
        parent: NodeId,
        token: L::Token,
        start: u32,
        end: u32,
        trailing: bool,
        missing: bool,
    ) -> NodeId {
        NodeId(self.push(parent.0, token.index() | leaf_bits(trailing, missing), start, end))
    }

    /// Marks a node added so far as belonging to the token before it (`Tree::is_trailing`).
    #[inline]
    pub fn set_trailing(&mut self, node: NodeId) {
        self.kinds[node.0 as usize] |= TRAILING;
    }

    /// Marks a leaf added so far as a missing token (`Tree::is_missing`).
    #[inline]
    pub fn set_missing(&mut self, node: NodeId) {
        debug_assert!(self.kinds[node.0 as usize] & LEAF != 0);
        self.kinds[node.0 as usize] |= MISSING;
    }

    /// The message `Tree::error_message` gives for `node`. Nodes must get their messages in id order, one each.
    pub fn set_error(&mut self, node: NodeId, message: Cow<'static, str>) {
        debug_assert!(self.errors.last().is_none_or(|&(last, _)| last < node.0));
        self.errors.push((node.0, message));
    }

    /// A writer of the nodes `len()..len() + capacity` by id (`ColumnWriter`): the columns grow to `len() + capacity`
    /// nodes, which `truncate` cuts back to the nodes written. Writing a node by id updates no length, so a walk keeps
    /// its whole state in registers; a write past the columns is dropped, and the walk that made it can tell from the
    /// ids it gave out.
    pub fn writer(&mut self, capacity: usize) -> ColumnWriter<'_, L> {
        let len = self.kinds.len() + capacity;
        self.kinds.resize(len, 0);
        self.ranges.resize(len, [0, 0]);
        self.links.resize(len, [0, 0]);
        ColumnWriter {
            kinds: &mut self.kinds,
            ranges: &mut self.ranges,
            links: &mut self.links,
            errors: &mut self.errors,
            language: PhantomData,
        }
    }

    /// Keeps the first `len` nodes (after a `writer`: the ones it wrote) and their error messages.
    pub fn truncate(&mut self, len: u32) {
        let errors = self.errors.partition_point(|&(id, _)| id < len);
        self.errors.truncate(errors);
        self.kinds.truncate(len as usize);
        self.ranges.truncate(len as usize);
        self.links.truncate(len as usize);
    }

    /// The tree of the nodes added since the last `finish`, over `text` (whose bytes their ranges index), its root
    /// node 0: the columns themselves, shrunk to fit. The builder is empty again.
    pub fn finish(&mut self, text: Arc<[u8]>, file_kind: L::FileKind) -> Tree<L> {
        debug_assert!(!self.kinds.is_empty(), "a tree has a root");
        let mut kinds = std::mem::take(&mut self.kinds);
        let mut ranges = std::mem::take(&mut self.ranges);
        let mut links = std::mem::take(&mut self.links);
        let mut errors = std::mem::take(&mut self.errors);
        kinds.shrink_to_fit();
        ranges.shrink_to_fit();
        links.shrink_to_fit();
        errors.shrink_to_fit();
        let data = TreeData { kinds, ranges, links, errors, body_depths: Vec::new(), nodes: Vec::new() };
        Tree { text, file_kind, data, root: 0, language: PhantomData }
    }
}

/// The `kinds` bits of a leaf, `Tree::is_trailing` and `Tree::is_missing` as given.
#[inline(always)]
fn leaf_bits(trailing: bool, missing: bool) -> u16 {
    LEAF | if trailing { TRAILING } else { 0 } | if missing { MISSING } else { 0 }
}

/// Writes a tree's nodes by id into the columns a `TreeBuilder` sized (`TreeBuilder::writer`): a composite when it is
/// opened (`open`) and again when it is closed (`close`), a leaf once. A write past the columns is dropped.
pub struct ColumnWriter<'a, L: TreeLanguage> {
    kinds: &'a mut [u16],
    ranges: &'a mut [[u32; 2]],
    links: &'a mut [[u32; 2]],
    errors: &'a mut Vec<(u32, Cow<'static, str>)>,
    language: PhantomData<fn() -> L>,
}

impl<L: TreeLanguage> ColumnWriter<'_, L> {
    /// The number of nodes the columns hold.
    #[inline(always)]
    pub fn capacity(&self) -> u32 {
        self.kinds.len() as u32
    }

    #[inline(always)]
    fn write(&mut self, id: u32, kind: u16, start: u32, end: u32, parent: u32) {
        let i = id as usize;
        if let (Some(k), Some(r), Some(l)) = (self.kinds.get_mut(i), self.ranges.get_mut(i), self.links.get_mut(i)) {
            *k = kind;
            *r = [start, end];
            *l = [parent, id + 1];
        }
    }

    /// The composite `id` of `kind` starting at `start`, a child of `parent` (`None`: the root), `Tree::is_trailing`
    /// when `trailing`; `close` ends it.
    #[inline(always)]
    pub fn open(&mut self, id: u32, parent: Option<NodeId>, kind: ElementTypeOf<L>, start: u32, trailing: bool) {
        let bits = if trailing { TRAILING } else { 0 };
        self.write(id, kind.index() as u16 | bits, start, start, parent.map_or(NONE, |p| p.0));
    }

    /// Closes the composite `id`: it ends at byte `end` (at least its start), its subtree before node `subtree_end`.
    #[inline(always)]
    pub fn close(&mut self, id: u32, end: u32, subtree_end: u32) {
        let i = id as usize;
        if let (Some(r), Some(l)) = (self.ranges.get_mut(i), self.links.get_mut(i)) {
            r[1] = end.max(r[0]);
            l[1] = subtree_end;
        }
    }

    /// The leaf `id` of `token` over `start..end`, a child of `parent`, `Tree::is_trailing` and `Tree::is_missing` as
    /// given.
    #[inline(always)]
    #[allow(clippy::too_many_arguments)]
    pub fn leaf(
        &mut self,
        id: u32,
        parent: NodeId,
        token: L::Token,
        start: u32,
        end: u32,
        trailing: bool,
        missing: bool,
    ) {
        self.write(id, token.index() | leaf_bits(trailing, missing), start, end, parent.0);
    }

    /// `TreeBuilder::set_error`.
    pub fn set_error(&mut self, node: NodeId, message: Cow<'static, str>) {
        debug_assert!(self.errors.last().is_none_or(|&(last, _)| last < node.0));
        self.errors.push((node.0, message));
    }
}

/// The leaves of a tree (tokens, whitespace, comments) by offset (`Tree::leaf_index`). The leaves of a tree are in
/// offset order and tile its text, but for the ranges of unexpanded bodies, which hold none.
pub struct LeafIndex<'t, L: TreeLanguage> {
    tree: &'t Tree<L>,
    leaves: Vec<NodeId>,
}

impl<L: TreeLanguage> LeafIndex<'_, L> {
    /// The leaf whose range holds the byte at `offset` (a binary search), None past the text and inside an unexpanded
    /// body. An empty leaf holds no byte.
    pub fn at(&self, offset: usize) -> Option<NodeId> {
        let i = self.leaves.partition_point(|&leaf| self.tree.range(leaf).end <= offset);
        self.leaves.get(i).copied().filter(|&leaf| self.tree.range(leaf).start <= offset)
    }
}

/// The children of a node.
pub struct Children<'t> {
    links: &'t [[u32; 2]],
    next: u32,
    /// The parent's subtree end.
    end: u32,
}

impl Iterator for Children<'_> {
    type Item = NodeId;

    #[inline]
    fn next(&mut self) -> Option<NodeId> {
        if self.next >= self.end {
            return None;
        }
        let id = self.next;
        self.next = self.links[id as usize][1];
        Some(NodeId(id))
    }
}

/// A preorder walk of a subtree: `(node, depth below the subtree root)`, a scan of its ids; the depth follows from the
/// parents (a node's parent is the node before it or one of that node's ancestors).
pub struct Preorder<'t> {
    links: &'t [[u32; 2]],
    root: u32,
    next: u32,
    end: u32,
    depth: usize,
}

impl Iterator for Preorder<'_> {
    type Item = (NodeId, usize);

    #[inline]
    fn next(&mut self) -> Option<(NodeId, usize)> {
        let id = self.next;
        if id >= self.end {
            return None;
        }
        if id != self.root {
            // climb from the node before to this node's parent
            let parent = self.links[id as usize][0];
            let mut prev = id - 1;
            let mut depth = self.depth;
            while prev != parent && prev != NONE {
                prev = self.links[prev as usize][0];
                depth = depth.saturating_sub(1);
            }
            self.depth = depth + 1;
        }
        self.next = id + 1;
        Some((NodeId(id), self.depth))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = (self.end - self.next.min(self.end)) as usize;
        (n, Some(n))
    }
}

/// The file's own tokens for a chameleon when lexing its text alone gives the same tokens: it starts at a token
/// lexed in the initial state with an empty state stack, ends at a token boundary where the lexer is back in that
/// state (or at the end of the file), and its last token is not empty. An empty last token is one the lexer emitted
/// by looking past the chameleon: `DANGLING_NEWLINE` of an unclosed string before a line break, which the text alone,
/// ending at the string, does not lex (`{a + "` then a newline: the fresh lexing ends in the string, so the string's
/// `Expecting '"'` sits at the end and the block's `Expecting '}'` at the same lexeme is dropped). The types are the
/// lexer's, before any remapping by the outer parse, as a fresh lexing would give them.
/// Returns their index range `first..last` in the file's tokens.
fn reusable_tokens<T, K>(file_tokens: &Lexed<T>, p: &Pending<K>) -> Option<(usize, usize)> {
    let (first, last) = p.file_tokens?;
    if !file_tokens.is_plain(first)
        || last > file_tokens.len()
        || (last < file_tokens.len() && !file_tokens.is_plain(last))
        || file_tokens.starts.get(first).copied() != Some(p.start as u32)
        || file_tokens.starts.get(last).copied() != Some(p.end as u32)
        || (last > first && file_tokens.starts.get(last - 1).copied() == Some(p.end as u32))
    {
        return None;
    }
    Some((first, last))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A language past Kotlin's and Java's bounds: 300 token types, so node types start at 1024.
    #[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
    struct Tok(u16);
    #[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
    struct Node(u16);
    struct Toy;

    impl TokenType for Tok {
        const NODE_BASE: usize = 1024;
        fn index(self) -> u16 {
            self.0
        }
        fn from_index(index: usize) -> Option<Tok> {
            (index < 300).then_some(Tok(index as u16))
        }
        fn debug_name(self) -> &'static str {
            "tok"
        }
    }

    impl NodeType for Node {
        fn index(self) -> u16 {
            self.0
        }
        fn from_index(index: usize) -> Option<Node> {
            (index < 400).then_some(Node(index as u16))
        }
        fn debug_name(self) -> &'static str {
            "node"
        }
    }

    impl TreeLanguage for Toy {
        type Token = Tok;
        type Node = Node;
        type FileKind = ();
        const ERROR_ELEMENT: Node = Node(0);
    }

    #[test]
    fn tree_builder_builds_preorder_with_wide_kinds_and_trailing_leaves() {
        let text: Arc<[u8]> = Arc::from(&b"a b;"[..]);
        let mut b = TreeBuilder::<Toy>::new();
        for _ in 0..2 {
            let root = b.open(None, ElementType::Node(Node(399)), 0);
            let inner = b.open(Some(root), ElementType::Node(Node(7)), 0);
            b.leaf(inner, Tok(299), 0, 1);
            let space = b.leaf(inner, Tok(3), 1, 2);
            b.set_trailing(space);
            b.close(inner, 2);
            b.leaf(root, Tok(256), 2, 3);
            let semi = b.leaf(root, Tok(4), 3, 4);
            let missing = b.leaf(root, Tok(4), 4, 4);
            b.set_missing(missing);
            b.set_error(semi, Cow::Borrowed("expected"));
            b.close(root, 4);
            let tree = b.finish(text.clone(), ());
            assert!(b.is_empty());
            assert_eq!(tree.len(), 7);
            assert!(tree.is_missing(NodeId(6)) && !tree.is_missing(NodeId(5)));
            assert_eq!(tree.kind(NodeId(0)), ElementType::Node(Node(399)));
            assert_eq!(tree.node_kind(NodeId(1)), Some(Node(7)));
            assert_eq!(tree.token_kind(NodeId(2)), Some(Tok(299)));
            assert!(!tree.is_trailing(NodeId(2)) && tree.is_trailing(NodeId(3)));
            assert_eq!(tree.token_kind(NodeId(4)), Some(Tok(256)));
            assert_eq!(tree.children(NodeId(0)).collect::<Vec<_>>(), [NodeId(1), NodeId(4), NodeId(5), NodeId(6)]);
            assert_eq!(tree.range(NodeId(1)), 0..2);
            assert_eq!(tree.parent(NodeId(3)), Some(NodeId(1)));
            assert_eq!(tree.error_message(NodeId(5)), Some("expected"));
            let leaves: Vec<_> = tree.leaves().map(|(t, r)| (t.0, r)).collect();
            assert_eq!(leaves, [(299, 0..1), (3, 1..2), (256, 2..3), (4, 3..4), (4, 4..4)]);
        }
    }

    const SPACE: Tok = Tok(3);
    const COMMA: Tok = Tok(4);
    const COMMENT: Tok = Tok(10);
    const NAME: Tok = Tok(20);
    const FILE: Node = Node(1);
    const DECL: Node = Node(2);
    const ARGS: Node = Node(3);

    /// `/*d*/ f(g) ,/*t*/;x`: a declaration `f(g)` with its comments and separator inside it, a nested declaration `g`,
    /// an empty error element, an error element over `;`.
    fn reading_tree() -> Tree<Toy> {
        let text: Arc<[u8]> = Arc::from(&b"/*d*/ f(g) ,/*t*/;x"[..]);
        let mut b = TreeBuilder::<Toy>::new();
        let root = b.open(None, ElementType::Node(FILE), 0); // 0
        let decl = b.open(Some(root), ElementType::Node(DECL), 0); // 1
        b.leaf(decl, COMMENT, 0, 5); // 2
        b.leaf(decl, SPACE, 5, 6); // 3
        b.leaf(decl, NAME, 6, 7); // 4
        let args = b.open(Some(decl), ElementType::Node(ARGS), 7); // 5
        b.leaf(args, Tok(7), 7, 8); // 6
        let inner = b.open(Some(args), ElementType::Node(DECL), 8); // 7
        b.leaf(inner, NAME, 8, 9); // 8
        b.close(inner, 9);
        b.leaf(args, Tok(8), 9, 10); // 9
        b.close(args, 10);
        b.leaf(decl, SPACE, 10, 11); // 10
        b.leaf(decl, COMMA, 11, 12); // 11
        let empty = b.open(Some(decl), ElementType::Node(Node(0)), 12); // 12
        b.close(empty, 12);
        b.set_error(empty, Cow::Borrowed("empty"));
        b.leaf(decl, COMMENT, 12, 17); // 13
        b.close(decl, 17);
        let error = b.open(Some(root), ElementType::Node(Node(0)), 17); // 14
        b.leaf(error, Tok(5), 17, 18); // 15
        b.close(error, 18);
        b.set_error(error, Cow::Borrowed("expected"));
        b.leaf(root, NAME, 18, 19); // 16
        b.close(root, 19);
        b.finish(text, ())
    }

    #[test]
    fn previous_sibling_climbs_from_the_id_before() {
        let tree = reading_tree();
        let previous = |i: u32| tree.previous_sibling(NodeId(i)).map(|n| n.0);
        assert_eq!(
            [0, 1, 14, 16, 5, 10, 7, 9, 2].map(previous),
            [None, None, Some(1), Some(14), Some(4), Some(5), Some(6), Some(7), None]
        );
        for i in 0..tree.len() as u32 {
            if let Some(next) = tree.next_sibling(NodeId(i)) {
                assert_eq!(tree.previous_sibling(next), Some(NodeId(i)));
            }
        }
    }

    #[test]
    fn own_range_leaves_out_the_trivia_the_separator_and_empty_elements() {
        let tree = reading_tree();
        let trivia = |n: NodeId| matches!(tree.token_kind(n), Some(SPACE | COMMENT));
        assert_eq!(tree.own_range(NodeId(1), trivia, |t| t == COMMA), 6..10);
        assert_eq!(tree.own_range(NodeId(1), trivia, |_| false), 6..12);
        assert_eq!(tree.own_range(NodeId(4), trivia, |t| t == COMMA), 6..7);
        assert_eq!(tree.own_range(NodeId(12), trivia, |t| t == COMMA), 12..12);
        assert_eq!(tree.utf8_text(NodeId(5)), Some("(g)"));
    }

    #[test]
    fn descendants_pruned_skips_the_subtrees_it_takes() {
        let tree = reading_tree();
        let own: Vec<u32> =
            tree.descendants_pruned(NodeId(1), |n| tree.node_kind(n) == Some(DECL)).map(|n| n.0).collect();
        assert_eq!(own, [1, 2, 3, 4, 5, 6, 9, 10, 11, 12, 13]);
        let names: Vec<u32> = tree
            .descendants_pruned(NodeId(1), |n| tree.node_kind(n) == Some(ARGS))
            .filter(|&n| tree.token_kind(n) == Some(NAME))
            .map(|n| n.0)
            .collect();
        assert_eq!(names, [4]);
        let all: Vec<u32> = tree.descendants_pruned(NodeId(7), |_| true).map(|n| n.0).collect();
        assert_eq!(all, [7]);
    }

    #[test]
    fn covering_ancestors_errors_and_leaves_by_offset() {
        let tree = reading_tree();
        let covering = |r: Range<usize>| tree.deepest_covering(r).map(|n| n.0);
        assert_eq!(
            [8..9, 6..10, 7..10, 12..12, 19..19, 0..19, 20..20].map(covering),
            [Some(8), Some(1), Some(5), Some(13), Some(16), Some(0), None]
        );
        let around =
            |i: u32, kind: Node| tree.ancestor_or_self(NodeId(i), |n| tree.node_kind(n) == Some(kind)).map(|n| n.0);
        assert_eq!(
            [around(8, DECL), around(7, DECL), around(8, FILE), around(4, ARGS)],
            [Some(7), Some(7), Some(0), None]
        );
        let errors: Vec<(u32, &str)> = tree.errors().map(|(n, m)| (n.0, m)).collect();
        assert_eq!(errors, [(12, "empty"), (14, "expected")]);
        let leaves = tree.leaf_index();
        let at = |offset: usize| leaves.at(offset).map(|n| n.0);
        assert_eq!(
            [0, 4, 5, 6, 12, 17, 18, 19].map(at),
            [Some(2), Some(2), Some(3), Some(4), Some(13), Some(15), Some(16), None]
        );
    }
}
