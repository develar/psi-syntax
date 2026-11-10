//! `csharp_syntax::syntax_getters` and `csharp_syntax::read`: the getters a consumer reads declarations, names and
//! references by (each answer Roslyn's, `tools/csharp-oracle/oracle.sh getters`), every getter on every node of the
//! goldens, of random bytes and fragments and of deep and long inputs without a panic, on a small stack and in linear
//! time; every child of every node in a slot; the `_of` dispatchers equal to the fields they dispatch to; `read::span`
//! equal to the one-pass spans `getters` prints.

mod common;

use csharp_syntax::read;
use csharp_syntax::syntax_getters::getters::getters_to_string;
use csharp_syntax::syntax_getters::*;
use csharp_syntax::{
    DocumentationMode, Entry, FileKind, LanguageVersion, NodeId, NodeKind, Parser, TokenKind, Tree, syntax,
};
use psi_check::{Rng, limit};

// jscpd:ignore-start: shorthands over C#'s own `Tree`; the Java tests' read alike over theirs, which shares no type
fn parse(text: &str) -> Tree {
    Tree::parse(text, FileKind::default())
}

fn text(tree: &Tree, id: NodeId) -> &str {
    tree.utf8_text(id).unwrap()
}
// jscpd:ignore-end

/// The text of `Span` (`ToString()`).
fn spanned(tree: &Tree, id: NodeId) -> &str {
    std::str::from_utf8(&tree.source()[read::span(tree, id)]).unwrap()
}

fn all(tree: &Tree, kind: NodeKind) -> Vec<NodeId> {
    tree.descendants(tree.root()).filter(|&n| tree.node_kind(n) == Some(kind)).collect()
}

/// The declaration of `kind` whose identifier (`identifier_of`) is `name`.
fn named(tree: &Tree, kind: NodeKind, name: &str) -> NodeId {
    all(tree, kind)
        .into_iter()
        .find(|&n| identifier_of(tree, n).is_some_and(|i| text(tree, i) == name))
        .unwrap_or_else(|| panic!("no {kind:?} named {name}"))
}

fn texts(tree: &Tree, ids: impl Iterator<Item = NodeId>) -> Vec<String> {
    ids.map(|i| spanned(tree, i).to_string()).collect()
}

#[test]
fn a_class_reads_as_roslyn_s_class_declaration() {
    let tree = parse(
        "/// doc\n[A, B(1)] [C] public sealed partial class Foo<T, U> : Base<T>, IBar where T : class where U : new()\n\
         { int f; void M() { } }",
    );
    let class = named(&tree, NodeKind::ClassDeclaration, "Foo");
    assert_eq!(texts(&tree, class_declaration::attribute_lists(&tree, class)), ["[A, B(1)]", "[C]"]);
    assert_eq!(texts(&tree, class_declaration::modifiers(&tree, class)), ["public", "sealed", "partial"]);
    assert_eq!(text(&tree, class_declaration::keyword(&tree, class).unwrap()), "class");
    assert_eq!(spanned(&tree, class_declaration::type_parameter_list(&tree, class).unwrap()), "<T, U>");
    assert_eq!(class_declaration::parameter_list(&tree, class), None);
    let base = class_declaration::base_list(&tree, class).unwrap();
    let types: Vec<NodeId> = base_list::types(&tree, base).collect();
    assert_eq!(texts(&tree, types.iter().map(|&t| base_type::type_(&tree, t).unwrap())), ["Base<T>", "IBar"]);
    assert_eq!(class_declaration::constraint_clauses(&tree, class).count(), 2);
    assert_eq!(texts(&tree, type_declaration::members(&tree, class)), ["int f;", "void M() { }"]);
    assert_eq!(class_declaration::semicolon_token(&tree, class), None);
    assert_eq!(type_declaration::arity(&tree, class), 2);
    assert!(has_modifier(&tree, class, TokenKind::SealedKeyword));
    assert!(!has_modifier(&tree, class, TokenKind::StaticKeyword));
    // `Span` starts at the first attribute, past the doc comment (structured trivia of the `[`)
    assert!(spanned(&tree, class).starts_with("[A, B(1)]"));
    // the abstract classes' getters and the dispatchers answer the same nodes
    assert_eq!(member_declaration::modifiers(&tree, class).count(), 3);
    assert_eq!(base_type_declaration::identifier(&tree, class), class_declaration::identifier(&tree, class));
    assert_eq!(type_parameter_list_of(&tree, class), class_declaration::type_parameter_list(&tree, class));
    // a getter of another type answers none
    assert_eq!(struct_declaration::identifier(&tree, class), None);
    assert_eq!(class_declaration::identifier(&tree, tree.root()), None);
}

