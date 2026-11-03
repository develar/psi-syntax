//! `java_syntax::psi`: each getter on the inputs where a consumer once read the tree differently from IntelliJ (the
//! answers are the live PSI's, `tools/java-oracle/JavaPsiOracle.java --getters`), every getter on every node of the
//! goldens and of random and deeply nested inputs without a panic, and the one-pass `field::declaration_heads` equal to
//! the per-field walk.

use java_syntax::psi::code_reference::Kind;
use java_syntax::psi::modifier::{Modifier, Modifiers};
use java_syntax::psi::type_element::TypeKind;
use java_syntax::psi::*;
use java_syntax::{FileKind, LanguageLevel, NodeId, NodeKind, TokenKind, Tree};

fn parse(text: &str) -> Tree {
    Tree::parse(text, FileKind::default())
}

fn text(tree: &Tree, id: NodeId) -> &str {
    tree.utf8_text(id).unwrap()
}

/// The nodes of the kind, in document order.
fn all(tree: &Tree, kind: NodeKind) -> Vec<NodeId> {
    tree.descendants(tree.root()).filter(|&n| tree.node_kind(n) == Some(kind)).collect()
}

/// The node of the kind whose name identifier is `name`.
fn named(tree: &Tree, kind: NodeKind, name: &str) -> NodeId {
    all(tree, kind)
        .into_iter()
        .find(|&n| get_name_identifier(tree, n).is_some_and(|i| text(tree, i) == name))
        .unwrap_or_else(|| panic!("no {kind:?} named {name}"))
}

/// The node of the kind whose text is `source`.
fn with_text(tree: &Tree, kind: NodeKind, source: &str) -> NodeId {
    all(tree, kind).into_iter().find(|&n| text(tree, n) == source).unwrap_or_else(|| panic!("no {kind:?} `{source}`"))
}

fn modifiers(tree: &Tree, owner: NodeId) -> Modifiers {
    Modifier::ALL.into_iter().filter(|&m| has_modifier_property(tree, owner, m)).fold(Modifiers::EMPTY, Modifiers::with)
}

#[test]
fn a_private_interface_method_is_private_and_not_public() {
    let tree = parse("interface I { private void h() {} void g(); default void d() {} static void s() {} }");
    use Modifier::*;
    assert_eq!(modifiers(&tree, named(&tree, NodeKind::METHOD, "h")), Modifiers::of(&[PRIVATE]));
    assert_eq!(modifiers(&tree, named(&tree, NodeKind::METHOD, "g")), Modifiers::of(&[PUBLIC, ABSTRACT]));
    assert_eq!(modifiers(&tree, named(&tree, NodeKind::METHOD, "d")), Modifiers::of(&[PUBLIC, DEFAULT]));
    assert_eq!(modifiers(&tree, named(&tree, NodeKind::METHOD, "s")), Modifiers::of(&[PUBLIC, STATIC]));
}

#[test]
fn a_stray_at_in_an_interface_body_is_no_annotation_type() {
    let tree = parse("interface I { @interface }\n@interface A { int v() default 1; }");
    let i = named(&tree, NodeKind::CLASS, "I");
    assert!(class::is_interface(&tree, i));
    assert!(!class::is_annotation_type(&tree, i));
    let a = named(&tree, NodeKind::CLASS, "A");
    assert!(class::is_interface(&tree, a) && class::is_annotation_type(&tree, a));
    let v = named(&tree, NodeKind::ANNOTATION_METHOD, "v");
    assert_eq!(modifiers(&tree, v), Modifiers::of(&[Modifier::PUBLIC, Modifier::ABSTRACT]));
}

#[test]
fn brackets_after_a_parameter_list_are_return_type_dimensions() {
    let tree = parse("class A { String n()[] { return null; } int[] m()[][] { return null; } A() {} }");
    assert_eq!(method::get_return_type_array_dimensions(&tree, named(&tree, NodeKind::METHOD, "n")), Some(1));
    assert_eq!(method::get_return_type_array_dimensions(&tree, named(&tree, NodeKind::METHOD, "m")), Some(3));
    let constructor = named(&tree, NodeKind::METHOD, "A");
    assert!(method::is_constructor(&tree, constructor));
    assert_eq!(method::get_return_type_element(&tree, constructor), None);
    assert_eq!(method::get_return_type_array_dimensions(&tree, constructor), None);
}

