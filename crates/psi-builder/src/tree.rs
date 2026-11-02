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
use crate::language::{ElementType, ElementTypeOf, Language, NodeType, TokenType};
use crate::lexer::Lexed;

/// A node of a `Tree`.
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct NodeId(pub u32);

const NONE: u32 = u32::MAX;

/// `kinds` bit of a leaf (a token; no children).
const LEAF: u16 = 0x8000;
/// `kinds` bit of a block or lambda left unexpanded (`ParseOptions::expand_bodies`).
const UNEXPANDED: u16 = 0x4000;
/// `kinds` bits of the element type's `ElementType::index`.
const KIND_INDEX: u16 = 0x3FFF;

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
        self.push(parent, token.index() as u16 | LEAF, start, end);
    }

    /// Closes a composite: its byte end, and its subtree ends at the last node appended.
    #[inline(always)]
    pub(crate) fn set_end(&mut self, node: u32, end: u32) {
        let len = self.nodes.len() as u32;
        let n = &mut self.nodes[node as usize];
        n.end = end.max(n.start);
        n.subtree_end = len;
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
pub struct Tree<L: Language> {
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
        builder.set_nesting(0, self.limit);
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
        delegate.set_nesting(self.nesting, self.limit);
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

    /// Parses with `limit` instead of `Language::NESTING_LIMIT` (tests: a small limit puts the too-deep paths on ordinary
    /// inputs). Not part of the consumer contract.
    #[doc(hidden)]
    pub fn set_nesting_limit(&mut self, limit: u32) {
        self.nesting_limit = Some(limit);
    }

    fn limit(&self) -> u32 {
        self.nesting_limit.unwrap_or(L::NESTING_LIMIT)
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
        self.begin_chameleon(&tree.text, &mut data, chameleon, tree.file_kind);
        self.bind_frames(&tree.text, &mut data, tree.file_kind);
        let body = data.take_exact();
        self.data = data;
        self.options = ParseOptions::default();
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
        file.set_nesting(0, limit);
        if L::BLOCK_BRACES.is_some() {
            file.share_brace_pairs(self.file_brace_pairs.clone(), 0);
        }
        parse(frame);
        let root = L::frame_builder(frame).begin_bind(&mut data, None);
        self.depth = 1;
        self.bind_frames(&text, &mut data, file_kind);

        let tree = data.take_exact();
        self.data = data;
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
    /// too deeply nested" over its tokens, and no frame is added.
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
            file_kind,
        };
        L::parse_chameleon(p.kind, &mut chameleon);
        L::frame_builder(&mut self.frames[self.depth]).begin_bind(data, Some(p.node));
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
        Parser::new().parse(text, file_kind)
    }

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

    /// `Parser::parse_body` on a fresh parser, every body inside expanded.
    pub fn parse_body(&self, id: NodeId) -> Option<Tree<L>> {
        Parser::new().parse_body(self, id, ParseOptions::default())
    }

    /// Whether the node is a leaf (a token; it has no children).
    #[inline]
    pub fn is_leaf(&self, id: NodeId) -> bool {
        self.data.kinds[id.0 as usize] & LEAF != 0
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
