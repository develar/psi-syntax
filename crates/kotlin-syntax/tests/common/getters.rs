//! The lines `tools/kotlin-psi-getters/GetterOracle.java` prints, made from `kotlin_syntax::psi`: one per composite
//! node and getter, in preorder, the getters of a node in the oracle's order. The oracle picks them by the PSI
//! classes an element is an instance of; the tables below are those classes by node kind (`GetterOracle.java
//! --classes`), so a getter that answers on the wrong kinds shows as a missing or an extra line.

use std::fmt::Write;

use kotlin_syntax::NodeKind::{self, *};
use kotlin_syntax::psi::*;
use kotlin_syntax::tokens::MODIFIER_KEYWORDS_ARRAY;
use kotlin_syntax::{ElementType, NodeId, Tree};

/// The text both sides parse: kotlinc's line separator conversion, as `GetterOracle.java` and `PsiOracle.java` do.
pub fn oracle_text(bytes: &[u8]) -> Vec<u8> {
    let mut text = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' if bytes.get(i + 1) == Some(&b'\n') => {}
            b'\r' => text.push(b'\n'),
            b => text.push(b),
        }
        i += 1;
    }
    text
}

fn kinds(tree: &Tree, id: NodeId, set: &[NodeKind]) -> bool {
    tree.node_kind(id).is_some_and(|k| set.contains(&k))
}

const DECLARATION: &[NodeKind] = &[
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
];
const MODIFIER_LIST_OWNER_EXTRA: &[NodeKind] = &[PACKAGE_DIRECTIVE, TYPE_PROJECTION, TYPE_REFERENCE];
const ANNOTATED_EXTRA: &[NodeKind] = &[ANNOTATED_EXPRESSION, TYPE_CONSTRAINT, KT_FILE];
const NAMED_DECLARATION: &[NodeKind] = &[
    CLASS,
    DESTRUCTURING_DECLARATION_ENTRY,
    ENUM_ENTRY,
    FUN,
    FUNCTION_LITERAL,
    OBJECT_DECLARATION,
    PRIMARY_CONSTRUCTOR,
    PROPERTY,
    SECONDARY_CONSTRUCTOR,
    TYPEALIAS,
    TYPE_PARAMETER,
    VALUE_PARAMETER,
];
const CLASS_OR_OBJECT: &[NodeKind] = &[CLASS, ENUM_ENTRY, OBJECT_DECLARATION];
const KT_CLASS: &[NodeKind] = &[CLASS, ENUM_ENTRY];
const SUPER_TYPE_LIST_ENTRY: &[NodeKind] = &[DELEGATED_SUPER_TYPE_ENTRY, SUPER_TYPE_CALL_ENTRY, SUPER_TYPE_ENTRY];
const CALLABLE: &[NodeKind] = &[
    DESTRUCTURING_DECLARATION_ENTRY,
    FUN,
    FUNCTION_LITERAL,
    PRIMARY_CONSTRUCTOR,
    PROPERTY,
    SECONDARY_CONSTRUCTOR,
    VALUE_PARAMETER,
];
const TYPE_PARAMETER_LIST_OWNER: &[NodeKind] = &[
    CLASS,
    DESTRUCTURING_DECLARATION_ENTRY,
    ENUM_ENTRY,
    FUN,
    FUNCTION_LITERAL,
    OBJECT_DECLARATION,
    PRIMARY_CONSTRUCTOR,
    PROPERTY,
    SECONDARY_CONSTRUCTOR,
    TYPEALIAS,
    VALUE_PARAMETER,
];
const TYPE_ELEMENT: &[NodeKind] = &[DYNAMIC_TYPE, FUNCTION_TYPE, INTERSECTION_TYPE, NULLABLE_TYPE, USER_TYPE];
const CALL_ELEMENT: &[NodeKind] =
    &[ANNOTATION_ENTRY, CALL_EXPRESSION, CONSTRUCTOR_DELEGATION_CALL, SUPER_TYPE_CALL_ENTRY];