#[test]
fn an_import_with_a_trailing_dot_is_on_demand() {
    let tree =
        parse("import a.b.;\nimport a.b.*;\nimport a.B;\nimport static a.B.c;\nimport static a.B.*;\nimport module m;");
    let imports: Vec<NodeId> = tree
        .descendants(tree.root())
        .filter(|&n| {
            matches!(
                tree.node_kind(n),
                Some(
                    NodeKind::IMPORT_STATEMENT | NodeKind::IMPORT_STATIC_STATEMENT | NodeKind::IMPORT_MODULE_STATEMENT
                )
            )
        })
        .collect();
    let on_demand: Vec<bool> = imports.iter().map(|&i| import_statement::is_on_demand(&tree, i)).collect();
    assert_eq!(on_demand, [true, true, false, false, true, true]);
    let reference =
        |i: usize| import_statement::get_import_reference(&tree, imports[i]).map(|r| tree.node_kind(r).unwrap());
    assert_eq!(reference(2), Some(NodeKind::JAVA_CODE_REFERENCE));
    assert_eq!(reference(3), Some(NodeKind::IMPORT_STATIC_REFERENCE));
    assert_eq!(reference(4), Some(NodeKind::JAVA_CODE_REFERENCE));
    assert_eq!(reference(5), None);
    let static_reference = all(&tree, NodeKind::IMPORT_STATIC_REFERENCE)[0];
    let class_reference = import_static_reference::get_class_reference(&tree, static_reference).unwrap();
    assert_eq!(text(&tree, class_reference), "a.B");
    let member = import_static_reference::get_reference_name_element(&tree, static_reference).unwrap();
    assert_eq!(text(&tree, member), "c");
    let kind = |source: &str| {
        code_reference::get_kind_enum(&tree, with_text(&tree, NodeKind::JAVA_CODE_REFERENCE, source), false)
    };
    assert_eq!(kind("a.B"), Kind::CLASS_FQ_NAME_KIND);
}

#[test]
fn new_without_dimensions_or_initializer_is_no_array_creation() {
    let tree = parse(
        "class D { Object a = new int[]; Object b = new int[1]; Object c = new int[] { 1 }; Object d = new A[]; }",
    );
    let creation = |name: &str| {
        let field = named(&tree, NodeKind::FIELD, name);
        let new = get_initializer(&tree, field).unwrap();
        new_expression::is_array_creation(&tree, new)
    };
    assert_eq!([creation("a"), creation("b"), creation("c"), creation("d")], [false, true, true, false]);
    let d = get_initializer(&tree, named(&tree, NodeKind::FIELD, "d")).unwrap();
    assert_eq!(text(&tree, new_expression::get_class_reference(&tree, d).unwrap()), "A");
}

#[test]
fn an_error_element_in_an_argument_list_is_no_expression() {
    let tree = parse("class F { void m() { f(a,); } }");
    let list = all(&tree, NodeKind::EXPRESSION_LIST)[0];
    let kinds: Vec<NodeKind> =
        expression_list::get_expressions(&tree, list).map(|e| tree.node_kind(e).unwrap()).collect();
    assert_eq!(kinds, [NodeKind::REFERENCE_EXPRESSION, NodeKind::EMPTY_EXPRESSION]);
    assert!(tree.children(list).any(|c| tree.node_kind(c) == Some(NodeKind::ERROR_ELEMENT)));
}

