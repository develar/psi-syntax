//! The lossless syntax tree: one arena of nodes with `u32` ids, each a kind and a byte range, children in order,
//! tokens as leaves; whitespace and comments are leaves where PSI puts them. `Tree::parse` lexes, parses and builds
//! it, expanding every lazy element as PSI does when the tree is walked: a doc comment with the KDoc lexer and parser,
//! a markdown link inside it with the Kotlin lexer and `KDocLinkParser`, a collapsed block or lambda by reparsing its
//! text with `KotlinParser.parseBlockExpression` / `parseLambdaExpression`, each in place where the tree meets it (a
//! stack of builders on the heap, not recursion), so node ids are in preorder.

use std::borrow::Cow;
use std::ops::Range;
use std::sync::Arc;

use crate::builder::{BuilderBuffers, LazyKind, Pending, PsiBuilderImpl, SemanticWhitespaceAwarePsiBuilderImpl};
use crate::lexer::{self, Lexed};
use crate::node_kind::NodeKind;
use crate::parsing::kotlin_parser;
use crate::tokens::{ElementType, TokenKind};

/// How a file is parsed: `KotlinParser.parse` runs `parseFile` for `.kt` and `parseScript` for anything else.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum FileKind {
    Kt,
    Kts,
}

impl FileKind {
    /// `FileUtilRt.getExtension(name)` empty or `kt` is a file, any other extension a script (case-sensitive).
    pub fn from_file_name(name: &str) -> FileKind {
        let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
        match base.rfind('.') {
            None => FileKind::Kt,
            Some(dot) => {
                let extension = &base[dot + 1..];
                if extension.is_empty() || extension == "kt" { FileKind::Kt } else { FileKind::Kts }
            }
        }
    }
}

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

/// The arena a builder appends to, one column per field, nodes in preorder: a node's subtree is the ids
/// `id..subtree_end[id]`, so its first child is `id + 1` when it has one and the next sibling of a child is its
/// `subtree_end` while that is inside the parent's.
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
}

impl TreeData {
    #[inline(always)]
    fn push(&mut self, parent: u32, kind: u16, start: u32, end: u32) -> u32 {
        let id = self.kinds.len() as u32;
        self.kinds.push(kind);
        self.ranges.push([start, end]);
        self.links.push([parent, id + 1]);
        id
    }

    /// A composite node starting at `start` as the next node, a child of `parent` (`u32::MAX`: a root); its end and
    /// its subtree's are set when it is done (`set_end`), after its descendants are appended.
    pub(crate) fn add_composite(&mut self, parent: u32, kind: ElementType, start: u32) -> u32 {
        self.push(parent, kind.index() as u16, start, start)
    }

    /// A leaf as the next node, a child of `parent`.
    pub(crate) fn add_leaf(&mut self, parent: u32, kind: ElementType, start: u32, end: u32) -> u32 {
        self.push(parent, kind.index() as u16 | LEAF, start, end)
    }

    /// A token leaf as the next node, a child of `parent`.
    #[inline(always)]
    pub(crate) fn add_token_leaf(&mut self, parent: u32, token: TokenKind, start: u32, end: u32) {
        self.push(parent, token as u16 | LEAF, start, end);
    }

    /// Closes a composite: its byte end, and its subtree ends at the last node appended.
    pub(crate) fn set_end(&mut self, node: u32, end: u32) {
        let n = node as usize;
        let range = &mut self.ranges[n];
        range[1] = end.max(range[0]);
        self.links[n][1] = self.kinds.len() as u32;
    }

    /// Marks a chameleon block or lambda as left unexpanded.
    fn set_unexpanded(&mut self, node: u32) {
        self.kinds[node as usize] |= UNEXPANDED;
    }

    /// Room for `additional` more nodes in every column.
    pub(crate) fn reserve(&mut self, additional: usize) {
        self.kinds.reserve(additional);
        self.ranges.reserve(additional);
        self.links.reserve(additional);
    }

    pub(crate) fn set_error(&mut self, node: u32, message: Cow<'static, str>) {
        self.errors.push((node, message));
    }

    fn clear(&mut self) {
        self.kinds.clear();
        self.ranges.clear();
        self.links.clear();
        self.errors.clear();
    }

    /// A copy in exactly sized buffers; the errors are taken (most files have none: their list never allocates; the
    /// others' is shrunk to fit).
    fn take_exact(&mut self) -> TreeData {
        let mut errors = std::mem::take(&mut self.errors);
        errors.shrink_to_fit();
        TreeData {
            kinds: self.kinds.as_slice().to_vec(),
            ranges: self.ranges.as_slice().to_vec(),
            links: self.links.as_slice().to_vec(),
            errors,
        }
    }
}

