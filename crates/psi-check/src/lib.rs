//! What the parsers' tests and `ksdump` check of a tree, the same for every language over `psi_builder::Tree`: the
//! leaves are the text, the links agree, two subtrees are the same, the nesting and stack limits hold; and what their
//! robustness tests share: the wall-clock limits, the small stack, deterministic generators and the inputs built from
//! them.

use std::path::PathBuf;
use std::time::Duration;

use psi_builder::stack::StackUse;
use psi_builder::{Language, NodeId, Parser, Tree, TreeLanguage};

/// A wall-clock limit of these tests, multiplied by `PSI_SYNTAX_TIME_SCALE` (accio's CI sets it for its shared runners);
/// unset, the limit as measured on a developer's machine, which `scripts/gate.sh` holds.
pub fn limit(limit: Duration) -> Duration {
    limit * std::env::var("PSI_SYNTAX_TIME_SCALE").ok().and_then(|s| s.parse::<u32>().ok()).unwrap_or(1)
}

/// The stack a parse is promised to fit (AGENTS.md, "The consumer contract"): 1 MB in a release build; a debug build's
/// frames are several times larger.
pub const STACK: usize = if cfg!(debug_assertions) { 16 << 20 } else { 1 << 20 };

/// Runs `f` on a `STACK` thread named `name`: a stack overflow aborts the test binary with the form's name.
pub fn on_stack<T: Send + 'static>(name: &str, f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new().name(name.to_string()).stack_size(STACK).spawn(f).unwrap().join().unwrap()
}

/// A deterministic generator (xorshift64), so a failure reproduces.
pub struct XorShift(pub u64);

impl XorShift {
    #[allow(clippy::should_implement_trait)] // a generator, not an iterator: it never ends
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// The text of a random round: up to 200 random bytes every third round, else up to 200 random `pieces`.
    pub fn bytes_or_pieces(&mut self, round: usize, pieces: &[&[u8]]) -> Vec<u8> {
        let len = (self.next() % 200) as usize;
        let mut text = Vec::with_capacity(len * 3);
        for _ in 0..len {
            if round.is_multiple_of(3) {
                text.push(self.next() as u8);
            } else {
                text.extend_from_slice(pieces[(self.next() % pieces.len() as u64) as usize]);
            }
        }
        text
    }
}

/// A deterministic generator (xorshift64*), so a failure reproduces.
pub struct Rng(pub u64);

impl Rng {
    #[allow(clippy::should_implement_trait)] // a generator, not an iterator: it never ends
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545F4914F6CDD1D)
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// `levels` of `forms` (an opening and its closing text) taken by `pick(forms.len())`, nested around `1`.
pub fn nested_mix(forms: &[(&str, &str)], levels: usize, mut pick: impl FnMut(usize) -> usize) -> String {
    let (mut open, mut close) = (String::new(), Vec::new());
    for _ in 0..levels {
        let (o, c) = forms[pick(forms.len())];
        open.push_str(o);
        close.push(c);
    }
    close.reverse();
    format!("{open}1{}", close.concat())
}

