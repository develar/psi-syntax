//! `JavaSourceUtil` (`java/java-psi-impl/src/com/intellij/psi/impl/source/tree/JavaSourceUtil.java`): a reference's
//! text as written, without whitespace, comments and annotations.

use crate::node_kind::NodeKind;
use crate::syntax_element_types::JAVA_COMMENT_OR_WHITESPACE_BIT_SET;
use crate::tree::{NodeId, Tree};

/// `getReferenceText(ref)` (`JavaSourceUtil.java:74`): the leaves of the reference but those of `REF_FILTER` (`:31`:
/// whitespace, comments and the subtrees of annotations), a space between two leaves where the next starts with a
/// Java identifier part and the text so far ends with one or with `?` (`List<? extends T>` keeps its spaces). The
/// package name of `package a . /* c */ b;` is `a.b`, the name of `@ org.junit.Test` `org.junit.Test`: the tree-pure
/// part of `getQualifiedName()`, which resolves.
pub fn get_reference_text(tree: &Tree, reference: NodeId) -> String {
    let mut buffer = String::new();
    append_reference_text(tree, reference, &mut buffer);
    buffer
}

/// `get_reference_text` appended to `buffer` (the space rule reads what the buffer holds already, as IntelliJ's
/// fresh buffer starts empty: pass an empty one for the same text).
pub fn append_reference_text(tree: &Tree, reference: NodeId, buffer: &mut String) {
    let start = buffer.len();
    let filtered = |n: NodeId| {
        JAVA_COMMENT_OR_WHITESPACE_BIT_SET.contains(tree.kind(n)) || tree.node_kind(n) == Some(NodeKind::ANNOTATION)
    };
    for leaf in tree.descendants_pruned(reference, filtered).filter(|&n| tree.is_leaf(n)) {
        if filtered(leaf) {
            continue;
        }
        let text = String::from_utf8_lossy(tree.text(leaf));
        if buffer.len() > start
            && text.chars().next().is_some_and(is_java_identifier_part)
            && buffer.chars().next_back().is_some_and(|last| last == '?' || is_java_identifier_part(last))
        {
            buffer.push(' ');
        }
        buffer.push_str(&text);
    }
}

/// `Character.isJavaIdentifierPart(char)` of a UTF-16 unit: `[:jletterdigit:]` as the lexer reads it (JFlex's table of
/// that method); a character past the Basic Multilingual Plane is a surrogate unit there, which is none.
fn is_java_identifier_part(c: char) -> bool {
    (c as u32) < 0x10000 && crate::lexer::is_java_letter_digit(c as u32)
}