/// A parsed Kotlin file.
pub struct Tree {
    text: Arc<[u8]>,
    file_kind: FileKind,
    data: TreeData,
    root: u32,
}

/// How `Parser::parse_with_options` builds the tree.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct ParseOptions {
    /// Whether the bodies PSI parses lazily (a block, `BLOCK`, or a lambda, `LAMBDA_EXPRESSION`, that the parser
    /// skipped as a balanced `{...}`) are parsed and expanded. When false they stay childless nodes with their kind
    /// and range (`Tree::is_unexpanded`), and `Tree::parse_body` parses one on demand into exactly the subtree the
    /// full parse has there, because PSI too builds such a body from its text alone. Doc comments are expanded
    /// either way. True by default.
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
#[derive(Default)]
pub struct Parser {
    /// The file's tokens as the lexer made them (a chameleon reuses a slice of them).
    file: Lexed,
    /// A chameleon's text lexed afresh.
    relexed: Lexed,
    /// The builders still binding: the file's, then each chameleon's being expanded inside the one before it.
    frames: Vec<Frame>,
    /// The buffers of finished builders, for the next ones.
    spare: Vec<BuilderBuffers>,
    /// The tree being built; copied into exactly sized buffers when done.
    data: TreeData,
    options: ParseOptions,
}

/// A builder whose tree is being appended, with the buffers it was made from (its own are taken out of them).
struct Frame {
    builder: PsiBuilderImpl,
    bufs: BuilderBuffers,
}

impl Parser {
    pub fn new() -> Parser {
        Parser::default()
    }

    /// Parses `text` as a `.kt` file or a script (see `Tree::parse`).
    pub fn parse(&mut self, text: impl AsRef<[u8]>, file_kind: FileKind) -> Tree {
        self.parse_with_options(text, file_kind, ParseOptions::default())
    }

    /// `parse` with `options` (e.g. bodies left unexpanded for a pass that reads only declarations).
    pub fn parse_with_options(&mut self, text: impl AsRef<[u8]>, file_kind: FileKind, options: ParseOptions) -> Tree {
        self.options = options;
        let tree = self.parse_with(text.as_ref(), file_kind, |builder| kotlin_parser::parse(builder, file_kind));
        self.options = ParseOptions::default();
        tree
    }

    /// The subtree of the body `id` of `tree` that `ParseOptions { expand_bodies: false }` left unexpanded
    /// (`Tree::is_unexpanded`), as its own tree over the same source: its root is a node of the body's kind and
    /// range, its ids start at 0, its ranges are offsets in `tree.source()`. Bodies inside it are expanded as
    /// `options` says. `None` when `id` is not an unexpanded body.
    pub fn parse_body(&mut self, tree: &Tree, id: NodeId, options: ParseOptions) -> Option<Tree> {
        if !tree.is_unexpanded(id) {
            return None;
        }
        let kind = match tree.node_kind(id)? {
            NodeKind::BLOCK => LazyKind::Block,
            NodeKind::LAMBDA_EXPRESSION => LazyKind::Lambda,
            _ => return None,
        };
        let range = tree.range(id);
        self.options = options;
        let mut data = std::mem::take(&mut self.data);
        data.clear();
        let root = data.add_composite(NONE, tree.kind(id), range.start as u32);
        data.set_end(root, range.end as u32);
        let chameleon = Pending { node: root, kind, start: range.start, end: range.end, file_tokens: None };
        let frame = self.begin_chameleon(&tree.text, &mut data, chameleon);
        self.frames.push(frame);
        self.bind_frames(&tree.text, &mut data);
        let body = data.take_exact();
        self.data = data;
        self.options = ParseOptions::default();
        Some(Tree { text: tree.text.clone(), file_kind: tree.file_kind, data: body, root })
    }