#[test]
fn later_fields_read_the_first_fields_type_and_modifiers() {
    let tree = parse("class C { @Inject private int a = 1, b[], c; }");
    let [a, b, c] =
        [named(&tree, NodeKind::FIELD, "a"), named(&tree, NodeKind::FIELD, "b"), named(&tree, NodeKind::FIELD, "c")];
    let type_a = get_type_element(&tree, a).unwrap();
    assert_eq!(get_type_element(&tree, b), Some(type_a));
    assert_eq!(get_type_element(&tree, c), Some(type_a));
    assert_eq!(get_modifier_list(&tree, c), get_modifier_list(&tree, a));
    assert_eq!(field::get_type_array_dimensions(&tree, a), Some(0));
    assert_eq!(field::get_type_array_dimensions(&tree, b), Some(1));
    assert!(has_modifier_property(&tree, c, Modifier::PRIVATE));
    assert_eq!(text(&tree, get_initializer(&tree, a).unwrap()), "1");
    assert_eq!(get_initializer(&tree, b), None);
    let list = get_modifier_list(&tree, a).unwrap();
    let annotations: Vec<NodeId> = modifier_list::get_annotations(&tree, list).collect();
    assert_eq!(annotations.len(), 1);
    let name = annotation::get_name_reference_element(&tree, annotations[0]).unwrap();
    assert_eq!(java_source_util::get_reference_text(&tree, name), "Inject");
    let heads: Vec<(NodeId, NodeId)> =
        field::declaration_heads(&tree, get_containing_class(&tree, a).unwrap()).collect();
    assert_eq!(heads, [(a, a), (b, a), (c, a)]);
}

#[test]
fn later_locals_read_the_first_variable_of_their_statement() {
    let tree = parse("class C { void m() { final int p = 1, q[]; int x = ; } }");
    let [p, q] = [named(&tree, NodeKind::LOCAL_VARIABLE, "p"), named(&tree, NodeKind::LOCAL_VARIABLE, "q")];
    assert_eq!(get_type_element(&tree, q), get_type_element(&tree, p));
    assert_eq!(get_modifier_list(&tree, q), get_modifier_list(&tree, p));
    assert!(has_modifier_property(&tree, q, Modifier::FINAL));
    assert_eq!(local_variable::get_type_array_dimensions(&tree, q), Some(1));
    // `int x = ;`: the error element after `=` is no initializer.
    assert_eq!(get_initializer(&tree, named(&tree, NodeKind::LOCAL_VARIABLE, "x")), None);
}

#[test]
fn a_leading_type_annotation_is_no_qualifier() {
    let tree = parse("class C extends @A Base implements java.util.@T List<String> {}");
    let c = named(&tree, NodeKind::CLASS, "C");
    let extends: Vec<NodeId> =
        reference_list::get_reference_elements(&tree, class::get_extends_list(&tree, c).unwrap()).collect();
    assert_eq!(extends.len(), 1);
    assert_eq!(code_reference::get_qualifier(&tree, extends[0]), None);
    assert_eq!(text(&tree, code_reference::get_reference_name_element(&tree, extends[0]).unwrap()), "Base");
    let implements = class::get_implements_list(&tree, c).unwrap();
    let list = reference_list::get_reference_elements(&tree, implements).next().unwrap();
    assert_eq!(text(&tree, code_reference::get_qualifier(&tree, list).unwrap()), "java.util");
    assert!(code_reference::get_parameter_list(&tree, list).is_some());
    assert_eq!(java_source_util::get_reference_text(&tree, list), "java.util.List<String>");
    assert_eq!(code_reference::get_kind_enum(&tree, extends[0], false), Kind::CLASS_NAME_KIND);
    let java_util = code_reference::get_qualifier(&tree, list).unwrap();
    assert_eq!(code_reference::get_kind_enum(&tree, java_util, false), Kind::CLASS_OR_PACKAGE_NAME_KIND);
}

#[test]
fn a_delegating_call_is_named_by_its_keyword() {
    let tree = parse("class C { C(int x) { this(x, 1); } C() { super(); } void m() { a.<T>b(); c(); } }");
    let callee = |source: &str| {
        let call = with_text(&tree, NodeKind::METHOD_CALL_EXPRESSION, source);
        method_call_expression::get_method_expression(&tree, call).unwrap()
    };
    let this = callee("this(x, 1)");
    let name = reference_expression::get_reference_name_element(&tree, this).unwrap();
    assert_eq!(tree.token_kind(name), Some(TokenKind::THIS_KEYWORD));
    let sup = callee("super()");
    assert_eq!(
        tree.token_kind(reference_expression::get_reference_name_element(&tree, sup).unwrap()),
        Some(TokenKind::SUPER_KEYWORD)
    );
    let ab = callee("a.<T>b()");
    assert_eq!(text(&tree, reference_expression::get_qualifier_expression(&tree, ab).unwrap()), "a");
    assert_eq!(text(&tree, reference_expression::get_reference_name_element(&tree, ab).unwrap()), "b");
    let c = callee("c()");
    assert_eq!(reference_expression::get_qualifier_expression(&tree, c), None);
}

