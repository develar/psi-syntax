//! The lossless syntax tree: one arena of nodes with `u32` ids, each a kind and a byte range, children in order,
//! tokens as leaves; whitespace and comments are leaves where PSI puts them. `Tree::parse` lexes, parses and builds
//! it, expanding every lazy element as PSI does when the tree is walked: a doc comment with the KDoc lexer and parser,
//! a markdown link inside it with the Kotlin lexer and `KDocLinkParser`, a collapsed block or lambda by reparsing its
//! text with `KotlinParser.parseBlockExpression` / `parseLambdaExpression` (a worklist, not recursion).

use std::borrow::Cow;
use std::ops::Range;
use std::sync::Arc;

use crate::builder::{LazyKind, Pending, PsiBuilderImpl, SemanticWhitespaceAwarePsiBuilderImpl};
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

#[derive(Copy, Clone, Debug)]
struct Node {
    kind: ElementType,
    leaf: bool,
    start: u32,
    end: u32,
    parent: u32,
    first_child: u32,
    last_child: u32,
    next_sibling: u32,
}

/// The arena a builder appends to.
#[derive(Default, Debug)]
pub(crate) struct TreeData {
    nodes: Vec<Node>,
    /// The message of each error node, sorted by node id.
    errors: Vec<(u32, Cow<'static, str>)>,
}

impl TreeData {
    fn push(&mut self, parent: u32, kind: ElementType, leaf: bool, start: u32, end: u32) -> u32 {
        let id = self.nodes.len() as u32;
        self.nodes.push(Node {
            kind,
            leaf,
            start,
            end,
            parent,
            first_child: NONE,
            last_child: NONE,
            next_sibling: NONE,
        });
        if parent != NONE {
            let last = self.nodes[parent as usize].last_child;
            if last == NONE {
                self.nodes[parent as usize].first_child = id;
            } else {
                self.nodes[last as usize].next_sibling = id;
            }
            self.nodes[parent as usize].last_child = id;
        }
        id
    }

    /// A composite node starting at `start` (its end is set when it is done) as the last child of `parent`
    /// (`u32::MAX`: a root).
    pub(crate) fn add_composite(&mut self, parent: u32, kind: ElementType, start: u32) -> u32 {
        self.push(parent, kind, false, start, start)
    }

    /// A leaf as the last child of `parent`.
    pub(crate) fn add_leaf(&mut self, parent: u32, kind: ElementType, start: u32, end: u32) -> u32 {
        self.push(parent, kind, true, start, end)
    }

    pub(crate) fn set_end(&mut self, node: u32, end: u32) {
        let n = &mut self.nodes[node as usize];
        n.end = end.max(n.start);
    }

    pub(crate) fn parent_of(&self, node: u32) -> Option<u32> {
        let parent = self.nodes[node as usize].parent;
        (parent != NONE).then_some(parent)
    }

    pub(crate) fn set_error(&mut self, node: u32, message: Cow<'static, str>) {
        self.errors.push((node, message));
    }
}

/// A parsed Kotlin file.
pub struct Tree {
    text: Arc<[u8]>,
    file_kind: FileKind,
    data: TreeData,
    root: u32,
}

impl Tree {
    /// Parses `text` as a `.kt` file or a script. Any bytes parse; invalid UTF-8 lexes as `BAD_CHARACTER`s. At most
    /// `u32::MAX - 1` bytes are parsed.
    pub fn parse(text: impl AsRef<[u8]>, file_kind: FileKind) -> Tree {
        Self::parse_with(text.as_ref(), file_kind, |builder| kotlin_parser::parse(builder, file_kind))
    }