#[test]
fn members_read_their_types_names_parameters_and_bodies() {
    let tree = parse(
        "class C {\n\
           public static T M<T>(this int x, ref int y, params int[] z) where T : struct => default;\n\
           C(int a) : base(a) { }\n\
           ~C() { }\n\
           public static C operator +(C a, C b) => a;\n\
           public static implicit operator int(C c) => 0;\n\
           int I.P { get => 1; set { } }\n\
           public int Q { get; private set; } = 3;\n\
           int this[int i] => i;\n\
           event System.Action E { add { } remove { } }\n\
           event System.Action F, G;\n\
           private int f, g = 2;\n\
           void Varargs(__arglist) { }\n\
         }\n\
         enum E : byte { X = 1, Y, }\n\
         delegate R D<in A, out R>(A a);",
    );
    let m = named(&tree, NodeKind::MethodDeclaration, "M");
    assert_eq!(spanned(&tree, method_declaration::return_type(&tree, m).unwrap()), "T");
    assert_eq!(method_declaration::arity(&tree, m), 1);
    let parameters = base_method_declaration::parameter_list(&tree, m).unwrap();
    let params: Vec<NodeId> = parameter_list::parameters(&tree, parameters).collect();
    assert_eq!(texts(&tree, params.iter().copied()), ["this int x", "ref int y", "params int[] z"]);
    assert!(has_modifier(&tree, params[0], TokenKind::ThisKeyword));
    assert!(!has_modifier(&tree, params[1], TokenKind::ThisKeyword));
    assert_eq!(spanned(&tree, parameter::type_(&tree, params[2]).unwrap()), "int[]");
    assert_eq!(text(&tree, parameter::identifier(&tree, params[2]).unwrap()), "z");
    assert_eq!(method_declaration::body(&tree, m), None);
    assert_eq!(spanned(&tree, expression_body_of(&tree, m).unwrap()), "=> default");
    assert!(method_declaration::semicolon_token(&tree, m).is_some());
    let ctor = named(&tree, NodeKind::ConstructorDeclaration, "C");
    assert_eq!(spanned(&tree, constructor_declaration::initializer(&tree, ctor).unwrap()), ": base(a)");
    assert_eq!(spanned(&tree, body_of(&tree, ctor).unwrap()), "{ }");
    assert!(destructor_declaration::tilde_token(&tree, all(&tree, NodeKind::DestructorDeclaration)[0]).is_some());
    let op = all(&tree, NodeKind::OperatorDeclaration)[0];
    assert_eq!(text(&tree, operator_declaration::operator_token(&tree, op).unwrap()), "+");
    let conversion = all(&tree, NodeKind::ConversionOperatorDeclaration)[0];
    assert_eq!(
        text(&tree, conversion_operator_declaration::implicit_or_explicit_keyword(&tree, conversion).unwrap()),
        "implicit"
    );
    assert_eq!(spanned(&tree, conversion_operator_declaration::type_(&tree, conversion).unwrap()), "int");
    let p = named(&tree, NodeKind::PropertyDeclaration, "P");
    assert_eq!(spanned(&tree, base_property_declaration::explicit_interface_specifier(&tree, p).unwrap()), "I.");
    let accessors = accessor_list::accessors(&tree, base_property_declaration::accessor_list(&tree, p).unwrap());
    let accessors: Vec<NodeId> = accessors.collect();
    assert_eq!(text(&tree, accessor_declaration::keyword(&tree, accessors[0]).unwrap()), "get");
    assert_eq!(spanned(&tree, accessor_declaration::expression_body(&tree, accessors[0]).unwrap()), "=> 1");
    assert_eq!(spanned(&tree, accessor_declaration::body(&tree, accessors[1]).unwrap()), "{ }");
    let q = named(&tree, NodeKind::PropertyDeclaration, "Q");
    assert_eq!(spanned(&tree, property_declaration::initializer(&tree, q).unwrap()), "= 3");
    let indexer = all(&tree, NodeKind::IndexerDeclaration)[0];
    assert_eq!(spanned(&tree, indexer_declaration::parameter_list(&tree, indexer).unwrap()), "[int i]");
    let event = named(&tree, NodeKind::EventDeclaration, "E");
    assert_eq!(spanned(&tree, event_declaration::type_(&tree, event).unwrap()), "System.Action");
    let event_field = all(&tree, NodeKind::EventFieldDeclaration)[0];
    let declaration = base_field_declaration::declaration(&tree, event_field).unwrap();
    let names: Vec<&str> = variable_declaration::variables(&tree, declaration)
        .map(|v| text(&tree, variable_declarator::identifier(&tree, v).unwrap()))
        .collect();
    assert_eq!(names, ["F", "G"]);
    let field = all(&tree, NodeKind::FieldDeclaration)[0];
    let declaration = field_declaration::declaration(&tree, field).unwrap();
    assert_eq!(spanned(&tree, variable_declaration::type_(&tree, declaration).unwrap()), "int");
    let g = variable_declaration::variables(&tree, declaration).nth(1).unwrap();
    assert_eq!(spanned(&tree, variable_declarator::initializer(&tree, g).unwrap()), "= 2");
    let varargs = named(&tree, NodeKind::MethodDeclaration, "Varargs");
    let list = method_declaration::parameter_list(&tree, varargs).unwrap();
    let arglist = parameter_list::parameters(&tree, list).next().unwrap();
    assert!(parameter::is_arg_list(&tree, arglist));
    assert_eq!(parameter_list::parameter_count(&tree, list), 0);
    let e = named(&tree, NodeKind::EnumDeclaration, "E");
    let members: Vec<NodeId> = enum_declaration::members(&tree, e).collect();
    assert_eq!(texts(&tree, members.iter().copied()), ["X = 1", "Y"]);
    assert_eq!(spanned(&tree, enum_member_declaration::equals_value(&tree, members[0]).unwrap()), "= 1");
    assert_eq!(members_of(&tree, e).count(), 2);
    let d = named(&tree, NodeKind::DelegateDeclaration, "D");
    assert_eq!(delegate_declaration::arity(&tree, d), 2);
    assert_eq!(spanned(&tree, delegate_declaration::return_type(&tree, d).unwrap()), "R");
    let variance: Vec<&str> =
        type_parameter_list::parameters(&tree, delegate_declaration::type_parameter_list(&tree, d).unwrap())
            .map(|t| text(&tree, type_parameter::variance_keyword(&tree, t).unwrap()))
            .collect();
    assert_eq!(variance, ["in", "out"]);
}