#[test]
fn method_references_and_qualified_new() {
    let tree = parse(
        "class C { Object a = String[]::new; Object b = Foo::bar; Object c = o.new I(); Object d = new A(1) { }; }",
    );
    let init = |name: &str| get_initializer(&tree, named(&tree, NodeKind::FIELD, name)).unwrap();
    let a = init("a");
    assert!(method_reference_expression::is_constructor(&tree, a));
    assert_eq!(tree.node_kind(method_reference_expression::get_qualifier(&tree, a).unwrap()), Some(NodeKind::TYPE));
    let b = init("b");
    assert!(!method_reference_expression::is_constructor(&tree, b));
    assert_eq!(text(&tree, method_reference_expression::get_reference_name_element(&tree, b).unwrap()), "bar");
    let c = init("c");
    assert_eq!(text(&tree, new_expression::get_qualifier(&tree, c).unwrap()), "o");
    let i = new_expression::get_class_reference(&tree, c).unwrap();
    assert_eq!(code_reference::get_kind_enum(&tree, i, false), Kind::CLASS_IN_QUALIFIED_NEW_KIND);
    let d = init("d");
    let anonymous = new_expression::get_anonymous_class(&tree, d).unwrap();
    let base = anonymous_class::get_base_class_reference(&tree, anonymous).unwrap();
    assert_eq!(new_expression::get_class_or_anonymous_class_reference(&tree, d), Some(base));
    assert_eq!(new_expression::get_argument_list(&tree, d), anonymous_class::get_argument_list(&tree, anonymous));
    assert_eq!(new_expression::get_class_reference(&tree, d), None);
    assert!(has_modifier_property(&tree, anonymous, Modifier::FINAL));
}

#[test]
fn enums_are_final_or_sealed_and_their_constants_public_static_final() {
    let tree = parse("enum E { A; void m() {} }\nenum F { A { }; abstract void m(); E() {} }\nclass O { enum N {} }");
    use Modifier::*;
    assert_eq!(modifiers(&tree, named(&tree, NodeKind::CLASS, "E")), Modifiers::of(&[PACKAGE_LOCAL, FINAL]));
    assert_eq!(modifiers(&tree, named(&tree, NodeKind::CLASS, "F")), Modifiers::of(&[PACKAGE_LOCAL, ABSTRACT, SEALED]));
    assert_eq!(modifiers(&tree, named(&tree, NodeKind::CLASS, "N")), Modifiers::of(&[PACKAGE_LOCAL, STATIC, FINAL]));
    assert_eq!(modifiers(&tree, all(&tree, NodeKind::ENUM_CONSTANT)[0]), Modifiers::of(&[PUBLIC, STATIC, FINAL]));
    let constructor = named(&tree, NodeKind::METHOD, "E");
    assert!(has_modifier_property(&tree, constructor, PRIVATE));
    let f_a = all(&tree, NodeKind::ENUM_CONSTANT)[1];
    let body = enum_constant::get_initializing_class(&tree, f_a).unwrap();
    assert!(modifiers(&tree, body).is_empty());
    assert_eq!(anonymous_class::get_base_class_reference(&tree, body), None);
}

#[test]
fn local_records_interfaces_and_classes() {
    let tree = parse("class C { void m() { class L {} record R() {} interface I {} } interface J { class K {} } }");
    use Modifier::*;
    assert_eq!(modifiers(&tree, named(&tree, NodeKind::CLASS, "L")), Modifiers::of(&[PACKAGE_LOCAL]));
    assert_eq!(modifiers(&tree, named(&tree, NodeKind::CLASS, "R")), Modifiers::of(&[PACKAGE_LOCAL, STATIC, FINAL]));
    assert_eq!(modifiers(&tree, named(&tree, NodeKind::CLASS, "I")), Modifiers::of(&[PACKAGE_LOCAL, STATIC, ABSTRACT]));
    assert_eq!(modifiers(&tree, named(&tree, NodeKind::CLASS, "K")), Modifiers::of(&[PUBLIC, STATIC]));
    assert_eq!(get_containing_class(&tree, named(&tree, NodeKind::CLASS, "L")), None);
    assert_eq!(
        get_containing_class(&tree, named(&tree, NodeKind::CLASS, "K")),
        Some(named(&tree, NodeKind::CLASS, "J"))
    );
}