    /// Lexes `text`, runs `parse` on its builder, builds and expands the tree.
    pub(crate) fn parse_with(
        text: &[u8],
        file_kind: FileKind,
        parse: impl FnOnce(&mut SemanticWhitespaceAwarePsiBuilderImpl),
    ) -> Tree {
        let text: Arc<[u8]> = Arc::from(&text[..text.len().min(u32::MAX as usize - 1)]);
        let lexed = lexer::tokenize(&text);
        let mut data = TreeData::default();
        let mut pending = Vec::new();

        let file = PsiBuilderImpl::new(text.clone(), 0, text.len(), lexed.types.clone(), lexed.starts.clone(), Some(0));
        let mut builder = SemanticWhitespaceAwarePsiBuilderImpl::new(file);
        parse(&mut builder);
        let root = builder.delegate.build_tree(&mut data, None, &mut pending);

        while let Some(p) = pending.pop() {
            expand(&text, &lexed, &mut data, p, &mut pending);
        }
        Tree { text, file_kind, data, root }
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

    /// The number of nodes.
    pub fn len(&self) -> usize {
        self.data.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.nodes.is_empty()
    }

    fn node(&self, id: NodeId) -> &Node {
        &self.data.nodes[id.0 as usize]
    }

    /// The element type: a node type for a composite, a token type for a leaf. A doc comment (`DOC_COMMENT`, "KDoc")
    /// and a markdown link (`KDOC_MARKDOWN_LINK`) are composites with a token type, as their PSI is.
    pub fn kind(&self, id: NodeId) -> ElementType {
        self.node(id).kind
    }

    /// The token type of a leaf.
    pub fn token_kind(&self, id: NodeId) -> Option<TokenKind> {
        let n = self.node(id);
        if n.leaf { n.kind.token() } else { None }
    }

    /// The node type of a composite.
    pub fn node_kind(&self, id: NodeId) -> Option<NodeKind> {
        self.node(id).kind.node()
    }

    /// Whether the node is a leaf (a token; it has no children).
    pub fn is_leaf(&self, id: NodeId) -> bool {
        self.node(id).leaf
    }

    /// The byte range in `source()`.
    pub fn range(&self, id: NodeId) -> Range<usize> {
        let n = self.node(id);
        n.start as usize..n.end as usize
    }

    /// The node's text.
    pub fn text(&self, id: NodeId) -> &[u8] {
        &self.text[self.range(id)]
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        let parent = self.node(id).parent;
        (parent != NONE).then_some(NodeId(parent))
    }

    pub fn first_child(&self, id: NodeId) -> Option<NodeId> {
        let child = self.node(id).first_child;
        (child != NONE).then_some(NodeId(child))
    }

    pub fn last_child(&self, id: NodeId) -> Option<NodeId> {
        let child = self.node(id).last_child;
        (child != NONE).then_some(NodeId(child))
    }

    pub fn next_sibling(&self, id: NodeId) -> Option<NodeId> {
        let next = self.node(id).next_sibling;
        (next != NONE).then_some(NodeId(next))
    }

    /// The children in order.
    pub fn children(&self, id: NodeId) -> Children<'_> {
        Children { tree: self, next: self.node(id).first_child }
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

    /// The nodes of the subtree at `id` in document order (preorder), with their depth below `id`.
    pub fn preorder(&self, id: NodeId) -> Preorder<'_> {
        Preorder { tree: self, root: id.0, next: id.0, depth: 0 }
    }

    /// The leaves (tokens, whitespace, comments) in document order.
    pub fn tokens(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.preorder(self.root()).map(|(id, _)| id).filter(|&id| self.is_leaf(id))
    }
}

/// The children of a node.
pub struct Children<'t> {
    tree: &'t Tree,
    next: u32,
}

impl Iterator for Children<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        if self.next == NONE {
            return None;
        }
        let id = self.next;
        self.next = self.tree.data.nodes[id as usize].next_sibling;
        Some(NodeId(id))
    }
}

/// A preorder walk of a subtree: `(node, depth below the subtree root)`.
pub struct Preorder<'t> {
    tree: &'t Tree,
    root: u32,
    next: u32,
    depth: usize,
}

