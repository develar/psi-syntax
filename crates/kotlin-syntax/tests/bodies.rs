//! Body-less parsing (`ParseOptions { expand_bodies: false }`) on every golden: the tree is the full tree with each
//! lazily parsed block and lambda left childless, and `parse_body` on each of them gives the full tree's subtree.
//! Also `leaves()` and `descendants()` against the tree they scan.

mod common;

use kotlin_syntax::dump::subtree_to_string;
use kotlin_syntax::{FileKind, NodeId, ParseOptions, Parser, Tree};

/// Each node of the subtree at `id` with what identifies it: depth, kind, leafness, range, error message.
fn shape(tree: &Tree, id: NodeId) -> Vec<String> {
    tree.preorder(id)
        .map(|(n, depth)| {
            format!("{depth} {:?} {} {:?} {:?}", tree.kind(n), tree.is_leaf(n), tree.range(n), tree.error_message(n))
        })
        .collect()
}

#[test]
fn body_less_trees_are_full_trees_without_body_interiors() {
    let mut parser = Parser::new();
    let lazy_options = ParseOptions { expand_bodies: false };
    let (mut files, mut bodies) = (0, 0);
    for golden in common::goldens() {
        let text = common::source_text(&golden);
        let kind = FileKind::from_file_name(&common::file_name(&golden));
        let full = parser.parse(&text, kind);
        let lazy = parser.parse_with_options(&text, kind, lazy_options);
        assert!(lazy.len() <= full.len());
        // Walk both in preorder; an unexpanded body skips the full node's subtree.
        let (mut l, mut f) = (0u32, 0u32);
        while (l as usize) < lazy.len() {
            let (ln, fnode) = (NodeId(l), NodeId(f));
            assert_eq!(lazy.kind(ln), full.kind(fnode), "{}", golden.source.display());
            assert_eq!(lazy.range(ln), full.range(fnode), "{}", golden.source.display());
            assert_eq!(lazy.is_leaf(ln), full.is_leaf(fnode));
            assert_eq!(lazy.error_message(ln), full.error_message(fnode));
            assert_eq!(lazy.parent(ln).is_some(), full.parent(fnode).is_some());
            if lazy.is_unexpanded(ln) {
                assert!(lazy.first_child(ln).is_none());
                bodies += 1;
                let body = lazy.parse_body(ln).expect("an unexpanded body parses");
                assert_eq!(body.range(body.root()), full.range(fnode));
                assert_eq!(shape(&body, body.root()), shape(&full, fnode), "{}", golden.source.display());
                assert_eq!(subtree_to_string(&body, body.root()), subtree_to_string(&full, fnode));
                // A body parsed body-less in turn leaves its own bodies out.
                let shallow = parser.parse_body(&lazy, ln, lazy_options).expect("an unexpanded body parses");
                assert!(shallow.len() <= body.len());
                f = full.subtree_end(fnode).0;
            } else {
                assert!(full.parse_body(fnode).is_none());
                f += 1;
            }
            l += 1;
        }
        assert_eq!(f as usize, full.len(), "{}", golden.source.display());
        files += 1;
    }
    assert!(bodies > 1000, "{bodies} bodies in {files} files");
}

#[test]
fn leaves_and_descendants_scan_the_tree() {
    for golden in common::goldens() {
        let text = common::source_text(&golden);
        let tree = Tree::parse(&text, FileKind::from_file_name(&common::file_name(&golden)));
        let expected: Vec<_> =
            tree.tokens().filter_map(|id| tree.token_kind(id).map(|t| (t, tree.range(id)))).collect();
        assert_eq!(tree.leaves().collect::<Vec<_>>(), expected);
        let covered: usize = tree.leaves().map(|(_, r)| r.len()).sum();
        assert_eq!(covered, text.len());
        for id in [tree.root(), NodeId(tree.len() as u32 / 2)] {
            let walked: Vec<NodeId> = tree.preorder(id).map(|(n, _)| n).collect();
            assert_eq!(tree.descendants(id).collect::<Vec<_>>(), walked);
        }
    }
}