#[test]
fn records_extension_blocks_and_primary_constructors() {
    let tree = parse(
        "record R(int X) : Base(X), I;\n\
         record struct S(int Y);\n\
         class P(int z) : Base(z) { }\n\
         static class Ext { extension<T>(T[] items) where T : class { public int Count => items.Length; } }",
    );
    let r = named(&tree, NodeKind::RecordDeclaration, "R");
    assert_eq!(text(&tree, record_declaration::keyword(&tree, r).unwrap()), "record");
    assert_eq!(record_declaration::class_or_struct_keyword(&tree, r), None);
    assert_eq!(spanned(&tree, type_declaration::parameter_list(&tree, r).unwrap()), "(int X)");
    let base = type_declaration::primary_constructor_base_type_if_class(&tree, r).unwrap();
    assert_eq!(spanned(&tree, base), "Base(X)");
    assert_eq!(spanned(&tree, primary_constructor_base_type::argument_list(&tree, base).unwrap()), "(X)");
    let s = named(&tree, NodeKind::RecordStructDeclaration, "S");
    assert_eq!(text(&tree, record_declaration::class_or_struct_keyword(&tree, s).unwrap()), "struct");
    // a record struct is no class: Roslyn's `PrimaryConstructorBaseTypeIfClass` is null for it
    assert_eq!(type_declaration::primary_constructor_base_type_if_class(&tree, s), None);
    let p = named(&tree, NodeKind::ClassDeclaration, "P");
    assert_eq!(spanned(&tree, parameter_list_of(&tree, p).unwrap()), "(int z)");
    let block = all(&tree, NodeKind::ExtensionBlockDeclaration)[0];
    // `ExtensionBlockDeclarationSyntax.Identifier => default`, `BaseList => null`
    assert_eq!(type_declaration::identifier(&tree, block), None);
    assert_eq!(type_declaration::base_list(&tree, block), None);
    assert_eq!(text(&tree, type_declaration::keyword(&tree, block).unwrap()), "extension");
    assert_eq!(spanned(&tree, type_declaration::parameter_list(&tree, block).unwrap()), "(T[] items)");
    assert_eq!(type_declaration::arity(&tree, block), 1);
    assert_eq!(type_declaration::constraint_clauses(&tree, block).count(), 1);
    assert_eq!(texts(&tree, type_declaration::members(&tree, block)), ["public int Count => items.Length;"]);
}

#[test]
fn namespaces_and_using_directives() {
    let tree = parse(
        "extern alias E;\nglobal using System;\nusing static System.Math;\nusing L = System.Collections.Generic.List<int>;\n\
         using P = (int, int);\nusing unsafe X = int*;\nusing E::A.B;\n\
         namespace Outer.Inner { using Q; class C { } namespace N { } }",
    );
    let usings = compilation_unit::usings(&tree, tree.root()).collect::<Vec<_>>();
    assert_eq!(usings.len(), 6);
    assert_eq!(compilation_unit::externs(&tree, tree.root()).count(), 1);
    assert!(using_directive::global_keyword(&tree, usings[0]).is_some());
    assert_eq!(spanned(&tree, using_directive::name(&tree, usings[0]).unwrap()), "System");
    assert!(using_directive::static_keyword(&tree, usings[1]).is_some());
    let alias = using_directive::alias(&tree, usings[2]).unwrap();
    assert_eq!(text(&tree, identifier_name::identifier(&tree, name_equals::name(&tree, alias).unwrap()).unwrap()), "L");
    let list = using_directive::name(&tree, usings[2]).unwrap();
    assert_eq!(read::dotted_name(&tree, list).as_deref(), Some("System.Collections.Generic.List"));
    assert_eq!(name::error_display_name(&tree, list).as_deref(), Some("System.Collections.Generic.List<>"));
    // an alias of a tuple names no name: `Name` is `NamespaceOrType as NameSyntax`
    assert_eq!(using_directive::name(&tree, usings[3]), None);
    assert_eq!(
        tree.node_kind(using_directive::namespace_or_type(&tree, usings[3]).unwrap()),
        Some(NodeKind::TupleType)
    );
    assert!(using_directive::unsafe_keyword(&tree, usings[4]).is_some());
    let qualified = using_directive::name(&tree, usings[5]).unwrap();
    assert_eq!(read::dotted_name(&tree, qualified).as_deref(), Some("A.B"));
    assert_eq!(name::get_alias_qualifier_opt(&tree, qualified).map(|t| text(&tree, t)), Some("E"));
    assert_eq!(name::error_display_name(&tree, qualified).as_deref(), Some("E::A.B"));
    let segments: Vec<&str> = read::name_segments(&tree, qualified).into_iter().map(|t| text(&tree, t)).collect();
    assert_eq!(segments, ["A", "B"]);
    let ns = all(&tree, NodeKind::NamespaceDeclaration)[0];
    assert_eq!(
        read::dotted_name(&tree, base_namespace_declaration::name(&tree, ns).unwrap()).as_deref(),
        Some("Outer.Inner")
    );
    assert_eq!(base_namespace_declaration::usings(&tree, ns).count(), 1);
    assert_eq!(base_namespace_declaration::members(&tree, ns).count(), 2);
    let tree = parse("namespace A.B;\nusing C;\nclass D { }");
    let ns = all(&tree, NodeKind::FileScopedNamespaceDeclaration)[0];
    assert_eq!(spanned(&tree, base_namespace_declaration::name(&tree, ns).unwrap()), "A.B");
    assert_eq!(base_namespace_declaration::usings(&tree, ns).count(), 1);
    assert_eq!(texts(&tree, base_namespace_declaration::members(&tree, ns)), ["class D { }"]);
}