/// The chains of `n` binary operators of Java and C# (one polyadic expression or a loop of the parser), then the
/// comparisons that look like type arguments, each a field initializer of `field`.
pub fn operator_chains(n: usize, field: fn(&str) -> String) -> Vec<(&'static str, String)> {
    let r = |s: &str| s.repeat(n);
    let mut chains: Vec<(&'static str, String)> =
        ["+", "-", "*", "/", "%", "<<", ">>", ">>>", "<", ">", "<=", ">=", "==", "!=", "&", "^", "|", "&&", "||"]
            .iter()
            .map(|op| (*op, field(&format!("a{}", format!(" {op} a").repeat(n)))))
            .collect();
    chains.extend([
        ("<, no spaces", field(&format!("a{}", r("<a")))),
        (">, no spaces", field(&format!("a{}", r(">a")))),
        ("generic-looking", field(&format!("a{}", r("<b>c")))),
    ]);
    chains
}

/// The longest climb `check_links` asks `Tree::last_child` for: past it, the check holds the climb's premise instead.
const LAST_CHILD_CLIMB: u32 = 64;

/// Ids are preorder, every node but the root has a parent before it whose subtree and range hold its own, and the
/// first child, next sibling and last child links agree with the parent links; `each(id, previous)` sees every node
/// but the root with the sibling before it by those links.
///
/// `Tree::last_child` climbs the parents from the subtree's last node, a step per level, so asking it of every node of
/// a chain 100 000 deep is quadratic (80 s of C#'s robustness test, against its 3 s of parsing). It is asked wherever
/// the climb is at most `LAST_CHILD_CLIMB` levels, which is every node of ordinary code; on a deeper one the check
/// holds what that climb reaches instead: the last child by the parent links ends where its parent's subtree ends, so
/// it is the ancestor of the subtree's last node whose parent is the node, given the preorder and parents checked here.
pub fn check_links<L: TreeLanguage>(tree: &Tree<L>, mut each: impl FnMut(NodeId, Option<NodeId>)) {
    assert!(tree.preorder(tree.root()).map(|(id, _)| id.0 as usize).eq(0..tree.len()), "ids are not preorder");
    let mut last_child: Vec<Option<NodeId>> = vec![None; tree.len()];
    let mut depth = vec![0u32; tree.len()];
    for i in 1..tree.len() as u32 {
        let id = NodeId(i);
        let parent = tree.parent(id).expect("every node but the root has a parent");
        assert!(parent.0 < i && tree.subtree_end(parent).0 >= tree.subtree_end(id).0);
        let (p, r) = (tree.range(parent), tree.range(id));
        assert!(p.start <= r.start && r.end <= p.end, "{id:?} {r:?} is outside its parent's {p:?}");
        match last_child[parent.0 as usize] {
            None => assert_eq!(tree.first_child(parent), Some(id)),
            Some(previous) => assert_eq!(tree.next_sibling(previous), Some(id)),
        }
        each(id, last_child[parent.0 as usize]);
        last_child[parent.0 as usize] = Some(id);
        depth[i as usize] = depth[parent.0 as usize] + 1;
    }
    for (i, last) in last_child.iter().enumerate() {
        let id = NodeId(i as u32);
        let end = tree.subtree_end(id);
        let climb = if end.0 > id.0 + 1 { depth[end.0 as usize - 1] - depth[i] } else { 0 };
        if climb <= LAST_CHILD_CLIMB {
            assert_eq!(tree.last_child(id), *last);
        } else {
            let last = last.expect("a node with descendants has a last child");
            assert_eq!(tree.subtree_end(last), end, "{id:?}'s last child {last:?} does not end its subtree");
        }
    }
}

/// The leaves, in order, are the whole text, and the root's range is the text's.
pub fn check_leaves_are_text<L: TreeLanguage>(tree: &Tree<L>, text: &[u8]) {
    let covered: Vec<u8> = tree.tokens().flat_map(|id| tree.text(id).iter().copied()).collect();
    assert!(covered == text, "the leaves are not the text");
    assert_eq!(tree.range(tree.root()), 0..text.len());
}

/// The subtree of `a` at `a_root` and that of `b` at `b_root` have the same nodes: kinds, ranges, error messages and
/// shape (what their dumps show, and the ranges).
pub fn same_subtree<L: TreeLanguage>(a: &Tree<L>, a_root: NodeId, b: &Tree<L>, b_root: NodeId) -> bool {
    let size = a.subtree_end(a_root).0 - a_root.0;
    if b.subtree_end(b_root).0 - b_root.0 != size {
        return false;
    }
    (0..size).all(|k| {
        let (x, y) = (NodeId(a_root.0 + k), NodeId(b_root.0 + k));
        a.kind(x) == b.kind(y)
            && a.range(x) == b.range(y)
            && a.error_message(x) == b.error_message(y)
            && a.subtree_end(x).0 - x.0 == b.subtree_end(y).0 - y.0
            && (k == 0 || a.parent(x).map(|p| x.0 - p.0) == b.parent(y).map(|p| y.0 - p.0))
    })
}

/// The error elements of a construct past a nesting limit or the stack budget (`builder::TOO_DEEPLY_NESTED`).
pub fn too_deep_errors<L: TreeLanguage>(tree: &Tree<L>) -> usize {
    (0..tree.len() as u32).filter(|&i| tree.error_message(NodeId(i)) == Some("Code is too deeply nested")).count()
}

/// A parser with the nesting limit `limit`, so that ordinary code takes the too-deep paths.
pub fn nesting_limited<L: Language>(limit: u32) -> Parser<L> {
    let mut parser = Parser::new();
    parser.set_nesting_limit(limit);
    parser
}

/// Walks the body-less parse `lazy` of `name` beside its full parse `full`: each node has the range of the full
/// tree's node there, and `body(lazy_id, full_id)` checks every unexpanded body against the full tree's subtree.
/// Returns the bodies.
pub fn each_body<L: TreeLanguage>(
    name: &str,
    lazy: &Tree<L>,
    full: &Tree<L>,
    mut body: impl FnMut(NodeId, NodeId),
) -> usize {
    let (mut f, mut bodies) = (0u32, 0);
    for l in 0..lazy.len() as u32 {
        let (ln, fnode) = (NodeId(l), NodeId(f));
        assert_eq!(lazy.range(ln), full.range(fnode), "{name}");
        if lazy.is_unexpanded(ln) {
            bodies += 1;
            body(ln, fnode);
            f = full.subtree_end(fnode).0;
        } else {
            f += 1;
        }
    }
    bodies
}

/// The levels and stack `parse` takes on a `STACK` thread for every form, printed; the forms that ran out of stack
/// below `min_levels` levels are the failures.
pub fn depth_limit_failures(
    forms: Vec<(&'static str, String)>,
    min_levels: u32,
    parse: fn(&[u8]) -> StackUse,
) -> Vec<String> {
    let mut failures = Vec::new();
    for (name, text) in forms {
        let used = on_stack(name, move || parse(text.as_bytes()));
        println!(
            "{name}: {} levels, {} bytes, {} a level{}",
            used.levels,
            used.bytes,
            used.bytes / used.levels.max(1) as usize,
            if used.out_of_stack() { ", out of stack" } else { "" }
        );
        if used.out_of_stack() && used.levels < min_levels {
            failures.push(format!("{name}: out of stack at {} levels ({} bytes)", used.levels, used.bytes));
        }
    }
    failures
}

/// A parser keeps at most its retained bytes from one file to the next (`Parser::set_retained_bytes`), and a parser
/// whose buffers were shrunk parses as a fresh one does: `parse` parses with the parser and checks the tree against a
/// fresh parse, of `file` and of `big` (`file` repeated past 8 MiB of buffers).
pub fn check_retained_bytes<L: Language>(file: &str, big: &str, parse: impl Fn(&mut Parser<L>, &str)) {
    let mut parser = Parser::new();
    parser.set_retained_bytes(8 << 20);
    parse(&mut parser, big);
    assert!(parser.grown_bytes() > 8 << 20, "{}", parser.grown_bytes());
    assert!(parser.heap_bytes() <= 4 << 20, "{}", parser.heap_bytes());
    // a share under 1 / the frames' capacity still keeps the file's builder (`Shrink::nested`), so the buffers keep
    // nearly all of what they are shrunk to (0.58 of it while the share dropped every frame)
    assert!(parser.heap_bytes() > 3 << 20, "{}", parser.heap_bytes());
    parse(&mut parser, file);
    parse(&mut parser, big);
    parser.shrink_to(0);
    assert_eq!(parser.heap_bytes(), 0);
    parse(&mut parser, file);
    parser.set_retained_bytes(usize::MAX);
    parse(&mut parser, big);
    assert_eq!(parser.heap_bytes(), parser.grown_bytes());
}

/// The files with `extension` under the directories `roots` names (`:`-separated), depth first: the inputs of a corpus
/// run.
pub fn files_under(roots: &str, extension: &str) -> impl Iterator<Item = PathBuf> + use<> {
    let mut stack: Vec<PathBuf> = roots.split(':').map(Into::into).collect();
    let extension = extension.to_string();
    std::iter::from_fn(move || {
        while let Some(path) = stack.pop() {
            if path.is_dir() {
                stack.extend(std::fs::read_dir(&path).into_iter().flatten().flatten().map(|e| e.path()));
            } else if path.extension().is_some_and(|e| *e == *extension) {
                return Some(path);
            }
        }
        None
    })
}

/// `String.trim()` of a Java test's text, by bytes: the bytes up to a space at both ends.
pub fn trim(text: Vec<u8>) -> Vec<u8> {
    let start = text.iter().position(|&b| b > b' ').unwrap_or(text.len());
    let end = text.iter().rposition(|&b| b > b' ').map_or(start, |e| e + 1);
    text[start..end].to_vec()
}

/// The end of a comparison of a golden's leaf texts with our tokens, stopped at leaf `i` and token `j`: both used up,
/// or the first leaf that differs, or the lengths.
pub fn leaves_compared<K: std::fmt::Debug>(
    expected: &[String],
    ours: &[(K, String)],
    i: usize,
    j: usize,
) -> Result<(), String> {
    if i < expected.len() && j < ours.len() {
        return Err(format!("leaf {i}: want {:?} got {:?} ({:?})", expected[i], ours[j].1, ours[j].0));
    }
    if i < expected.len() || j < ours.len() {
        return Err(format!("lengths: {} leaves, {} tokens, stopped at {i}/{j}", expected.len(), ours.len()));
    }
    Ok(())
}