#[test]
fn a_multi_catch_parameter_and_a_resource_are_final() {
    let tree =
        parse("class C { void m() { try (var r = open(); R s = open()) {} catch (A | B e) {} catch (A f) {} } }");
    assert!(has_modifier_property(&tree, named(&tree, NodeKind::PARAMETER, "e"), Modifier::FINAL));
    assert!(!has_modifier_property(&tree, named(&tree, NodeKind::PARAMETER, "f"), Modifier::FINAL));
    let e_type = get_type_element(&tree, named(&tree, NodeKind::PARAMETER, "e")).unwrap();
    assert_eq!(type_element::calculate_type(&tree, e_type).kind, TypeKind::Disjunction);
    let r = named(&tree, NodeKind::RESOURCE_VARIABLE, "r");
    assert!(has_modifier_property(&tree, r, Modifier::FINAL));
    assert!(type_element::is_inferred_type(&tree, get_type_element(&tree, r).unwrap()));
    assert_eq!(local_variable::get_type_array_dimensions(&tree, r), None);
    let s = named(&tree, NodeKind::RESOURCE_VARIABLE, "s");
    assert_ne!(get_type_element(&tree, s), get_type_element(&tree, r));
}

#[test]
fn array_types_and_varargs() {
    let tree = parse(
        "record R(int a[], String... b) { R { } }\nclass C { void f(int[]... xs, String... ys[], int z) {} List<? extends T> w; Object o = (A & B) x; }",
    );
    let xs = named(&tree, NodeKind::PARAMETER, "xs");
    let ty = type_element::calculate_type(&tree, get_type_element(&tree, xs).unwrap());
    assert_eq!((ty.kind, ty.array_dimensions, ty.ellipsis), (ty.kind, 2, true));
    assert!(matches!(ty.kind, TypeKind::Primitive(_)));
    assert!(parameter::is_var_args(&tree, xs));
    assert_eq!(parameter::get_type_array_dimensions(&tree, named(&tree, NodeKind::PARAMETER, "ys")), Some(2));
    assert!(!parameter::is_var_args(&tree, named(&tree, NodeKind::PARAMETER, "z")));
    assert!(!method::is_var_args(&tree, named(&tree, NodeKind::METHOD, "f")));
    assert_eq!(
        record_component::get_type_array_dimensions(&tree, named(&tree, NodeKind::RECORD_COMPONENT, "a")),
        Some(1)
    );
    assert!(record_component::is_var_args(&tree, named(&tree, NodeKind::RECORD_COMPONENT, "b")));
    let compact = named(&tree, NodeKind::METHOD, "R");
    assert!(method::is_compact_constructor(&tree, compact));
    assert_eq!(method::get_parameter_list(&tree, compact), None);
    assert!(method::is_var_args(&tree, compact));
    let w = get_type_element(&tree, named(&tree, NodeKind::FIELD, "w")).unwrap();
    let argument = all(&tree, NodeKind::TYPE).into_iter().find(|&t| text(&tree, t) == "? extends T").unwrap();
    assert_eq!(type_element::calculate_type(&tree, argument).kind, TypeKind::Wildcard);
    let list = type_element::get_innermost_component_reference_element(&tree, w).unwrap();
    assert_eq!(text(&tree, list), "List<? extends T>");
    assert_eq!(java_source_util::get_reference_text(&tree, list), "List<? extends T>");
    let cast = all(&tree, NodeKind::TYPE_CAST_EXPRESSION)[0];
    let cast_type = type_cast_expression::get_cast_type(&tree, cast).unwrap();
    assert_eq!(type_element::calculate_type(&tree, cast_type).kind, TypeKind::Intersection);
    assert_eq!(text(&tree, type_cast_expression::get_operand(&tree, cast).unwrap()), "x");
}