    /// Lexes `text`, runs `parse` on its builder, builds and expands the tree.
    pub(crate) fn parse_with(
        &mut self,
        text: &[u8],
        file_kind: FileKind,
        parse: impl FnOnce(&mut SemanticWhitespaceAwarePsiBuilderImpl),
    ) -> Tree {
        let text: Arc<[u8]> = Arc::from(&text[..text.len().min(u32::MAX as usize - 1)]);
        lexer::tokenize_into(&text, &mut self.file);
        crate::counters::add(crate::counters::Counter::FileTokens, self.file.len() as u64);
        let mut data = std::mem::take(&mut self.data);
        data.clear();

        let mut bufs = self.spare.pop().unwrap_or_default();
        bufs.lex_types.clear();
        bufs.lex_types.extend_from_slice(&self.file.types);
        bufs.starts.clear();
        bufs.starts.extend_from_slice(&self.file.starts);
        let file = PsiBuilderImpl::new(text.clone(), 0, text.len(), &mut bufs, Some(0));
        let mut builder = SemanticWhitespaceAwarePsiBuilderImpl::new(file, &mut bufs);
        parse(&mut builder);
        let mut builder = builder.into_delegate(&mut bufs);
        let root = builder.begin_bind(&mut data, None);
        self.frames.push(Frame { builder, bufs });
        self.bind_frames(&text, &mut data);

        let tree = data.take_exact();
        self.data = data;
        Tree { text, file_kind, data: tree, root }
    }

    /// Runs the builders on `frames` to the end: the top one appends its nodes until it appends a chameleon, whose
    /// builder then goes on top and appends the chameleon's subtree right after it. So node ids are in preorder, and
    /// nesting takes heap, not stack.
    fn bind_frames(&mut self, text: &Arc<[u8]>, data: &mut TreeData) {
        while let Some(frame) = self.frames.last_mut() {
            match frame.builder.resume_bind(data) {
                Some(chameleon)
                    if !self.options.expand_bodies && matches!(chameleon.kind, LazyKind::Block | LazyKind::Lambda) =>
                {
                    data.set_unexpanded(chameleon.node);
                }
                Some(chameleon) => {
                    let frame = self.begin_chameleon(text, data, chameleon);
                    self.frames.push(frame);
                }
                None => {
                    let Some(Frame { builder, mut bufs }) = self.frames.pop() else { break };
                    builder.release(&mut bufs);
                    self.spare.push(bufs);
                }
            }
        }
    }

    /// Parses one chameleon's text with a new builder and starts appending its subtree under its node.
    fn begin_chameleon(&mut self, text: &Arc<[u8]>, data: &mut TreeData, p: Pending) -> Frame {
        use crate::counters::{Counter, add};
        let len = p.end - p.start;
        let mut bufs = self.spare.pop().unwrap_or_default();
        let mut builder = match p.kind {
            LazyKind::Block | LazyKind::Lambda => {
                add(if p.kind == LazyKind::Block { Counter::BlockChameleons } else { Counter::LambdaChameleons }, 1);
                let file_token = match reusable_tokens(&self.file, &p) {
                    Some((first, last)) => {
                        add(Counter::ReusedTokens, (last - first) as u64);
                        bufs.lex_types.clear();
                        bufs.lex_types.extend_from_slice(&self.file.types[first..last]);
                        bufs.starts.clear();
                        bufs.starts.extend(self.file.starts[first..=last].iter().map(|s| s - p.start as u32));
                        Some(first)
                    }
                    None => {
                        lexer::tokenize_into(&text[p.start..p.end], &mut self.relexed);
                        add(Counter::RelexedTokens, self.relexed.len() as u64);
                        bufs.lex_types.clear();
                        bufs.lex_types.extend_from_slice(&self.relexed.types);
                        bufs.starts.clear();
                        bufs.starts.extend_from_slice(&self.relexed.starts);
                        None
                    }
                };
                let delegate = PsiBuilderImpl::new(text.clone(), p.start, len, &mut bufs, file_token);
                let mut builder = SemanticWhitespaceAwarePsiBuilderImpl::new(delegate, &mut bufs);
                if p.kind == LazyKind::Block {
                    kotlin_parser::parse_block_expression(&mut builder);
                } else {
                    kotlin_parser::parse_lambda_expression(&mut builder);
                }
                builder.into_delegate(&mut bufs)
            }
            LazyKind::KDoc => {
                crate::kdoc::lexer::tokenize_into(&text[p.start..p.end], &mut bufs.lex_types, &mut bufs.starts);
                add(Counter::KDocChameleons, 1);
                add(Counter::KDocTokens, bufs.lex_types.len() as u64);
                let mut builder = PsiBuilderImpl::new(text.clone(), p.start, len, &mut bufs, None);
                crate::kdoc::parser::parse(ElementType::Token(TokenKind::DOC_COMMENT), &mut builder);
                builder
            }
            LazyKind::KDocLink => {
                lexer::tokenize_into(&text[p.start..p.end], &mut self.relexed);
                add(Counter::KDocLinkChameleons, 1);
                add(Counter::RelexedTokens, self.relexed.len() as u64);
                bufs.lex_types.clear();
                bufs.lex_types.extend_from_slice(&self.relexed.types);
                bufs.starts.clear();
                bufs.starts.extend_from_slice(&self.relexed.starts);
                let mut builder = PsiBuilderImpl::new(text.clone(), p.start, len, &mut bufs, None);
                crate::kdoc::parser::parse_markdown_link(
                    ElementType::Token(TokenKind::KDOC_MARKDOWN_LINK),
                    &mut builder,
                );
                builder
            }
        };
        builder.begin_bind(data, Some(p.node));
        Frame { builder, bufs }
    }
}