#[test]
fn names_and_expressions() {
    let tree = parse(
        "class C { void M() { var x = global::System.Math.Abs(1); a?.b.c(); new List<int, string>(1) { }; \
         Func<int, int> l = async x => x; F(ref y, out var z, name: 3); _ = typeof(Dictionary<,>); } }",
    );
    let call = all(&tree, NodeKind::InvocationExpression)[0];
    let callee = invocation_expression::expression(&tree, call).unwrap();
    assert_eq!(spanned(&tree, callee), "global::System.Math.Abs");
    let access = callee;
    assert_eq!(spanned(&tree, member_access_expression::expression(&tree, access).unwrap()), "global::System.Math");
    assert_eq!(spanned(&tree, member_access_expression::name(&tree, access).unwrap()), "Abs");
    assert_eq!(spanned(&tree, invocation_expression::argument_list(&tree, call).unwrap()), "(1)");
    let alias = all(&tree, NodeKind::AliasQualifiedName)[0];
    assert_eq!(
        text(&tree, identifier_name::identifier(&tree, alias_qualified_name::alias(&tree, alias).unwrap()).unwrap()),
        "global"
    );
    assert_eq!(spanned(&tree, alias_qualified_name::name(&tree, alias).unwrap()), "System");
    let conditional = all(&tree, NodeKind::ConditionalAccessExpression)[0];
    assert_eq!(spanned(&tree, conditional_access_expression::expression(&tree, conditional).unwrap()), "a");
    assert_eq!(spanned(&tree, conditional_access_expression::when_not_null(&tree, conditional).unwrap()), ".b.c()");
    let creation = all(&tree, NodeKind::ObjectCreationExpression)[0];
    let ty = object_creation_expression::type_(&tree, creation).unwrap();
    assert_eq!(spanned(&tree, ty), "List<int, string>");
    assert_eq!(name::arity(&tree, ty), 2);
    assert_eq!(spanned(&tree, generic_name::type_argument_list(&tree, ty).unwrap()), "<int, string>");
    assert_eq!(read::type_head(&tree, ty).as_deref(), Some("List"));
    assert!(base_object_creation_expression::initializer(&tree, creation).is_some());
    let var = variable_declaration::type_(&tree, all(&tree, NodeKind::VariableDeclaration)[0]).unwrap();
    assert!(type_::is_var(&tree, var));
    let lambda = all(&tree, NodeKind::SimpleLambdaExpression)[0];
    assert_eq!(text(&tree, anonymous_function_expression::async_keyword(&tree, lambda).unwrap()), "async");
    assert_eq!(spanned(&tree, anonymous_function_expression::body(&tree, lambda).unwrap()), "x");
    let args: Vec<NodeId> = argument_list::arguments(&tree, all(&tree, NodeKind::ArgumentList)[3]).collect();
    assert_eq!(text(&tree, argument::ref_kind_keyword(&tree, args[0]).unwrap()), "ref");
    assert_eq!(spanned(&tree, argument::name_colon(&tree, args[2]).unwrap()), "name:");
    assert_eq!(
        spanned(
            &tree,
            base_expression_colon::expression(&tree, argument::name_colon(&tree, args[2]).unwrap()).unwrap()
        ),
        "name"
    );
    let unbound = all(&tree, NodeKind::GenericName)
        .into_iter()
        .find(|&g| text(&tree, generic_name::identifier(&tree, g).unwrap()) == "Dictionary")
        .unwrap();
    assert!(generic_name::is_unbound_generic_name(&tree, unbound));
    assert_eq!(name::error_display_name(&tree, unbound).as_deref(), Some("Dictionary<,>"));
    let tree = parse("[System.Obsolete(\"x\")] class C { }");
    let attribute = all(&tree, NodeKind::Attribute)[0];
    assert_eq!(
        read::dotted_name(&tree, attribute::name(&tree, attribute).unwrap()).as_deref(),
        Some("System.Obsolete")
    );
}

