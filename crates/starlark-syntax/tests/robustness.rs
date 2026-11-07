use psi_check::{check_leaves_are_text, check_links, nesting_limited, too_deep_errors};
use starlark_syntax::{FileKind, Parser, Starlark};

#[test]
fn malformed_inputs_remain_lossless() {
    let inputs: &[&[u8]] = &[
        b"def broken(",
        b"target(name =",
        b"if condition:\npass\n",
        b"load(\"rules.bzl\", alias = )",
        b"value = [1, 2,",
        b"value = {\"key\":",
        b"x = (1 + 2\n",
        b"x = \"unterminated\nnext = 1",
        b"x = b'bytes'\n",
        b"\t  mixed_indent = True\n",
    ];
    let mut parser = Parser::new();
    for input in inputs {
        let tree = parser.parse(input, FileKind::Bzl);
        check_leaves_are_text(&tree, input);
        check_links(&tree, |_, _| {});
    }
}

#[test]
fn seeded_expression_mutations_build_lossless_trees() {
    let alphabet = b"abcXYZ_0129 #()[]{}:+-*/=,\n\t'\".";
    let mut state = 0x5eed_u32;
    let mut parser = Parser::new();
    for len in 0..128 {
        let mut input = Vec::with_capacity(len);
        for _ in 0..len {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            input.push(alphabet[state as usize % alphabet.len()]);
        }
        let tree = parser.parse(&input, FileKind::Bzl);
        check_leaves_are_text(&tree, &input);
        check_links(&tree, |_, _| {});
    }
}

#[test]
fn parser_nesting_limit_keeps_the_tree_lossless() {
    let input = format!("{}1{}", "(".repeat(128), ")".repeat(128));
    let mut parser = nesting_limited::<Starlark>(8);
    let tree = parser.parse(input.as_bytes(), FileKind::Bzl);
    check_leaves_are_text(&tree, input.as_bytes());
    check_links(&tree, |_, _| {});
    assert!(too_deep_errors(&tree) > 0);
}
