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
    let sources = common::kotlin_files(&dir);
    let mut failures = Vec::new();
    for source in &sources {
        let name = common::path_name(source);
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
        let name = common::path_name(&source);
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

/// A modifier list whose parent is no `KtModifierListOwner` belongs to no owner: the dangling `private @A` left in a
/// class body or at the end of the file, and the `companion` of a companion block (`KtClassBody`, `KtCompanionBlock`
/// and `KtFile` have no modifier list), so the modifier getters of the node that holds it answer nothing. The oracle
/// prints those getters only for owners, so this is checked here.
#[test]
fn a_dangling_modifier_list_has_no_owner() {
    use kotlin_syntax::NodeKind::{CLASS_BODY, COMPANION_BLOCK, KT_FILE, MODIFIER_LIST};
    use kotlin_syntax::psi::*;
    use kotlin_syntax::tokens::TokenKind::{COMPANION_KEYWORD, PRIVATE_KEYWORD};
    let text = "class C {\n    companion {\n        private @A\n    }\n    private @B\n}\nprivate @C\n";
    let tree = Tree::parse(text, FileKind::Kt);
    let mut holders = Vec::new();
    for id in tree.descendants(tree.root()) {
        if tree.node_kind(id) != Some(MODIFIER_LIST) {
            continue;
        }
        let parent = tree.parent(id).expect("a modifier list has a parent");
        holders.push(tree.node_kind(parent));
        assert_eq!(get_modifier_list(&tree, parent), None);
        assert!(!has_modifier(&tree, parent, PRIVATE_KEYWORD));
        assert!(!has_modifier(&tree, parent, COMPANION_KEYWORD));
        assert!(!is_private(&tree, parent));
        assert_eq!(visibility_modifier_type(&tree, parent), None);
        assert_eq!(modality_modifier(&tree, parent), None);
        if tree.node_kind(parent) != Some(KT_FILE) {
            assert_eq!(get_annotation_entries(&tree, parent).count(), 0);
        }
        // The list itself still answers as a `KtModifierList`.
        let companion = tree.node_kind(parent) == Some(COMPANION_BLOCK);
        assert!(kt_modifier_list_has_modifier(&tree, id, if companion { COMPANION_KEYWORD } else { PRIVATE_KEYWORD }));
        assert_eq!(kt_modifier_list_get_annotation_entries(&tree, id).count(), usize::from(!companion));
    }
    assert_eq!(holders, [Some(COMPANION_BLOCK), Some(CLASS_BODY), Some(CLASS_BODY), Some(KT_FILE)]);
}

/// A `kt_<class>_` method answers nothing for a node of another class (Java's types would not let it be called
/// there), on every node of every golden: `kt_parameter_get_default_value` of a property is not its initializer,
/// `kt_property_accessor_is_getter` of the `get` of `@get:A` is false. Each also answers on a node of its own class
/// somewhere, so the check is not vacuous.
#[test]
fn a_class_method_answers_nothing_on_another_class() {
    use kotlin_syntax::NodeKind::{self, *};
    use kotlin_syntax::psi::*;
    use kotlin_syntax::tokens::TokenKind::{DATA_KEYWORD, PRIVATE_KEYWORD};
    type Check = (&'static str, &'static [NodeKind], fn(&Tree, kotlin_syntax::NodeId) -> bool);
    const CO: &[NodeKind] = &[CLASS, ENUM_ENTRY, OBJECT_DECLARATION];
    const KT_CLASS: &[NodeKind] = &[CLASS, ENUM_ENTRY];
    const ENTRY: &[NodeKind] = &[DELEGATED_SUPER_TYPE_ENTRY, SUPER_TYPE_CALL_ENTRY, SUPER_TYPE_ENTRY];
    const ARGUMENT: &[NodeKind] = &[VALUE_ARGUMENT, LAMBDA_ARGUMENT];
    const QUALIFIED: &[NodeKind] = &[DOT_QUALIFIED_EXPRESSION, SAFE_ACCESS_EXPRESSION];
    let checks: &[Check] = &[
        ("kt_modifier_list_has_modifier", &[MODIFIER_LIST], |t, e| {
            kt_modifier_list_has_modifier(t, e, PRIVATE_KEYWORD)
        }),
        ("kt_modifier_list_get_modifier", &[MODIFIER_LIST], |t, e| {
            kt_modifier_list_get_modifier(t, e, DATA_KEYWORD).is_some()
        }),
        ("kt_modifier_list_get_annotations", &[MODIFIER_LIST], |t, e| {
            kt_modifier_list_get_annotations(t, e).next().is_some()
        }),
        ("kt_modifier_list_get_annotation_entries", &[MODIFIER_LIST], |t, e| {
            kt_modifier_list_get_annotation_entries(t, e).next().is_some()
        }),
        ("kt_modifier_list_visibility_modifier", &[MODIFIER_LIST], |t, e| {
            kt_modifier_list_visibility_modifier(t, e).is_some()
        }),
        ("kt_file_get_file_annotation_list", &[KT_FILE], |t, e| kt_file_get_file_annotation_list(t, e).is_some()),
        ("kt_annotation_get_entries", &[ANNOTATION], |t, e| kt_annotation_get_entries(t, e).next().is_some()),
        ("kt_annotation_get_use_site_target", &[ANNOTATION], |t, e| kt_annotation_get_use_site_target(t, e).is_some()),
        ("kt_annotation_entry_get_type_reference", &[ANNOTATION_ENTRY], |t, e| {
            kt_annotation_entry_get_type_reference(t, e).is_some()
        }),
        ("kt_annotation_entry_get_at_symbol", &[ANNOTATION_ENTRY], |t, e| {
            kt_annotation_entry_get_at_symbol(t, e).is_some()
        }),
        ("kt_annotation_entry_get_use_site_target", &[ANNOTATION_ENTRY], |t, e| {
            kt_annotation_entry_get_use_site_target(t, e).is_some()
        }),
        ("kt_annotation_entry_get_short_name", &[ANNOTATION_ENTRY], |t, e| {
            kt_annotation_entry_get_short_name(t, e).is_some()
        }),
        ("kt_annotation_use_site_target_get_annotation_use_site_target", &[ANNOTATION_TARGET], |t, e| {
            kt_annotation_use_site_target_get_annotation_use_site_target(t, e).is_some()
        }),
        ("kt_constructor_get_containing_class_or_object", &[PRIMARY_CONSTRUCTOR, SECONDARY_CONSTRUCTOR], |t, e| {
            kt_constructor_get_containing_class_or_object(t, e).is_some()
        }),
        ("kt_class_body_containing_class_or_object", &[CLASS_BODY], |t, e| {
            kt_class_body_containing_class_or_object(t, e).is_some()
        }),
        ("kt_class_or_object_is_top_level", CO, kt_class_or_object_is_top_level),
        ("kt_class_or_object_is_data", CO, kt_class_or_object_is_data),
        ("kt_class_or_object_is_annotation", CO, kt_class_or_object_is_annotation),
        ("kt_class_or_object_get_declaration_keyword", CO, |t, e| {
            kt_class_or_object_get_declaration_keyword(t, e).is_some()
        }),
        ("kt_class_or_object_get_super_type_list", CO, |t, e| kt_class_or_object_get_super_type_list(t, e).is_some()),
        ("kt_class_or_object_get_super_type_list_entries", CO, |t, e| {
            kt_class_or_object_get_super_type_list_entries(t, e).next().is_some()
        }),
        ("kt_class_or_object_get_body", CO, |t, e| kt_class_or_object_get_body(t, e).is_some()),
        ("kt_class_or_object_get_primary_constructor", CO, |t, e| {
            kt_class_or_object_get_primary_constructor(t, e).is_some()
        }),
        ("kt_class_or_object_get_primary_constructor_parameters", CO, |t, e| {
            kt_class_or_object_get_primary_constructor_parameters(t, e).next().is_some()
        }),
        ("kt_class_or_object_get_secondary_constructors", CO, |t, e| {
            kt_class_or_object_get_secondary_constructors(t, e).next().is_some()
        }),
        ("kt_super_type_list_get_entries", &[SUPER_TYPE_LIST, INITIALIZER_LIST], |t, e| {
            kt_super_type_list_get_entries(t, e).next().is_some()
        }),
        ("kt_class_is_interface", KT_CLASS, kt_class_is_interface),
        ("kt_class_is_enum", KT_CLASS, kt_class_is_enum),
        ("kt_class_is_sealed", KT_CLASS, kt_class_is_sealed),
        ("kt_class_is_inner", KT_CLASS, kt_class_is_inner),
        ("kt_class_is_inline", KT_CLASS, kt_class_is_inline),
        ("kt_class_is_value", KT_CLASS, kt_class_is_value),
        ("kt_class_get_fun_keyword", KT_CLASS, |t, e| kt_class_get_fun_keyword(t, e).is_some()),
        ("kt_object_declaration_is_companion", &[OBJECT_DECLARATION], kt_object_declaration_is_companion),
        ("kt_object_declaration_is_object_literal", &[OBJECT_DECLARATION], kt_object_declaration_is_object_literal),
        ("kt_object_declaration_get_object_keyword", &[OBJECT_DECLARATION], |t, e| {
            kt_object_declaration_get_object_keyword(t, e).is_some()
        }),
        ("kt_super_type_list_entry_get_type_as_user_type", ENTRY, |t, e| {
            kt_super_type_list_entry_get_type_as_user_type(t, e).is_some()
        }),
        ("kt_named_function_is_local", &[FUN], kt_named_function_is_local),
        ("kt_named_function_is_anonymous", &[FUN], kt_named_function_is_anonymous),
        ("kt_named_function_is_top_level", &[FUN], kt_named_function_is_top_level),
        ("kt_property_is_top_level", &[PROPERTY], kt_property_is_top_level),
        ("kt_property_is_member", &[PROPERTY], kt_property_is_member),
        ("kt_property_is_local", &[PROPERTY], kt_property_is_local),
        ("kt_property_get_delegate_expression", &[PROPERTY], |t, e| {
            kt_property_get_delegate_expression(t, e).is_some()
        }),
        ("kt_property_get_accessors", &[PROPERTY], |t, e| kt_property_get_accessors(t, e).next().is_some()),
        ("kt_property_accessor_is_getter", &[PROPERTY_ACCESSOR], kt_property_accessor_is_getter),
        ("kt_property_accessor_is_setter", &[PROPERTY_ACCESSOR], kt_property_accessor_is_setter),
        ("kt_parameter_get_default_value", &[VALUE_PARAMETER], |t, e| kt_parameter_get_default_value(t, e).is_some()),
        ("kt_parameter_is_mutable", &[VALUE_PARAMETER], kt_parameter_is_mutable),
        ("kt_parameter_is_var_arg", &[VALUE_PARAMETER], kt_parameter_is_var_arg),
        ("kt_parameter_has_val_or_var", &[VALUE_PARAMETER], kt_parameter_has_val_or_var),
        ("kt_parameter_get_destructuring_declaration", &[VALUE_PARAMETER], |t, e| {
            kt_parameter_get_destructuring_declaration(t, e).is_some()
        }),
        ("kt_parameter_is_loop_parameter", &[VALUE_PARAMETER], kt_parameter_is_loop_parameter),
        ("kt_parameter_is_catch_parameter", &[VALUE_PARAMETER], kt_parameter_is_catch_parameter),
        ("kt_parameter_is_context_parameter", &[VALUE_PARAMETER], kt_parameter_is_context_parameter),
        ("kt_parameter_is_lambda_parameter", &[VALUE_PARAMETER], kt_parameter_is_lambda_parameter),
        ("kt_parameter_is_function_type_parameter", &[VALUE_PARAMETER], kt_parameter_is_function_type_parameter),
        ("kt_type_alias_is_top_level", &[TYPEALIAS], kt_type_alias_is_top_level),
        ("kt_type_parameter_get_extends_bound", &[TYPE_PARAMETER], |t, e| {
            kt_type_parameter_get_extends_bound(t, e).is_some()
        }),
        ("kt_type_constraint_get_subject_type_parameter_name", &[TYPE_CONSTRAINT], |t, e| {
            kt_type_constraint_get_subject_type_parameter_name(t, e).is_some()
        }),
        ("kt_type_constraint_get_bound_type_reference", &[TYPE_CONSTRAINT], |t, e| {
            kt_type_constraint_get_bound_type_reference(t, e).is_some()
        }),
        ("kt_destructuring_declaration_get_entries", &[DESTRUCTURING_DECLARATION], |t, e| {
            kt_destructuring_declaration_get_entries(t, e).next().is_some()
        }),
        ("kt_type_reference_get_type_element", &[TYPE_REFERENCE], |t, e| {
            kt_type_reference_get_type_element(t, e).is_some()
        }),
        ("kt_type_reference_has_parentheses", &[TYPE_REFERENCE], kt_type_reference_has_parentheses),
        ("kt_nullable_type_get_inner_type", &[NULLABLE_TYPE], |t, e| kt_nullable_type_get_inner_type(t, e).is_some()),
        ("unwrap_nullability", &[USER_TYPE, NULLABLE_TYPE, FUNCTION_TYPE, DYNAMIC_TYPE, INTERSECTION_TYPE], |t, e| {
            unwrap_nullability(t, e).is_some()
        }),
        ("kt_user_type_get_reference_expression", &[USER_TYPE], |t, e| {
            kt_user_type_get_reference_expression(t, e).is_some()
        }),
        ("kt_user_type_get_qualifier", &[USER_TYPE], |t, e| kt_user_type_get_qualifier(t, e).is_some()),
        ("kt_user_type_get_type_arguments", &[USER_TYPE], |t, e| {
            kt_user_type_get_type_arguments(t, e).next().is_some()
        }),
        ("kt_function_type_get_receiver", &[FUNCTION_TYPE], |t, e| kt_function_type_get_receiver(t, e).is_some()),
        ("kt_function_type_get_parameters", &[FUNCTION_TYPE], |t, e| {
            kt_function_type_get_parameters(t, e).next().is_some()
        }),
        ("kt_function_type_get_return_type_reference", &[FUNCTION_TYPE], |t, e| {
            kt_function_type_get_return_type_reference(t, e).is_some()
        }),
        ("kt_import_directive_get_imported_reference", &[IMPORT_DIRECTIVE], |t, e| {
            kt_import_directive_get_imported_reference(t, e).is_some()
        }),
        ("kt_import_directive_get_alias_name", &[IMPORT_DIRECTIVE], |t, e| {
            kt_import_directive_get_alias_name(t, e).is_some()
        }),
        ("kt_import_directive_is_all_under", &[IMPORT_DIRECTIVE], kt_import_directive_is_all_under),
        ("kt_import_directive_get_imported_fq_name", &[IMPORT_DIRECTIVE], |t, e| {
            kt_import_directive_get_imported_fq_name(t, e).is_some()
        }),
        ("kt_qualified_expression_get_receiver_expression", QUALIFIED, |t, e| {
            kt_qualified_expression_get_receiver_expression(t, e).is_some()
        }),
        ("kt_qualified_expression_get_selector_expression", QUALIFIED, |t, e| {
            kt_qualified_expression_get_selector_expression(t, e).is_some()
        }),
        ("kt_value_argument_get_argument_expression", ARGUMENT, |t, e| {
            kt_value_argument_get_argument_expression(t, e).is_some()
        }),
        ("kt_value_argument_is_named", ARGUMENT, kt_value_argument_is_named),
        ("kt_value_argument_is_spread", ARGUMENT, kt_value_argument_is_spread),
        ("kt_lambda_argument_get_lambda_expression", &[LAMBDA_ARGUMENT], |t, e| {
            kt_lambda_argument_get_lambda_expression(t, e).is_some()
        }),
        ("kt_lambda_expression_get_function_literal", &[LAMBDA_EXPRESSION], |t, e| {
            kt_lambda_expression_get_function_literal(t, e).is_some()
        }),
        (
            "containing_class_or_object",
            &[
                BACKING_FIELD,
                CLASS,
                CLASS_INITIALIZER,
                DESTRUCTURING_DECLARATION,
                DESTRUCTURING_DECLARATION_ENTRY,
                ENUM_ENTRY,
                FUN,
                FUNCTION_LITERAL,
                OBJECT_DECLARATION,
                PRIMARY_CONSTRUCTOR,
                PROPERTY,
                PROPERTY_ACCESSOR,
                SCRIPT,
                SCRIPT_INITIALIZER,
                SECONDARY_CONSTRUCTOR,
                TYPEALIAS,
                TYPE_PARAMETER,
                VALUE_PARAMETER,
            ],
            |t, e| containing_class_or_object(t, e).is_some(),
        ),
        (
            "containing_script",
            &[
                BACKING_FIELD,
                CLASS,
                CLASS_INITIALIZER,
                DESTRUCTURING_DECLARATION,
                DESTRUCTURING_DECLARATION_ENTRY,
                ENUM_ENTRY,
                FUN,
                FUNCTION_LITERAL,
                OBJECT_DECLARATION,
                PRIMARY_CONSTRUCTOR,
                PROPERTY,
                PROPERTY_ACCESSOR,
                SCRIPT,
                SCRIPT_INITIALIZER,
                SECONDARY_CONSTRUCTOR,
                TYPEALIAS,
                TYPE_PARAMETER,
                VALUE_PARAMETER,
            ],
            |t, e| containing_script(t, e).is_some(),
        ),
    ];
    let mut answered = vec![false; checks.len()];
    for golden in common::goldens() {
        let text = std::fs::read(&golden.source).expect("read the golden");
        let tree = Tree::parse(&text, FileKind::from_file_name(&golden.name));
        for id in tree.descendants(tree.root()) {
            let Some(kind) = tree.node_kind(id) else { continue };
            for (i, (name, kinds, answers)) in checks.iter().enumerate() {
                let answer = answers(&tree, id);
                if kinds.contains(&kind) {
                    answered[i] |= answer;
                } else {
                    assert!(!answer, "{name} answers on a {kind:?} of {}", golden.name);
                }
            }
        }
    }
    let silent: Vec<&str> = checks.iter().zip(&answered).filter(|(_, a)| !**a).map(|(c, _)| c.0).collect();
    assert!(silent.is_empty(), "never answer on their own class in the goldens: {silent:?}");
}
