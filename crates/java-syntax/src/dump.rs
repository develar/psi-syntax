//! The two dumps the Java parsing tests compare: `DebugUtil.psiToString(file, true, false).trim()` (`NAME.txt`, PSI
//! class names and their `toString()`, over `psi_builder::dump`'s layout) and
//! `DebugUtil.nodeTreeAsElementTypeToString(file.getNode(), true).trim()` (`NAME_node.txt`, the element types).
//!
//! A tree's types are java-syntax's; the PSI tree converts a few (`ElementTypeConverter`): `DOC_SPACE` becomes
//! `WHITE_SPACE`. The PSI labels follow the goldens; labels of types no golden has (code fragments, JShell) print
//! `PsiElement(<type>)` and are unverified.

use psi_builder::dump::psi_to_string_with;

use crate::node_kind::{Label, NodeKind};
use crate::lexer::is_java_letter_digit;
use crate::syntax_element_types::{EXPRESSION_BIT_SET, JAVA_COMMENT_OR_WHITESPACE_BIT_SET};
use crate::tokens::{ElementType, TokenKind};
use crate::tree::{NodeId, Tree};

/// `DebugUtil.psiToString(file, true, false).trim()` of `tree` as the file `file_name`.
pub fn psi_to_string(tree: &Tree, file_name: &str) -> String {
    psi_to_string_with(tree, tree.root(), |tree, id, out| label(tree, id, file_name, out))
}

/// The dump of the subtree at `node`, indented from 0 (`DebugUtil.psiToString(element, true, false).trim()`).
pub fn subtree_to_string(tree: &Tree, node: NodeId) -> String {
    psi_to_string_with(tree, node, |tree, id, out| label(tree, id, "", out))
}

/// The element type the PSI tree has for a node of `kind` (`ElementTypeConverter`).
fn psi_type_name(kind: ElementType) -> &'static str {
    match kind {
        ElementType::Token(TokenKind::DOC_SPACE) => TokenKind::WHITE_SPACE.debug_name(),
        _ => kind.debug_name(),
    }
}

/// `DebugUtil.nodeTreeAsElementTypeToString(file.getNode(), true).trim()`: each node's element type indented two
/// spaces per depth, `<empty list>` under a composite without children.
pub fn node_tree_to_string(tree: &Tree) -> String {
    let mut out = String::with_capacity(tree.len() * 24);
    for (id, depth) in tree.preorder(tree.root()) {
        push_indent(&mut out, depth);
        out.push_str(psi_type_name(tree.kind(id)));
        out.push('\n');
        if !tree.is_leaf(id) && tree.first_child(id).is_none() {
            push_indent(&mut out, depth + 1);
            out.push_str("<empty list>\n");
        }
    }
    let trimmed = out.trim_matches(|c: char| c <= ' ');
    if trimmed.len() != out.len() {
        out = trimmed.to_owned();
    }
    out
}

fn push_indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

/// The node's text, decoded as UTF-8 (an invalid byte is U+FFFD), as `getText()` gives it.
fn push_text(out: &mut String, text: &[u8]) {
    out.push_str(&String::from_utf8_lossy(text));
}

/// `PsiSnippetAttributeValueImpl.getValue()`: the text without one leading and one trailing quote (`"` or `'`), the
/// whole text when nothing would remain.
fn snippet_attribute_value(text: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = text.len();
    if text.first().is_some_and(|&b| b == b'"' || b == b'\'') {
        start += 1;
    }
    if text.last().is_some_and(|&b| b == b'"' || b == b'\'') {
        end -= 1;
    }
    if end <= start {
        return text;
    }
    &text[start..end]
}

/// `JavaSourceUtil.REF_FILTER`: `ElementType.JAVA_COMMENT_OR_WHITESPACE_BIT_SET` and `ANNOTATION`.
fn is_ref_filtered(kind: ElementType) -> bool {
    match kind {
        ElementType::Token(t) => JAVA_COMMENT_OR_WHITESPACE_BIT_SET.contains(t),
        ElementType::Node(n) => n == NodeKind::ANNOTATION,
    }
}

/// `Character.isJavaIdentifierPart(char)` of the UTF-16 unit `c` (a surrogate is not one).
fn is_java_identifier_part(c: char) -> bool {
    (c as u32) <= 0xFFFF && is_java_letter_digit(c as u32)
}