#[test]
fn package_names_and_reference_kinds() {
    let tree = parse("package a . /* c */ b;\nclass C { @ org . junit . Test void t() { Outer.this.x(); } }");
    let package = all(&tree, NodeKind::PACKAGE_STATEMENT)[0];
    assert_eq!(package_statement::get_package_name(&tree, package), "a.b");
    let reference = package_statement::get_package_reference(&tree, package).unwrap();
    assert_eq!(code_reference::get_kind_enum(&tree, reference, false), Kind::PACKAGE_NAME_KIND);
    let a = code_reference::get_qualifier(&tree, reference).unwrap();
    assert_eq!(code_reference::get_kind_enum(&tree, a, false), Kind::PACKAGE_NAME_KIND);
    let test = annotation::get_name_reference_element(&tree, all(&tree, NodeKind::ANNOTATION)[0]).unwrap();
    assert_eq!(java_source_util::get_reference_text(&tree, test), "org.junit.Test");
    assert_eq!(code_reference::get_kind_enum(&tree, test, false), Kind::CLASS_OR_PACKAGE_NAME_KIND);
    let this = all(&tree, NodeKind::THIS_EXPRESSION)[0];
    let outer = this_expression::get_qualifier(&tree, this).unwrap();
    assert_eq!(code_reference::get_kind_enum(&tree, outer, false), Kind::CLASS_NAME_KIND);
}

#[test]
fn a_doc_reference_of_a_package_info_below_java_8_is_fully_qualified() {
    let text = "/** {@link java.util.List} */\npackage a;";
    for (level, expected) in [
        (LanguageLevel::JDK_1_7, Kind::CLASS_FQ_OR_PACKAGE_NAME_KIND),
        (LanguageLevel::JDK_1_8, Kind::CLASS_OR_PACKAGE_NAME_KIND),
    ] {
        let tree = Tree::parse(text, FileKind::file(level));
        let list = all(&tree, NodeKind::JAVA_CODE_REFERENCE)
            .into_iter()
            .find(|&r| self::text(&tree, r) == "java.util.List")
            .unwrap();
        assert_eq!(code_reference::get_kind_enum(&tree, list, true), expected, "{level:?}");
        assert_eq!(code_reference::get_kind_enum(&tree, list, false), Kind::CLASS_OR_PACKAGE_NAME_KIND);
    }
}

#[test]
fn instanceof_patterns_and_lambdas() {
    let tree =
        parse("class C { void m() { if (o instanceof String s) {} if (o instanceof T) {} Runnable r = x -> x; } }");
    let [with_pattern, with_type] =
        [all(&tree, NodeKind::INSTANCE_OF_EXPRESSION)[0], all(&tree, NodeKind::INSTANCE_OF_EXPRESSION)[1]];
    assert_eq!(
        tree.node_kind(instance_of_expression::get_pattern(&tree, with_pattern).unwrap()),
        Some(NodeKind::TYPE_TEST_PATTERN)
    );
    assert_eq!(instance_of_expression::get_check_type(&tree, with_pattern), None);
    assert!(instance_of_expression::get_check_type(&tree, with_type).is_some());
    assert_eq!(text(&tree, instance_of_expression::get_operand(&tree, with_type).unwrap()), "o");
    let s = named(&tree, NodeKind::PATTERN_VARIABLE, "s");
    assert_eq!(text(&tree, get_type_element(&tree, s).unwrap()), "String");
    let lambda = all(&tree, NodeKind::LAMBDA_EXPRESSION)[0];
    let parameters = lambda_expression::get_parameter_list(&tree, lambda).unwrap();
    let x = parameter_list::get_parameters(&tree, parameters).next().unwrap();
    assert_eq!(get_type_element(&tree, x), None);
    assert_eq!(text(&tree, lambda_expression::get_body(&tree, lambda).unwrap()), "x");
}