#[test]
fn the_names_a_type_spells() {
    let tree = parse(
        "class C { (Foo a, Ns.Bar b)? t; Outer<int>.Inner[] u; delegate* unmanaged[Cdecl]<IntPtr, uint> f; \
         global::A.List<Dictionary<K, V>> g; int h; [Attr] ref Baz M() => default; }",
    );
    let names = |source: &str| {
        let ty = tree
            .descendants(tree.root())
            .find(|&n| tree.node_kind(n).is_some_and(is_type_syntax) && spanned(&tree, n) == source)
            .unwrap_or_else(|| panic!("no type `{source}`"));
        let mut out = Vec::new();
        read::type_names(&tree, ty, &mut out);
        out
    };
    assert_eq!(names("(Foo a, Ns.Bar b)?"), ["Foo", "Ns.Bar"]);
    assert_eq!(names("Outer<int>.Inner[]"), ["Outer.Inner"]);
    assert!(names("delegate* unmanaged[Cdecl]<IntPtr, uint>").is_empty());
    assert_eq!(names("global::A.List<Dictionary<K, V>>"), ["A.List"]);
    assert!(names("int").is_empty());
    assert_eq!(names("ref Baz"), ["Baz"]);
    let g = all(&tree, NodeKind::QualifiedName)
        .into_iter()
        .find(|&n| spanned(&tree, n) == "global::A.List<Dictionary<K, V>>")
        .unwrap();
    assert_eq!(read::type_head(&tree, g).as_deref(), Some("A.List"));
    let leaf = read::name_leaf(&tree, g).unwrap();
    assert_eq!(read::word(&tree, leaf), Some("List"));
    assert_eq!(name::get_unqualified_name(&tree, g).map(|n| tree.node_kind(n)), Some(Some(NodeKind::GenericName)));
}

#[test]
fn missing_tokens_are_some_and_null_slots_none() {
    // the parser inserts the missing `;` and `)`: a getter answers them, empty leaves
    let tree = parse("class C { int f void M( { } }");
    let field = all(&tree, NodeKind::FieldDeclaration)[0];
    let semicolon = field_declaration::semicolon_token(&tree, field).unwrap();
    assert!(tree.is_missing(semicolon));
    let m = all(&tree, NodeKind::MethodDeclaration)[0];
    let close =
        parameter_list::close_paren_token(&tree, method_declaration::parameter_list(&tree, m).unwrap()).unwrap();
    assert!(tree.is_missing(close));
    assert!(!read::is_code_token(&tree, close));
}

#[test]
fn directives_and_documentation_comments_are_trivia_with_getters_of_their_own() {
    let tree = parse("#if DEBUG && !X\nclass C { }\n#endif\n/// <param name=\"a\">x</param>\nclass D { }");
    let directive = all(&tree, NodeKind::IfDirectiveTrivia)[0];
    assert!(read::is_structured_trivia(&tree, directive));
    assert_eq!(text(&tree, directive_trivia::directive_name_token(&tree, directive).unwrap()), "if");
    assert_eq!(spanned(&tree, conditional_directive_trivia::condition(&tree, directive).unwrap()), "DEBUG && !X");
    // no symbol defined: the `#if` is read, its branch not taken, its condition false; the `#endif` read too
    assert!(directive_trivia::is_active(&tree, directive));
    assert!(!directive_trivia::branch_taken(&tree, directive) && !directive_trivia::condition_value(&tree, directive));
    assert!(directive_trivia::is_active(&tree, all(&tree, NodeKind::EndIfDirectiveTrivia)[0]));
    assert!(!directive_trivia::is_active(&tree, tree.root()));
    // ` `, `<param name="a">x</param>`, the line end: XML text, an element, XML text
    let doc = all(&tree, NodeKind::SingleLineDocumentationCommentTrivia)[0];
    let content: Vec<Option<NodeKind>> =
        documentation_comment_trivia::content(&tree, doc).map(|c| tree.node_kind(c)).collect();
    assert_eq!(content, [Some(NodeKind::XmlText), Some(NodeKind::XmlElement), Some(NodeKind::XmlText)]);
    // a class whose first token carries both keeps them out of its slots and its span
    let d = named(&tree, NodeKind::ClassDeclaration, "D");
    assert_eq!(spanned(&tree, d), "class D { }");
}

#[test]
fn a_token_list_stops_at_the_keyword_after_it() {
    // `record` after modifiers: the record's keyword, not a modifier; `class` after `record`: its class-or-struct keyword
    let tree =
        parse("public sealed record class R(int A); public ref partial struct S { } internal readonly ref struct T;");
    let r = named(&tree, NodeKind::RecordDeclaration, "R");
    assert_eq!(texts(&tree, record_declaration::modifiers(&tree, r)), ["public", "sealed"]);
    assert_eq!(text(&tree, record_declaration::class_or_struct_keyword(&tree, r).unwrap()), "class");
    let s = named(&tree, NodeKind::StructDeclaration, "S");
    assert_eq!(texts(&tree, struct_declaration::modifiers(&tree, s)), ["public", "ref", "partial"]);
    let tree = parse("class C { void M() { await using var x = y; const int z = 1; } }");
    let local = all(&tree, NodeKind::LocalDeclarationStatement)[0];
    assert!(local_declaration_statement::await_keyword(&tree, local).is_some());
    assert!(local_declaration_statement::using_keyword(&tree, local).is_some());
    assert_eq!(local_declaration_statement::modifiers(&tree, local).count(), 0);
    assert!(local_declaration_statement::is_const(&tree, all(&tree, NodeKind::LocalDeclarationStatement)[1]));
}