impl Iterator for Preorder<'_> {
    type Item = (NodeId, usize);

    fn next(&mut self) -> Option<(NodeId, usize)> {
        if self.next == NONE {
            return None;
        }
        let id = self.next;
        let depth = self.depth;
        let nodes = &self.tree.data.nodes;
        let n = &nodes[id as usize];
        if n.first_child != NONE {
            self.next = n.first_child;
            self.depth += 1;
        } else {
            let mut cur = id;
            loop {
                if cur == self.root {
                    self.next = NONE;
                    break;
                }
                let c = &nodes[cur as usize];
                if c.next_sibling != NONE {
                    self.next = c.next_sibling;
                    break;
                }
                cur = c.parent;
                self.depth = self.depth.saturating_sub(1);
                if cur == NONE {
                    self.next = NONE;
                    break;
                }
            }
        }
        Some((NodeId(id), depth))
    }
}

/// Expands one chameleon: parses its text with a new builder and appends the result under its node.
fn expand(text: &Arc<[u8]>, file_tokens: &Lexed, data: &mut TreeData, p: Pending, pending: &mut Vec<Pending>) {
    let len = p.end - p.start;
    match p.kind {
        LazyKind::Block | LazyKind::Lambda => {
            let (types, starts, file_token) = match reusable_tokens(file_tokens, &p) {
                Some(tokens) => tokens,
                None => {
                    let lexed = lexer::tokenize(&text[p.start..p.end]);
                    (lexed.types, lexed.starts, None)
                }
            };
            let delegate = PsiBuilderImpl::new(text.clone(), p.start, len, types, starts, file_token);
            let mut builder = SemanticWhitespaceAwarePsiBuilderImpl::new(delegate);
            if p.kind == LazyKind::Block {
                kotlin_parser::parse_block_expression(&mut builder);
            } else {
                kotlin_parser::parse_lambda_expression(&mut builder);
            }
            builder.delegate.build_tree(data, Some(p.node), pending);
        }
        LazyKind::KDoc => {
            let (types, starts) = crate::kdoc::lexer::tokenize(&text[p.start..p.end]);
            let mut builder = PsiBuilderImpl::new(text.clone(), p.start, len, types, starts, None);
            crate::kdoc::parser::parse(ElementType::Token(TokenKind::DOC_COMMENT), &mut builder);
            builder.build_tree(data, Some(p.node), pending);
        }
        LazyKind::KDocLink => {
            let lexed = lexer::tokenize(&text[p.start..p.end]);
            let mut builder = PsiBuilderImpl::new(text.clone(), p.start, len, lexed.types, lexed.starts, None);
            crate::kdoc::parser::parse_markdown_link(ElementType::Token(TokenKind::KDOC_MARKDOWN_LINK), &mut builder);
            builder.build_tree(data, Some(p.node), pending);
        }
    }
}

/// The file's own tokens for a chameleon when lexing its text alone gives the same tokens: it starts at a token
/// lexed in the initial state with an empty state stack, ends at a token boundary where the lexer is back in that
/// state (or at the end of the file), and its last token is not empty. An empty last token is one the lexer emitted
/// by looking past the chameleon: `DANGLING_NEWLINE` of an unclosed string before a line break, which the text alone,
/// ending at the string, does not lex (`{a + "` then a newline: the fresh lexing ends in the string, so the string's
/// `Expecting '"'` sits at the end and the block's `Expecting '}'` at the same lexeme is dropped). The types are the
/// lexer's, before any remapping by the outer parse, as a fresh lexing would give them.
fn reusable_tokens(file_tokens: &Lexed, p: &Pending) -> Option<(Vec<TokenKind>, Vec<u32>, Option<usize>)> {
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
    let types = file_tokens.types[first..last].to_vec();
    let starts = file_tokens.starts[first..=last].iter().map(|s| s - p.start as u32).collect();
    Some((types, starts, Some(first)))
}
