//! `kotlin_syntax::psi` against the live PSI: each `tests/psi/NAME.kt` (or `.kts`) beside `NAME.getters.txt`, the
//! lines `tools/kotlin-psi-getters/GetterOracle.java` printed for it (Kotlin 2.4.21's own getters); the lines of
//! `tests/common/getters.rs` must equal them.

mod common;

use std::path::Path;

use common::getters;
use kotlin_syntax::{FileKind, Tree};

#[test]
fn getters_answer_as_the_psi() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/psi");
    let mut sources: Vec<_> = std::fs::read_dir(&dir)
        .expect("tests/psi")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "kt" || e == "kts"))
        .collect();
    sources.sort();
    assert!(!sources.is_empty());
    let mut failures = Vec::new();
    for source in &sources {
        let name = source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let text = getters::oracle_text(&std::fs::read(source).expect("read source"));
        let expected_path = dir.join(format!("{name}.getters.txt"));
        let expected = std::fs::read_to_string(&expected_path).expect("read the oracle's lines");
        let actual = getters::lines(&Tree::parse(&text, FileKind::from_file_name(&name)));
        if actual != expected {
            let first = actual.lines().zip(expected.lines()).position(|(a, e)| a != e);
            let line = first.unwrap_or(actual.lines().count().min(expected.lines().count()));
            failures.push(format!(
                "{name}: line {}: ours {:?}, the oracle's {:?}",
                line + 1,
                actual.lines().nth(line),
                expected.lines().nth(line)
            ));
        }
    }
    assert!(failures.is_empty(), "differ from the PSI:\n{}", failures.join("\n"));
}