impl Tree {
    /// Parses `text` as a `.kt` file or a script. Any bytes parse; invalid UTF-8 lexes as `BAD_CHARACTER`s. At most
    /// `u32::MAX - 1` bytes are parsed. A fresh `Parser` per call: to parse many files, keep a `Parser` and call
    /// `Parser::parse`.
    pub fn parse(text: impl AsRef<[u8]>, file_kind: FileKind) -> Tree {
        Parser::new().parse(text, file_kind)
    }

    /// `Parser::parse_with` on a fresh parser.
    #[cfg(test)]
    pub(crate) fn parse_with(
        text: &[u8],
        file_kind: FileKind,
        parse: impl FnOnce(&mut SemanticWhitespaceAwarePsiBuilderImpl),
    ) -> Tree {
        Parser::new().parse_with(text, file_kind, parse)
    }

    /// The parsed text.
    pub fn source(&self) -> &[u8] {
        &self.text
    }

    pub fn file_kind(&self) -> FileKind {
        self.file_kind
    }

    /// The `KtFile` node.
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
            + messages
    }

    /// The element type: a node type for a composite, a token type for a leaf. A doc comment (`DOC_COMMENT`, "KDoc")
    /// and a markdown link (`KDOC_MARKDOWN_LINK`) are composites with a token type, as their PSI is.
    #[inline]
    pub fn kind(&self, id: NodeId) -> ElementType {
        let index = (self.data.kinds[id.0 as usize] & KIND_INDEX) as usize;
        ElementType::from_index(index).unwrap_or(ElementType::Node(NodeKind::ERROR_ELEMENT))
    }

    /// The token type of a leaf.
    pub fn token_kind(&self, id: NodeId) -> Option<TokenKind> {
        if self.is_leaf(id) { self.kind(id).token() } else { None }
    }

    /// The node type of a composite.
    pub fn node_kind(&self, id: NodeId) -> Option<NodeKind> {
        self.kind(id).node()
    }

    /// Whether the node is a block or lambda that `ParseOptions { expand_bodies: false }` left unexpanded: it has no
    /// children, `parse_body` gives them.
    pub fn is_unexpanded(&self, id: NodeId) -> bool {
        self.data.kinds[id.0 as usize] & UNEXPANDED != 0
    }

    /// `Parser::parse_body` on a fresh parser, every body inside expanded.
    pub fn parse_body(&self, id: NodeId) -> Option<Tree> {
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
    pub fn first_child_of_kind(&self, id: NodeId, kind: impl Into<ElementType>) -> Option<NodeId> {
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
    pub fn descendants(&self, id: NodeId) -> impl DoubleEndedIterator<Item = NodeId> + ExactSizeIterator + use<> {
        (id.0..self.data.links[id.0 as usize][1]).map(NodeId)
    }

    /// Each token leaf's type and byte range, in document order: a scan of the columns. Their ranges tile the text,
    /// but for the ranges of unexpanded bodies. This, not the lexer, is what exact highlighting
    /// reads: the parser remaps soft keywords (`IDENTIFIER` to `by`, `get`, `data`, ...) and joins `?.`, `?:` and `!!`
    /// from two lexer tokens, and both show only in the tree's leaves.
    pub fn leaves(&self) -> impl Iterator<Item = (TokenKind, Range<usize>)> + '_ {
        let d = &self.data;
        d.kinds.iter().enumerate().filter_map(move |(i, &k)| {
            if k & LEAF == 0 {
                return None;
            }
            let token = ElementType::from_index((k & KIND_INDEX) as usize)?.token()?;
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
fn reusable_tokens(file_tokens: &Lexed, p: &Pending) -> Option<(usize, usize)> {
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

// A tree is shared across threads as is, a parser moves to the thread that keeps it.
const _: () = {
    const fn send_sync<T: Send + Sync + 'static>() {}
    const fn send<T: Send + 'static>() {}
    send_sync::<Tree>();
    send::<Parser>();
};