/// The goldens of `testdata/csharp`, each at its manifest's version, entry and documentation mode.
fn goldens() -> Vec<(String, Tree)> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/csharp");
    let manifest = std::fs::read_to_string(root.join("manifest.tsv")).expect("read manifest.tsv");
    let mut parser = Parser::new();
    let mut goldens = Vec::new();
    for line in manifest.lines().filter(|l| !l.is_empty()) {
        let columns: Vec<&str> = line.split('\t').collect();
        let mut kind = FileKind::default();
        if let Some(v) = columns.get(1).and_then(|v| LanguageVersion::from_name(v)) {
            kind.version = v;
        }
        if let Some(e) = columns.get(2).and_then(|e| Entry::from_name(e)) {
            kind.entry = e;
        }
        if let Some(d) = columns.get(3).and_then(|d| DocumentationMode::from_name(d)) {
            kind.doc_mode = d;
        }
        let symbols =
            csharp_syntax::PreprocessorSymbols::new(columns.get(4).unwrap_or(&"").split(';').filter(|s| !s.is_empty()));
        let text = std::fs::read(root.join(format!("{}.cs", columns[0]))).expect("read golden");
        let tree = if kind.entry == Entry::Tokens {
            parser.lex_with_symbols(&text, kind, &symbols)
        } else {
            parser.parse_with_symbols(&text, kind, &symbols)
        };
        goldens.push((columns[0].to_string(), tree));
    }
    goldens
}

/// Every getter on every node (`getters_to_string`); every child of every node in a slot of its type; each `_of`
/// dispatcher equal to its field; `read::span`, `span_end`, `first_token` and `last_token` consistent.
fn check_every_getter(tree: &Tree, name: &str) {
    let lines = getters_to_string(tree);
    if tree.is_lexed() {
        return;
    }
    // `read::span` and `span_end` of each node alone equal the one-pass spans of the `Span` lines (Roslyn's, by the
    // gate); `span` descends the left edge, a level per node there, so on a left-deep chain of n links each costs n:
    // asked of every node only below 50 000 nodes, `span_end` (the right edge) of every node always
    let every_span = tree.len() < 50_000;
    for line in lines.lines().filter(|l| l.contains(" Span ")) {
        let mut parts = line.split(' ');
        let id = NodeId(parts.next().unwrap().parse().unwrap());
        let expected = parts.nth(1).unwrap();
        assert_eq!(expected.split("..").nth(1), Some(read::span_end(tree, id).to_string().as_str()), "{name}: {id:?}");
        if every_span {
            let span = read::span(tree, id);
            assert_eq!(expected, format!("{}..{}", span.start, span.end), "{name}: {id:?}");
            if let (Some(first), Some(last)) = (read::first_token(tree, id), read::last_token(tree, id)) {
                assert_eq!((tree.range(first).start, tree.range(last).end), (span.start, span.end), "{name}: {id:?}");
            }
        }
    }
    for id in tree.descendants(tree.root()) {
        let Some(kind) = tree.node_kind(id) else { continue };
        let info = syntax::node_type(kind);
        for (slot, child) in slot_children(tree, id) {
            assert!(slot < info.slots.len(), "{name}: child {child:?} of {kind:?} {id:?} in no slot");
        }
        let field = |name: &str| info.slots.iter().position(|s| s.name == name);
        let nodes = |slot: Option<usize>| -> Vec<NodeId> {
            slot.map_or(Vec::new(), |s| slot_list(tree, id, s).filter(|&c| !tree.is_leaf(c)).collect())
        };
        let tokens =
            |slot: Option<usize>| -> Vec<NodeId> { slot.map_or(Vec::new(), |s| slot_list(tree, id, s).collect()) };
        let one = |slot: Option<usize>| slot.and_then(|s| self::slot(tree, id, s));
        assert_eq!(modifiers_of(tree, id).collect::<Vec<_>>(), tokens(field("Modifiers")), "{name}");
        assert_eq!(attribute_lists_of(tree, id).collect::<Vec<_>>(), nodes(field("AttributeLists")), "{name}");
        assert_eq!(members_of(tree, id).collect::<Vec<_>>(), nodes(field("Members")), "{name}");
        assert_eq!(constraint_clauses_of(tree, id).collect::<Vec<_>>(), nodes(field("ConstraintClauses")), "{name}");
        assert_eq!(identifier_of(tree, id), one(field("Identifier")), "{name}");
        assert_eq!(type_parameter_list_of(tree, id), one(field("TypeParameterList")), "{name}");
        assert_eq!(parameter_list_of(tree, id), one(field("ParameterList")), "{name}");
        assert_eq!(base_list_of(tree, id), one(field("BaseList")), "{name}");
        assert_eq!(body_of(tree, id), one(field("Body")), "{name}");
        assert_eq!(expression_body_of(tree, id), one(field("ExpressionBody")), "{name}");
        assert_eq!(type_of(tree, id), one(field("Type")), "{name}");
        assert_eq!(return_type_of(tree, id), one(field("ReturnType")), "{name}");
        assert_eq!(explicit_interface_specifier_of(tree, id), one(field("ExplicitInterfaceSpecifier")), "{name}");
        assert_eq!(accessor_list_of(tree, id), one(field("AccessorList")), "{name}");
        assert_eq!(semicolon_token_of(tree, id), one(field("SemicolonToken")), "{name}");
        assert_eq!(argument_list_of(tree, id), one(field("ArgumentList")), "{name}");
    }
}