/// FNV-1a 64 of the lines, as `GetterOracle.java --hash` and the example `psi_getters --hash` take it.
fn fnv(lines: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in lines.as_bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Every golden of `testdata/` parsed as the file it is (not trimmed: the oracle parses the file), its lines'
/// hash against `tests/psi/goldens.hash`, `GetterOracle.java --hash` over the same list.
#[test]
fn getters_answer_as_the_psi_on_every_golden() {
    let root = common::testdata();
    let hashes = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/psi/goldens.hash"))
        .expect("tests/psi/goldens.hash");
    let mut failures = Vec::new();
    let mut checked = 0;
    for line in hashes.lines() {
        let mut columns = line.split('\t');
        let (Some(path), Some(hash), Some(count)) = (columns.next(), columns.next(), columns.next()) else {
            panic!("a line of goldens.hash: {line:?}");
        };
        let source = root.join(path);
        let name = source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let text = getters::oracle_text(&std::fs::read(&source).expect("read the golden"));
        let lines = getters::lines(&Tree::parse(&text, FileKind::from_file_name(&name)));
        let ours = format!("{:016x}\t{}", fnv(&lines), lines.bytes().filter(|&b| b == b'\n').count());
        if ours != format!("{hash}\t{count}") {
            failures.push(path.to_owned());
        }
        checked += 1;
    }
    assert_eq!(checked, common::goldens().len(), "goldens.hash lists every golden");
    assert!(
        failures.is_empty(),
        "differ from the PSI (diff the lines of `GetterOracle.java --out` and `psi_getters --out`): {failures:?}"
    );
}

/// Every getter on every node, whatever its kind, of random bytes, random Kotlin fragments and every golden cut at
/// many points: no getter panics on any tree, and each answer is a node of the tree.
#[test]
fn every_getter_on_every_node_of_any_tree() {
    let mut rng = 0x9E37_79B9_7F4A_7C15_u64;
    let mut next = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    let pieces: &[&[u8]] = &[
        b"fun ",
        b"val ",
        b"var ",
        b"class ",
        b"object ",
        b"interface ",
        b"enum ",
        b"companion ",
        b"private ",
        b"internal ",
        b"@",
        b"@get:",
        b"@[",
        b"]",
        b"(",
        b")",
        b"<",
        b">",
        b"{",
        b"}",
        b":",
        b",",
        b".",
        b"?.",
        b"?",
        b"=",
        b"->",
        b"`",
        b"`a b`",
        b"x",
        b"Foo",
        b"import ",
        b"typealias ",
        b"by ",
        b"get() ",
        b"where ",
        b"*",
        b"\n",
        b" ",
        b"\xFF",
        b"$",
        b"\"",
        b"vararg ",
        b"constructor",
        b"init ",
        b"(a, b)",
        b"dynamic",
        b"&",
    ];
    let mut texts: Vec<(Vec<u8>, FileKind)> = Vec::new();
    for round in 0..2000 {
        let len = (next() % 120) as usize;
        let mut text = Vec::new();
        for _ in 0..len {
            if round % 4 == 0 {
                text.push(next() as u8);
            } else {
                text.extend_from_slice(pieces[(next() % pieces.len() as u64) as usize]);
            }
        }
        texts.push((text, if round % 2 == 0 { FileKind::Kt } else { FileKind::Kts }));
    }
    for golden in common::goldens() {
        let text = std::fs::read(&golden.source).expect("read the golden");
        let step = (text.len() / 5).max(1);
        let mut cut = 0;
        while cut <= text.len() {
            texts.push((text[..cut].to_vec(), FileKind::from_file_name(&golden.name)));
            cut += step;
        }
        texts.push((text, FileKind::from_file_name(&golden.name)));
    }
    for (text, kind) in &texts {
        let tree = Tree::parse(text, *kind);
        for id in tree.descendants(tree.root()) {
            every_getter(&tree, id);
        }
    }
}

fn every_getter(tree: &Tree, e: kotlin_syntax::NodeId) {
    use kotlin_syntax::psi::*;
    let len = tree.len();
    let node = |n: Option<kotlin_syntax::NodeId>| assert!(n.is_none_or(|n| (n.0 as usize) < len));
    let nodes = |ns: &mut dyn Iterator<Item = kotlin_syntax::NodeId>| ns.for_each(|n| assert!((n.0 as usize) < len));
    let _ = is_kt_expression(tree, e);
    node(get_modifier_list(tree, e));
    for &m in kotlin_syntax::tokens::MODIFIER_KEYWORDS_ARRAY.iter() {
        let _ = has_modifier(tree, e, m);
        let _ = kt_modifier_list_has_modifier(tree, e, m);
        node(kt_modifier_list_get_modifier(tree, e, m));
    }
    nodes(&mut kt_modifier_list_get_annotations(tree, e));
    nodes(&mut kt_modifier_list_get_annotation_entries(tree, e));
    nodes(&mut get_annotation_entries(tree, e));
    node(kt_file_get_file_annotation_list(tree, e));
    node(kt_modifier_list_visibility_modifier(tree, e));
    let _ = kt_modifier_list_visibility_modifier_type(tree, e);
    node(visibility_modifier(tree, e));
    let _ = visibility_modifier_type(tree, e);
    node(modality_modifier(tree, e));
    let _ = modality_modifier_type(tree, e);
    let _ = (is_private(tree, e), is_protected(tree, e));
    nodes(&mut kt_annotation_get_entries(tree, e));
    node(kt_annotation_get_use_site_target(tree, e));
    node(kt_annotation_entry_get_type_reference(tree, e));
    node(kt_annotation_entry_get_at_symbol(tree, e));
    node(kt_annotation_entry_get_use_site_target(tree, e));
    let _ = kt_annotation_entry_get_short_name(tree, e);
    let _ = kt_annotation_use_site_target_get_annotation_use_site_target(tree, e);
    node(get_name_identifier(tree, e));
    let _ = get_name(tree, e);
    node(kt_constructor_get_containing_class_or_object(tree, e));
    node(containing_class_or_object(tree, e));
    node(kt_class_body_containing_class_or_object(tree, e));
    node(containing_script(tree, e));
    node(containing_class_or_script(tree, e));
    let _ = (kt_class_or_object_is_top_level(tree, e), kt_class_or_object_is_data(tree, e));
    let _ = kt_class_or_object_is_annotation(tree, e);
    node(kt_class_or_object_get_declaration_keyword(tree, e));
    node(kt_class_or_object_get_super_type_list(tree, e));
    nodes(&mut kt_super_type_list_get_entries(tree, e));
    nodes(&mut kt_class_or_object_get_super_type_list_entries(tree, e));
    node(kt_class_or_object_get_body(tree, e));
    node(kt_class_or_object_get_primary_constructor(tree, e));
    node(kt_class_or_object_get_primary_constructor_parameter_list(tree, e));
    nodes(&mut kt_class_or_object_get_primary_constructor_parameters(tree, e));
    nodes(&mut kt_class_or_object_get_secondary_constructors(tree, e));
    let _ = (kt_class_is_interface(tree, e), kt_class_is_enum(tree, e), kt_class_is_sealed(tree, e));
    let _ = (kt_class_is_inner(tree, e), kt_class_is_inline(tree, e), kt_class_is_value(tree, e));
    node(kt_class_get_fun_keyword(tree, e));
    let _ = (kt_object_declaration_is_companion(tree, e), kt_object_declaration_is_object_literal(tree, e));
    node(kt_object_declaration_get_object_keyword(tree, e));
    node(kt_super_type_list_entry_get_type_as_user_type(tree, e));
    node(get_receiver_type_reference(tree, e));
    node(get_type_reference(tree, e));
    node(get_value_parameter_list(tree, e));
    nodes(&mut get_value_parameters(tree, e));
    node(get_type_parameter_list(tree, e));
    nodes(&mut get_type_parameters(tree, e));
    node(get_type_constraint_list(tree, e));
    nodes(&mut get_type_constraints(tree, e));
    let _ = is_extension_declaration(tree, e);
    let _ = (kt_named_function_is_local(tree, e), kt_named_function_is_anonymous(tree, e));
    let _ = (kt_named_function_is_top_level(tree, e), kt_property_is_top_level(tree, e));
    let _ = (kt_property_is_member(tree, e), kt_property_is_local(tree, e), is_var(tree, e));
    node(get_val_or_var_keyword(tree, e));
    node(get_initializer(tree, e));
    node(kt_property_get_delegate(tree, e));
    let _ = kt_property_has_delegate(tree, e);
    node(kt_property_get_delegate_expression(tree, e));
    nodes(&mut kt_property_get_accessors(tree, e));
    node(kt_property_get_getter(tree, e));
    node(kt_property_get_setter(tree, e));
    let _ = (kt_property_accessor_is_getter(tree, e), kt_property_accessor_is_setter(tree, e));
    node(kt_parameter_get_default_value(tree, e));
    let _ = (kt_parameter_has_default_value(tree, e), kt_parameter_is_mutable(tree, e));
    let _ = (kt_parameter_is_var_arg(tree, e), kt_parameter_has_val_or_var(tree, e));
    node(kt_parameter_get_destructuring_declaration(tree, e));
    let _ = (kt_parameter_is_loop_parameter(tree, e), kt_parameter_is_catch_parameter(tree, e));
    let _ = (kt_parameter_is_context_parameter(tree, e), kt_parameter_is_lambda_parameter(tree, e));
    let _ = (kt_parameter_is_function_type_parameter(tree, e), kt_type_alias_is_top_level(tree, e));
    node(kt_type_parameter_get_extends_bound(tree, e));
    node(kt_type_constraint_get_subject_type_parameter_name(tree, e));
    node(kt_type_constraint_get_bound_type_reference(tree, e));
    nodes(&mut kt_destructuring_declaration_get_entries(tree, e));
    node(kt_type_reference_get_type_element(tree, e));
    let _ = kt_type_reference_has_parentheses(tree, e);
    node(kt_nullable_type_get_inner_type(tree, e));
    node(unwrap_nullability(tree, e));
    node(kt_user_type_get_reference_expression(tree, e));
    node(kt_user_type_get_qualifier(tree, e));
    nodes(&mut kt_user_type_get_type_arguments(tree, e));
    node(kt_function_type_get_receiver(tree, e));
    node(kt_function_type_get_parameter_list(tree, e));
    nodes(&mut kt_function_type_get_parameters(tree, e));
    node(kt_function_type_get_return_type_reference(tree, e));
    node(get_referenced_name_element(tree, e));
    let _ = get_referenced_name(tree, e);
    node(kt_import_directive_get_imported_reference(tree, e));
    node(kt_import_directive_get_alias(tree, e));
    let _ = kt_import_directive_get_alias_name(tree, e);
    let _ = kt_import_directive_is_all_under(tree, e);
    let _ = kt_import_directive_get_imported_fq_name(tree, e);
    node(kt_qualified_expression_get_receiver_expression(tree, e));
    node(kt_qualified_expression_get_selector_expression(tree, e));
    node(get_callee_expression(tree, e));
    node(get_value_argument_list(tree, e));
    nodes(&mut get_lambda_arguments(tree, e));
    nodes(&mut get_value_arguments(tree, e));
    node(get_type_argument_list(tree, e));
    node(kt_value_argument_get_argument_expression(tree, e));
    node(kt_value_argument_get_argument_name(tree, e));
    let _ = (kt_value_argument_is_named(tree, e), kt_value_argument_is_spread(tree, e));
    node(kt_lambda_argument_get_lambda_expression(tree, e));
    node(kt_lambda_expression_get_function_literal(tree, e));
}