const QUALIFIED: &[NodeKind] = &[DOT_QUALIFIED_EXPRESSION, SAFE_ACCESS_EXPRESSION];
const VALUE_ARGUMENT_KINDS: &[NodeKind] = &[VALUE_ARGUMENT, LAMBDA_ARGUMENT];

struct Printer<'t> {
    tree: &'t Tree,
    out: String,
    prefix: String,
}

impl Printer<'_> {
    fn name(&self, id: NodeId) -> &'static str {
        match self.tree.kind(id) {
            ElementType::Node(KT_FILE) => "kotlin.FILE",
            ElementType::Node(n) => n.debug_name(),
            ElementType::Token(t) => t.debug_name(),
        }
    }

    fn el(&self, id: Option<NodeId>) -> String {
        match id {
            None => "null".to_owned(),
            Some(id) => {
                let r = self.tree.range(id);
                format!("{}@{}..{}", self.name(id), r.start, r.end)
            }
        }
    }

    fn els(&self, ids: impl Iterator<Item = NodeId>) -> String {
        let items: Vec<String> = ids.map(|i| self.el(Some(i))).collect();
        format!("[{}]", items.join(","))
    }

    fn str(value: Option<&str>) -> String {
        let Some(v) = value else { return "null".to_owned() };
        let mut b = String::from("\"");
        for c in v.chars() {
            match c {
                '\\' => b.push_str("\\\\"),
                '"' => b.push_str("\\\""),
                '\n' => b.push_str("\\n"),
                '\r' => b.push_str("\\r"),
                '\t' => b.push_str("\\t"),
                c => b.push(c),
            }
        }
        b.push('"');
        b
    }

    fn line(&mut self, getter: &str, answer: impl std::fmt::Display) {
        let _ = writeln!(self.out, "{}{getter}={answer}", self.prefix);
    }

    fn element(&mut self, e: NodeId) {
        let t = self.tree;
        let is = |set: &[NodeKind]| kinds(t, e, set);
        let kind = t.node_kind(e);
        if is_kt_expression(t, e) {
            self.line("expression", true);
        }
        if kind == Some(KT_FILE) {
            self.line("fileAnnotationList", self.el(kt_file_get_file_annotation_list(t, e)));
        }
        let owner = is(DECLARATION) || is(MODIFIER_LIST_OWNER_EXTRA);
        if owner {
            self.line("modifierList", self.el(get_modifier_list(t, e)));
            let modifiers: Vec<&str> =
                MODIFIER_KEYWORDS_ARRAY.iter().filter(|&&m| has_modifier(t, e, m)).filter_map(|m| m.value()).collect();
            self.line("modifiers", format!("[{}]", modifiers.join(",")));
            self.line("visibilityModifier", self.el(visibility_modifier(t, e)));
            self.line("visibilityModifierType", Self::str(visibility_modifier_type(t, e).map(|k| k.debug_name())));
        }
        if is(DECLARATION) {
            self.line("modalityModifierType", Self::str(modality_modifier_type(t, e).map(|k| k.debug_name())));
        }
        if owner || is(ANNOTATED_EXTRA) {
            self.line("annotationEntries", self.els(get_annotation_entries(t, e)));
        }
        if kind == Some(MODIFIER_LIST) {
            self.line("annotationEntries", self.els(kt_modifier_list_get_annotation_entries(t, e)));
        }
        if kind == Some(ANNOTATION) {
            self.line("entries", self.els(kt_annotation_get_entries(t, e)));
            self.line("useSiteTarget", self.el(kt_annotation_get_use_site_target(t, e)));
        }
        if kind == Some(ANNOTATION_ENTRY) {
            self.line("shortName", Self::str(kt_annotation_entry_get_short_name(t, e).as_deref()));
            self.line("useSiteTarget", self.el(kt_annotation_entry_get_use_site_target(t, e)));
            self.line("typeReference", self.el(kt_annotation_entry_get_type_reference(t, e)));
            self.line("atSymbol", self.el(kt_annotation_entry_get_at_symbol(t, e)));
        }
        if kind == Some(ANNOTATION_TARGET) {
            let target = kt_annotation_use_site_target_get_annotation_use_site_target(t, e);
            self.line("annotationUseSiteTarget", Self::str(target.map(|t| t.name())));
        }
        if is(NAMED_DECLARATION) {
            self.line("name", Self::str(get_name(t, e)));
            self.line("nameIdentifier", self.el(get_name_identifier(t, e)));
        }
        if kind == Some(IMPORT_ALIAS) {
            self.line("name", Self::str(get_name(t, e)));
            self.line("nameIdentifier", self.el(get_name_identifier(t, e)));
        }
        if is(CLASS_OR_OBJECT) {
            self.line("isTopLevel", kt_class_or_object_is_top_level(t, e));
            self.line("isData", kt_class_or_object_is_data(t, e));
            self.line("isAnnotation", kt_class_or_object_is_annotation(t, e));
            self.line("declarationKeyword", self.el(kt_class_or_object_get_declaration_keyword(t, e)));
            self.line("superTypeList", self.el(kt_class_or_object_get_super_type_list(t, e)));
            self.line("superTypeListEntries", self.els(kt_class_or_object_get_super_type_list_entries(t, e)));
            self.line("body", self.el(kt_class_or_object_get_body(t, e)));
            self.line("primaryConstructor", self.el(kt_class_or_object_get_primary_constructor(t, e)));
            self.line(
                "primaryConstructorParameters",
                self.els(kt_class_or_object_get_primary_constructor_parameters(t, e)),
            );
            self.line("secondaryConstructors", self.els(kt_class_or_object_get_secondary_constructors(t, e)));
        }
        if is(KT_CLASS) {
            self.line("isInterface", kt_class_is_interface(t, e));
            self.line("isEnum", kt_class_is_enum(t, e));
            self.line("isSealed", kt_class_is_sealed(t, e));
            self.line("isInner", kt_class_is_inner(t, e));
            self.line("isInline", kt_class_is_inline(t, e));
            self.line("isValue", kt_class_is_value(t, e));
            self.line("funKeyword", self.el(kt_class_get_fun_keyword(t, e)));
        }
        if kind == Some(OBJECT_DECLARATION) {
            self.line("isCompanion", kt_object_declaration_is_companion(t, e));
            self.line("isObjectLiteral", kt_object_declaration_is_object_literal(t, e));
        }
        if is(SUPER_TYPE_LIST_ENTRY) {
            self.line("typeReference", self.el(get_type_reference(t, e)));
            self.line("typeAsUserType", self.el(kt_super_type_list_entry_get_type_as_user_type(t, e)));
        }
        if is(CALLABLE) {
            self.line("receiverTypeReference", self.el(get_receiver_type_reference(t, e)));
            self.line("typeReference", self.el(get_type_reference(t, e)));
            self.line("valueParameterList", self.el(get_value_parameter_list(t, e)));
            self.line("valueParameters", self.els(get_value_parameters(t, e)));
        }
        if is(TYPE_PARAMETER_LIST_OWNER) {
            self.line("typeParameterList", self.el(get_type_parameter_list(t, e)));
            self.line("typeParameters", self.els(get_type_parameters(t, e)));
            self.line("typeConstraintList", self.el(get_type_constraint_list(t, e)));
            self.line("typeConstraints", self.els(get_type_constraints(t, e)));
        }
        if matches!(kind, Some(FUN | PROPERTY | PROPERTY_ACCESSOR)) {
            self.line("isExtensionDeclaration", is_extension_declaration(t, e));
        }
        if kind == Some(FUN) {
            self.line("isLocal", kt_named_function_is_local(t, e));
            self.line("isAnonymous", kt_named_function_is_anonymous(t, e));
            self.line("isTopLevel", kt_named_function_is_top_level(t, e));
            self.line("initializer", self.el(get_initializer(t, e)));
        }
        if kind == Some(PROPERTY) {
            self.line("isLocal", kt_property_is_local(t, e));
            self.line("isMember", kt_property_is_member(t, e));
            self.line("isTopLevel", kt_property_is_top_level(t, e));
            self.line("isVar", is_var(t, e));
            self.line("initializer", self.el(get_initializer(t, e)));
            self.line("delegate", self.el(kt_property_get_delegate(t, e)));
            self.line("delegateExpression", self.el(kt_property_get_delegate_expression(t, e)));
            self.line("accessors", self.els(kt_property_get_accessors(t, e)));
            self.line("getter", self.el(kt_property_get_getter(t, e)));
            self.line("setter", self.el(kt_property_get_setter(t, e)));
        }
        if kind == Some(PROPERTY_ACCESSOR) {
            self.line("isGetter", kt_property_accessor_is_getter(t, e));
            self.line("isSetter", kt_property_accessor_is_setter(t, e));
            self.line("typeReference", self.el(get_type_reference(t, e)));
            self.line("valueParameterList", self.el(get_value_parameter_list(t, e)));
            self.line("valueParameters", self.els(get_value_parameters(t, e)));
            self.line("initializer", self.el(get_initializer(t, e)));
        }
        if matches!(kind, Some(TYPE_PROJECTION | CONSTRUCTOR_CALLEE | FUNCTION_TYPE_RECEIVER)) {
            self.line("typeReference", self.el(get_type_reference(t, e)));
        }
        if kind == Some(VALUE_PARAMETER) {
            self.line("hasDefaultValue", kt_parameter_has_default_value(t, e));
            self.line("defaultValue", self.el(kt_parameter_get_default_value(t, e)));
            self.line("isVarArg", kt_parameter_is_var_arg(t, e));
            self.line("isMutable", kt_parameter_is_mutable(t, e));
            self.line("hasValOrVar", kt_parameter_has_val_or_var(t, e));
            self.line("valOrVarKeyword", self.el(get_val_or_var_keyword(t, e)));
            self.line("destructuringDeclaration", self.el(kt_parameter_get_destructuring_declaration(t, e)));
            self.line("isLoopParameter", kt_parameter_is_loop_parameter(t, e));
            self.line("isCatchParameter", kt_parameter_is_catch_parameter(t, e));
        }
        if kind == Some(TYPEALIAS) {
            self.line("typeReference", self.el(get_type_reference(t, e)));
            self.line("isTopLevel", kt_type_alias_is_top_level(t, e));
        }
        if kind == Some(TYPE_PARAMETER) {
            self.line("extendsBound", self.el(kt_type_parameter_get_extends_bound(t, e)));
        }
        if kind == Some(TYPE_CONSTRAINT) {
            self.line("subjectTypeParameterName", self.el(kt_type_constraint_get_subject_type_parameter_name(t, e)));
            self.line("boundTypeReference", self.el(kt_type_constraint_get_bound_type_reference(t, e)));
        }
        if kind == Some(DESTRUCTURING_DECLARATION) {
            self.line("entries", self.els(kt_destructuring_declaration_get_entries(t, e)));
            self.line("initializer", self.el(get_initializer(t, e)));
            self.line("isVar", is_var(t, e));
            self.line("valOrVarKeyword", self.el(get_val_or_var_keyword(t, e)));
        }
        if kind == Some(DESTRUCTURING_DECLARATION_ENTRY) {
            self.line("isVar", is_var(t, e));
            self.line("valOrVarKeyword", self.el(get_val_or_var_keyword(t, e)));
            self.line("initializer", self.el(get_initializer(t, e)));
        }
        if kind == Some(TYPE_REFERENCE) {
            self.line("typeElement", self.el(kt_type_reference_get_type_element(t, e)));
            self.line("hasParentheses", kt_type_reference_has_parentheses(t, e));
        }
        if is(TYPE_ELEMENT) {
            self.line("unwrapNullability", self.el(unwrap_nullability(t, e)));
        }
        if kind == Some(NULLABLE_TYPE) {
            self.line("innerType", self.el(kt_nullable_type_get_inner_type(t, e)));
            self.line("modifierList", self.el(get_modifier_list(t, e)));
            self.line("annotationEntries", self.els(get_annotation_entries(t, e)));
        }
        if kind == Some(USER_TYPE) {
            self.line("referenceExpression", self.el(kt_user_type_get_reference_expression(t, e)));
            self.line("qualifier", self.el(kt_user_type_get_qualifier(t, e)));
            self.line("referencedName", Self::str(get_referenced_name(t, e).as_deref()));
            self.line("typeArgumentList", self.el(get_type_argument_list(t, e)));
        }
        if kind == Some(FUNCTION_TYPE) {
            self.line("receiver", self.el(kt_function_type_get_receiver(t, e)));
            self.line("receiverTypeReference", self.el(get_receiver_type_reference(t, e)));
            self.line("parameterList", self.el(kt_function_type_get_parameter_list(t, e)));
            self.line("returnTypeReference", self.el(kt_function_type_get_return_type_reference(t, e)));
        }
        if matches!(kind, Some(REFERENCE_EXPRESSION | ENUM_ENTRY_SUPERCLASS_REFERENCE_EXPRESSION)) {
            self.line("referencedName", Self::str(get_referenced_name(t, e).as_deref()));
            self.line("referencedNameElement", self.el(get_referenced_name_element(t, e)));
        }
        if kind == Some(IMPORT_DIRECTIVE) {
            self.line("importedReference", self.el(kt_import_directive_get_imported_reference(t, e)));
            self.line("alias", self.el(kt_import_directive_get_alias(t, e)));
            self.line("aliasName", Self::str(kt_import_directive_get_alias_name(t, e)));
            self.line("isAllUnder", kt_import_directive_is_all_under(t, e));
            self.line("importedFqName", Self::str(kt_import_directive_get_imported_fq_name(t, e).as_deref()));
        }
        if is(QUALIFIED) {
            self.line("receiverExpression", self.el(kt_qualified_expression_get_receiver_expression(t, e)));
            self.line("selectorExpression", self.el(kt_qualified_expression_get_selector_expression(t, e)));
        }
        if is(CALL_ELEMENT) {
            self.line("calleeExpression", self.el(get_callee_expression(t, e)));
            self.line("valueArgumentList", self.el(get_value_argument_list(t, e)));
            self.line("valueArguments", self.els(get_value_arguments(t, e)));
            self.line("lambdaArguments", self.els(get_lambda_arguments(t, e)));
            self.line("typeArgumentList", self.el(get_type_argument_list(t, e)));
        }
        if is(VALUE_ARGUMENT_KINDS) {
            self.line("argumentExpression", self.el(kt_value_argument_get_argument_expression(t, e)));
            self.line("argumentName", self.el(kt_value_argument_get_argument_name(t, e)));
            self.line("isSpread", kt_value_argument_is_spread(t, e));
        }
        if kind == Some(LAMBDA_ARGUMENT) {
            self.line("lambdaExpression", self.el(kt_lambda_argument_get_lambda_expression(t, e)));
        }
        if kind == Some(LAMBDA_EXPRESSION) {
            self.line("functionLiteral", self.el(kt_lambda_expression_get_function_literal(t, e)));
            self.line("valueParameters", self.els(get_value_parameters(t, e)));
        }
    }
}

/// Every composite node's getter lines, in preorder.
pub fn lines(tree: &Tree) -> String {
    let mut p = Printer { tree, out: String::new(), prefix: String::new() };
    for id in tree.descendants(tree.root()) {
        if tree.is_leaf(id) {
            continue;
        }
        let r = tree.range(id);
        p.prefix = format!("{}..{} {} ", r.start, r.end, p.name(id));
        p.element(id);
    }
    p.out
}