#[test]
fn every_getter_answers_on_every_node_of_the_goldens() {
    let goldens = goldens();
    assert_eq!(goldens.len(), 7064);
    for (name, tree) in &goldens {
        check_every_getter(tree, name);
    }
}

const PIECES: &[&str] = &[
    "class ",
    "struct ",
    "record ",
    "interface ",
    "enum ",
    "delegate ",
    "extension",
    "namespace ",
    "using ",
    "global ",
    "static ",
    "public ",
    "partial ",
    "ref ",
    "readonly ",
    "this ",
    "params ",
    "async ",
    "await ",
    "new ",
    "operator ",
    "implicit ",
    "event ",
    "get",
    "set",
    "init",
    "where ",
    "unsafe ",
    "extern ",
    "alias ",
    "A",
    "b",
    "@c",
    "_",
    "::",
    ".",
    "?.",
    ",",
    ";",
    ":",
    "=",
    "=>",
    "(",
    ")",
    "[",
    "]",
    "{",
    "}",
    "<",
    ">",
    "?",
    "*",
    "1",
    "\"s\"",
    "$\"{x}\"",
    "__arglist",
    "var ",
    "int ",
    "#if X\n",
    "#endif\n",
    "#region\n",
    "/// <summary>a</summary>\n",
    "// c\n",
    "\n",
    " ",
];

#[test]
fn every_getter_answers_on_random_bytes_and_fragments() {
    let mut rng = Rng(0x9E3779B97F4A7C15);
    let mut parser = Parser::new();
    let entries = &common::ENTRIES[..5];
    for round in 0..3000 {
        let mut text = Vec::new();
        let len = rng.below(160);
        if round % 4 == 0 {
            text.extend((0..len).map(|_| rng.next() as u8));
        } else {
            for _ in 0..len {
                text.extend_from_slice(PIECES[rng.below(PIECES.len())].as_bytes());
            }
        }
        let kind = FileKind { entry: entries[round % entries.len()], ..FileKind::default() };
        let tree = parser.parse(&text, kind);
        check_every_getter(&tree, &format!("round {round} {:?}", String::from_utf8_lossy(&text)));
    }
}

/// Runs `f` on a thread of `stack` bytes: a getter that recursed per level would overflow it.
fn on_stack<T: Send + 'static>(stack: usize, f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new().stack_size(stack).spawn(f).unwrap().join().unwrap()
}

#[test]
fn deep_and_long_inputs_take_linear_time_and_no_stack() {
    let n = 30_000;
    let names: Vec<String> = (0..n).map(|i| format!("a{i}")).collect();
    let cases = [
        // a type of n members, a member of n declarators, a chain of n calls and accesses, a sum of n terms; a name of
        // 2000 parts (each of its qualified names prints its `ErrorDisplayName`, the parts to its left: quadratic
        // output, as Roslyn's)
        format!("class C {{ {} }}", names.iter().map(|m| format!("int {m};")).collect::<String>()),
        format!("class C {{ int {}; }}", names.join(", ")),
        format!("using {};", names[..2000].join(".")),
        format!("class C {{ void M() {{ x{}; }} }}", ".f()".repeat(n)),
        format!("class C {{ void M() {{ var y = {}; }} }}", names.join(" + ")),
        // 1000 levels: nested types, namespaces, blocks, parentheses, generic arguments, lambdas
        format!("{}{}", "class C { ".repeat(1000), "}".repeat(1000)),
        format!("{}{}", "namespace N { ".repeat(1000), "}".repeat(1000)),
        format!("class C {{ void M() {{ {}{} }} }}", "{ ".repeat(1000), "}".repeat(1000)),
        format!("class C {{ int x = {}1{}; }}", "(".repeat(1000), ")".repeat(1000)),
        format!("class C {{ {}int{} x; }}", "List<".repeat(30), ">".repeat(30)),
        format!("class C {{ object f = {}0; }}", "x => ".repeat(1000)),
    ];
    for text in cases {
        let head: String = text.chars().take(40).collect();
        let start = std::time::Instant::now();
        let tree = on_stack(256 << 20, move || parse(&text));
        let tree = std::sync::Arc::new(tree);
        let shared = tree.clone();
        // every getter of every node, on a stack of 256 KiB
        let lines = on_stack(256 << 10, move || {
            check_every_getter(&shared, "deep");
            let mut names = Vec::new();
            for id in shared.descendants(shared.root()) {
                if shared.node_kind(id).is_some_and(is_name_syntax) {
                    names.extend(read::dotted_name(&shared, id).map(|n| n.len()));
                    let _ = name::error_display_name(&shared, id);
                    let _ = name::get_alias_qualifier_opt(&shared, id);
                }
            }
            getters_to_string(&shared).len() + names.len()
        });
        assert!(
            start.elapsed() < limit(std::time::Duration::from_secs(10)),
            "{head}: {lines} bytes, {:?}",
            start.elapsed()
        );
    }
}