/// `JavaSourceUtil.getReferenceText(ref)`: the texts of the reference's leaves outside `REF_FILTER` (whitespace,
/// comments, annotations), a space between two that would otherwise join into one identifier (or after a `?`).
fn push_reference_text(tree: &Tree, reference: NodeId, out: &mut String) {
    let start = out.len();
    let end = tree.subtree_end(reference).0;
    let mut i = reference.0 + 1;
    while i < end {
        let id = NodeId(i);
        if is_ref_filtered(tree.kind(id)) {
            i = tree.subtree_end(id).0;
            continue;
        }
        i += 1;
        if !tree.is_leaf(id) {
            continue;
        }
        let leaf_text = String::from_utf8_lossy(tree.text(id));
        if out.len() > start
            && leaf_text.chars().next().is_some_and(is_java_identifier_part)
            && out[start..].chars().next_back().is_some_and(|last| last == '?' || is_java_identifier_part(last))
        {
            out.push(' ');
        }
        out.push_str(&leaf_text);
    }
}

/// What PSI's `toString()` prints for the node.
fn label(tree: &Tree, id: NodeId, file_name: &str, out: &mut String) {
    let kind = tree.kind(id);
    match kind {
        ElementType::Token(t) => {
            if !tree.is_leaf(id) {
                // an expanded lazy element: a doc comment or a holder
                match t {
                    TokenKind::DOC_COMMENT | TokenKind::DOC_MARKDOWN_COMMENT => out.push_str("PsiDocComment"),
                    _ => {
                        out.push_str("PsiElement(");
                        out.push_str(t.debug_name());
                        out.push(')');
                    }
                }
            } else if t.is_psi_white_space() {
                out.push_str("PsiWhiteSpace");
            } else if t.is_psi_comment() {
                out.push_str("PsiComment(");
                out.push_str(t.debug_name());
                out.push(')');
            } else if t.is_keyword() || t.is_identifier() {
                out.push_str(if t.is_keyword() { "PsiKeyword:" } else { "PsiIdentifier:" });
                push_text(out, tree.text(id));
            } else if t.is_psi_doc_token() {
                out.push_str("PsiDocToken:");
                out.push_str(t.debug_name());
            } else if t.is_fragment() {
                out.push_str("PsiFragment:");
                out.push_str(t.debug_name());
            } else if t.is_psi_java_token() {
                out.push_str("PsiJavaToken:");
                out.push_str(t.debug_name());
            } else {
                out.push_str("PsiElement(");
                out.push_str(t.debug_name());
                out.push(')');
            }
        }
        ElementType::Node(n) => node_label(tree, id, n, file_name, out),
    }
}