/// The goldens of `testdata/java`: each line of the manifest is `PATH<TAB>LEVEL<TAB>ENTRY<TAB>TRIM`.
fn goldens() -> Vec<(String, Tree)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/java");
    let manifest = std::fs::read_to_string(root.join("manifest.tsv")).expect("read manifest.tsv");
    let mut goldens = Vec::new();
    for line in manifest.lines().filter(|l| !l.is_empty()) {
        let columns: Vec<&str> = line.split('\t').collect();
        let level = LanguageLevel::from_name(columns[1]).unwrap_or(LanguageLevel::HIGHEST);
        let entry = java_syntax::Entry::from_name(columns[2]).unwrap_or(java_syntax::Entry::File);
        let mut text: Vec<u8> = std::fs::read(root.join(columns[0])).expect("read golden");
        text.retain(|&b| b != b'\r');
        goldens.push((columns[0].to_string(), Tree::parse(&text, FileKind { level, entry })));
    }
    goldens
}

/// Every getter on every node (`getters::getters_to_string`), and `declaration_heads` against the per-field walk.
fn check_every_getter(tree: &Tree) {
    let _ = getters::getters_to_string(tree, false);
    for parent in tree.descendants(tree.root()) {
        for (f, head) in field::declaration_heads(tree, parent) {
            assert_eq!(head, field::find_first_field_in_declaration(tree, f));
        }
    }
}

#[test]
fn every_getter_answers_on_every_node_of_the_goldens() {
    let goldens = goldens();
    assert_eq!(goldens.len(), 1036);
    for (_, tree) in &goldens {
        check_every_getter(tree);
    }
}

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn every_getter_answers_on_random_inputs() {
    let pieces: &[&str] = &[
        "class ",
        "interface ",
        "enum ",
        "record ",
        "@interface ",
        "@",
        "A",
        "b",
        " ",
        "{",
        "}",
        "(",
        ")",
        "[",
        "]",
        "<",
        ">",
        ",",
        ";",
        ".",
        "...",
        "=",
        "?",
        "&",
        "|",
        "::",
        "->",
        "new ",
        "this",
        "super",
        "extends ",
        "implements ",
        "permits ",
        "throws ",
        "var ",
        "int ",
        "final ",
        "static ",
        "private ",
        "public ",
        "default ",
        "abstract ",
        "value ",
        "sealed ",
        "non-sealed ",
        "import ",
        "package ",
        "module ",
        "requires ",
        "catch ",
        "try ",
        "instanceof ",
        "/** {@link A} */",
        "1",
        "\"s\"",
        "*",
        "_",
    ];
    let mut rng = XorShift(0x2545F4914F6CDD1D);
    for _ in 0..3000 {
        let len = (rng.next() % 120) as usize;
        let mut text = String::new();
        for _ in 0..len {
            text.push_str(pieces[(rng.next() % pieces.len() as u64) as usize]);
        }
        check_every_getter(&parse(&text));
    }
}

#[test]
fn deep_and_long_inputs_take_linear_time() {
    let n = 30_000;
    let names: Vec<String> = (0..n).map(|i| format!("a{i}")).collect();
    let cases = [
        // One declaration of n fields: `declaration_heads` is one pass.
        format!("class C {{ @A private int {}; }}", names.join(", ")),
        // n nested array levels, an initializer n deep, a qualified name of n segments.
        format!("class C {{ int{} x; }}", "[]".repeat(1000)),
        format!("class C {{ Object x = new int[][] {}1{}; }}", "{".repeat(n), "}".repeat(n)),
        format!("class C {{ {} x; }}", names[..2000].join(".")),
    ];
    for text in cases {
        let start = std::time::Instant::now();
        let tree =
            std::thread::Builder::new().stack_size(64 << 20).spawn(move || parse(&text)).unwrap().join().unwrap();
        let class = all(&tree, NodeKind::CLASS)[0];
        let heads = field::declaration_heads(&tree, class).count();
        let types = all(&tree, NodeKind::TYPE)
            .into_iter()
            .map(|t| type_element::calculate_type(&tree, t).array_dimensions)
            .max();
        let _ = getters::getters_to_string(&tree, false);
        assert!(start.elapsed().as_secs() < 30, "{heads} fields, deepest type {types:?}: {:?}", start.elapsed());
    }
}