#[test]
fn a_getter_reads_only_up_to_its_field() {
    // the identifier of a type of 30 000 members, asked 100 000 times: a scan of the members each time would be 3e9
    // steps; the walk stops at the identifier
    let members: String = (0..30_000).map(|i| format!("int a{i};")).collect();
    let tree = parse(&format!("public class C<T> : B {{ {members} }}"));
    let class = all(&tree, NodeKind::ClassDeclaration)[0];
    let start = std::time::Instant::now();
    let mut found = 0;
    for _ in 0..100_000 {
        found += class_declaration::identifier(&tree, class).is_some() as usize;
        found += type_declaration::base_list(&tree, class).is_some() as usize;
        found += type_parameter_list_of(&tree, class).is_some() as usize;
        found += type_declaration::members(&tree, class).next().is_some() as usize;
    }
    assert_eq!(found, 400_000);
    assert!(start.elapsed() < limit(std::time::Duration::from_secs(2)), "{:?}", start.elapsed());
}

#[test]
fn type_slots_are_where_csharp_reads_a_type() {
    let tree = parse(
        "using A.B; using static System.Math; using L = System.Collections.Generic.List<int>;\n\
         namespace N.M { [Attr] class C : Base<Arg>, I.J where T : IFoo {\n\
           Color Color; int I.J.P => 0; void F(Param p) { var v = (Cast)x; _ = x is Is; _ = x as As; \n\
           foreach (Item i in items) { } _ = typeof(Of); _ = x.Member; _ = new New(); _ = x is Pat q; \n\
           System.Int32 n = 0; g(a: 1); } } }",
    );
    let name = |text: &str| {
        tree.descendants(tree.root())
            .rfind(|&n| tree.node_kind(n).is_some_and(is_name_syntax) && spanned(&tree, n) == text)
            .unwrap_or_else(|| panic!("no name `{text}`"))
    };
    for t in [
        "Attr",
        "Base<Arg>",
        "Arg",
        "I.J",
        "IFoo",
        "Color",
        "Param",
        "Cast",
        "Is",
        "As",
        "Item",
        "Of",
        "New",
        "Pat",
        "Int32",
        "System.Int32",
        "System.Math",
    ] {
        assert!(is_in_type_slot(&tree, name(t)), "{t} is a type");
    }
    for t in ["A.B", "System.Collections.Generic.List<int>", "N.M", "x", "Member", "a", "System", "items"] {
        assert!(!is_in_type_slot(&tree, name(t)), "{t} is no type");
    }
    let member = name("Member");
    let info = slot_info(&tree, member).unwrap();
    assert_eq!((info.name, info.type_name), ("Name", "SimpleNameSyntax"));
    assert_eq!(slot_of(&tree, member), Some(syntax::types::MemberAccessExpressionSyntax::NAME));
    assert_eq!(slot_of(&tree, tree.root()), None);
    // a type argument list's `<`: a token, never a type
    let less = tree.descendants(tree.root()).find(|&n| tree.token_kind(n) == Some(TokenKind::LessThanToken)).unwrap();
    assert!(!is_in_type_slot(&tree, less));
}

#[test]
fn name_texts_and_documentation_comments() {
    let tree = parse(
        "class X { global :: System . /* c */ Collections.Generic.List< int > f; }\n\
         /// <summary>doc</summary>\n// plain\n[A] public class C { }\nclass D { }",
    );
    let field = all(&tree, NodeKind::FieldDeclaration)[0];
    let ty = variable_declaration::type_(&tree, field_declaration::declaration(&tree, field).unwrap()).unwrap();
    assert_eq!(read::name_text(&tree, ty).as_deref(), Some("global::System.Collections.Generic.List<int>"));
    assert_eq!(read::dotted_name(&tree, ty).as_deref(), Some("System.Collections.Generic.List"));
    let c = named(&tree, NodeKind::ClassDeclaration, "C");
    let doc = read::doc_comment(&tree, c).unwrap();
    assert_eq!(tree.node_kind(doc), Some(NodeKind::SingleLineDocumentationCommentTrivia));
    assert_eq!(read::declaration_start(&tree, c), tree.range(doc).start);
    assert!(read::span(&tree, c).start > tree.range(doc).end);
    let d = named(&tree, NodeKind::ClassDeclaration, "D");
    assert_eq!(read::doc_comment(&tree, d), None);
    assert_eq!(read::declaration_start(&tree, d), read::span(&tree, d).start);
    // without parsed doc comments, the `///` comment leaf
    let plain = Tree::parse(
        "/// <summary>doc</summary>\n//// no\nclass C { }",
        FileKind { doc_mode: DocumentationMode::None, ..FileKind::default() },
    );
    let c = all(&plain, NodeKind::ClassDeclaration)[0];
    assert_eq!(plain.utf8_text(read::doc_comment(&plain, c).unwrap()), Some("/// <summary>doc</summary>"));
    // `/**` opens one, `/***` and `/**/` do not, as the lexer reads them at `DocumentationMode::Parse`
    for (source, doc) in [("/** d */\nclass C { }", Some("/** d */")), ("/*** d */\nclass C { }", None), ("/**/\nclass C { }", None)] {
        let plain = Tree::parse(source, FileKind { doc_mode: DocumentationMode::None, ..FileKind::default() });
        let c = all(&plain, NodeKind::ClassDeclaration)[0];
        assert_eq!(read::doc_comment(&plain, c).and_then(|d| plain.utf8_text(d)), doc, "{source}");
        let parsed = parse(source);
        let c = all(&parsed, NodeKind::ClassDeclaration)[0];
        assert_eq!(read::doc_comment(&parsed, c).is_some(), doc.is_some(), "{source} parsed");
    }
}