/// The `toString()` of a composite (or of a leaf of a composite type, a collapsed one).
fn node_label(tree: &Tree, id: NodeId, n: NodeKind, file_name: &str, out: &mut String) {
    let class = n.psi_class();
    match n.label() {
        Label::Plain => out.push_str(class),
        Label::Text if n == NodeKind::DOC_SNIPPET_ATTRIBUTE_VALUE => {
            out.push_str(class);
            out.push(':');
            push_text(out, snippet_attribute_value(tree.text(id)));
        }
        Label::Text => {
            out.push_str(class);
            out.push(':');
            push_text(out, tree.text(id));
        }
        Label::SpaceText => {
            out.push_str(class);
            out.push_str(": ");
            push_text(out, tree.text(id));
        }
        Label::Empty => {
            out.push_str(class);
            out.push(':');
        }
        Label::Name => {
            out.push_str(class);
            out.push(':');
            match tree.first_child_of_kind(id, ElementType::Token(TokenKind::IDENTIFIER)) {
                Some(name) => push_text(out, tree.text(name)),
                None if n == NodeKind::METHOD => out.push_str("<unnamed>"),
                None => out.push_str("null"),
            }
        }
        Label::ModuleName => {
            out.push_str(class);
            out.push(':');
            // `PsiJavaModuleReferenceElementImpl.getReferenceText()`: the children that are not whitespace or comments
            if let Some(reference) = tree.first_child_of_kind(id, ElementType::Node(NodeKind::MODULE_REFERENCE)) {
                for child in tree.children(reference) {
                    if !tree.kind(child).token().is_some_and(|t| JAVA_COMMENT_OR_WHITESPACE_BIT_SET.contains(t)) {
                        push_text(out, tree.text(child));
                    }
                }
            }
        }
        Label::PackageName => {
            out.push_str(class);
            out.push(':');
            // `getPackageName()`: `JavaSourceUtil.getReferenceText` of the package reference, "" without one
            if let Some(reference) = tree.first_child_of_kind(id, ElementType::Node(NodeKind::JAVA_CODE_REFERENCE)) {
                push_reference_text(tree, reference, out);
            }
        }
        Label::Diamond => {
            out.push_str(class);
            out.push_str(":DIAMOND");
        }
        Label::SwitchSelector => {
            out.push_str(class);
            out.push_str(": ");
            match tree.children(id).find(|&c| EXPRESSION_BIT_SET.contains(tree.kind(c))) {
                Some(selector) => push_text(out, tree.text(selector)),
                None => out.push_str("(incomplete)"),
            }
        }
        Label::DocTagName => {
            out.push_str(class);
            out.push(':');
            match tree.first_child_of_kind(id, ElementType::Token(TokenKind::DOC_TAG_NAME)) {
                Some(name) => push_text(out, tree.text(name)),
                // `PsiInlineDocTagImpl.toString()` concatenates a null name element's text; `PsiDocTagImpl`'s would
                // throw, and its parser never leaves a tag without a name
                None if n == NodeKind::DOC_INLINE_TAG => out.push_str("null"),
                None => {}
            }
        }
        Label::SnippetAttributeName => {
            out.push_str(class);
            out.push(':');
            if let Some(name) = tree.first_child_of_kind(id, ElementType::Token(TokenKind::DOC_TAG_ATTRIBUTE_NAME)) {
                push_text(out, tree.text(name));
            }
        }
        Label::File => {
            out.push_str(class);
            out.push(':');
            out.push_str(file_name);
        }
        Label::Error => {
            out.push_str("PsiErrorElement:");
            out.push_str(tree.error_message(id).unwrap_or(""));
        }
        Label::Element => {
            out.push_str("PsiElement(");
            out.push_str(n.debug_name());
            out.push(')');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::SyntaxTreeBuilder;
    use crate::tree::{FileKind, Parser};

    /// A tree of the file's tokens under its root (no parser runs: the tokens and both dumps' labels).
    fn tokens_only(text: &str) -> Tree {
        Parser::new().parse_with(text.as_bytes(), FileKind::default(), |builder| {
            let mut b = SyntaxTreeBuilder::new(builder);
            let root = b.mark();
            while !b.eof() {
                b.advance_lexer();
            }
            root.done(&mut b, NodeKind::JAVA_FILE);
        })
    }

    #[test]
    fn dumps_label_the_leaves_as_psi_does() {
        let tree = tokens_only("class A {} // c\n@x 'c' 1 true");
        assert_eq!(
            psi_to_string(&tree, "A.java"),
            "PsiJavaFile:A.java\n  PsiKeyword:class('class')\n  PsiWhiteSpace(' ')\n  PsiIdentifier:A('A')\n  \
             PsiWhiteSpace(' ')\n  PsiJavaToken:LBRACE('{')\n  PsiJavaToken:RBRACE('}')\n  PsiWhiteSpace(' ')\n  \
             PsiComment(END_OF_LINE_COMMENT)('// c')\n  PsiWhiteSpace('\\n')\n  PsiJavaToken:AT('@')\n  \
             PsiIdentifier:x('x')\n  PsiWhiteSpace(' ')\n  PsiJavaToken:CHARACTER_LITERAL(''c'')\n  PsiWhiteSpace(' ')\n  \
             PsiJavaToken:INTEGER_LITERAL('1')\n  PsiWhiteSpace(' ')\n  PsiJavaToken:TRUE_KEYWORD('true')"
        );
        assert_eq!(
            node_tree_to_string(&tokens_only("int x;")),
            "java.FILE\n  INT_KEYWORD\n  WHITE_SPACE\n  IDENTIFIER\n  SEMICOLON"
        );
    }

    /// The labels of the nodes of `text`'s dump that start with `prefix`.
    fn labels(text: &str, prefix: &str) -> Vec<String> {
        let tree = Parser::new().parse(text.as_bytes(), FileKind::default());
        psi_to_string(&tree, "A.java").lines().map(str::trim).filter(|l| l.starts_with(prefix)).map(String::from).collect()
    }

    #[test]
    fn package_name_is_the_reference_text_without_annotations_and_comments() {
        // `JavaSourceUtil.getReferenceText`, as the Java PSI oracle prints these
        assert_eq!(labels("package pack1.@code pack2;", "PsiPackageStatement"), ["PsiPackageStatement:pack1.pack2"]);
        assert_eq!(
            labels("package a /* c */ . /** d */ b . @A(x=1) c;", "PsiPackageStatement"),
            ["PsiPackageStatement:a.b.c"]
        );
        assert_eq!(labels("module a /** x */ . b {}", "PsiJavaModule:"), ["PsiJavaModule:a.b"]);
    }

    #[test]
    fn inline_doc_tag_without_a_name_prints_null() {
        assert_eq!(
            labels("/** {@ doc } {@code x} */\nclass A {}", "PsiInlineDocTag"),
            ["PsiInlineDocTag:null", "PsiInlineDocTag:@code"]
        );
    }
}
