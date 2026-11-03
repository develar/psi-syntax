//! `getters_to_string`: the answer of every getter of this module on every node of a tree, one line each, as
//! `tools/java-oracle/JavaPsiOracle.java --getters` prints the real PSI's (`<path>.getters.txt`), so that `ksdump
//! --getters --check` compares the two (and `tests/psi.rs` runs every getter on every node of its inputs).
//!
//! A line is `<id> <getter> <answer>`, nodes in preorder (the oracle numbers `getNode()` in preorder, which is our id:
//! the gate keeps the node dumps equal), the getters of a node in a fixed order by its element type. An answer is a
//! node id, `-` for none, `true` or `false`, a count, a list of ids joined by `,` (`.` when empty), a set of
//! modifiers by name joined by `,` (`.` when empty), an enum constant's name, or a string with `\n` and `\\` escaped.
//! The oracle prints `!` where IntelliJ throws; `--getters --check` takes any answer of ours there.

use std::fmt::Write;

use super::code_reference::Kind;
use super::modifier::{Modifier, Modifiers};
use super::type_element::TypeKind;
use super::*;
use crate::node_kind::NodeKind;
use crate::tree::{NodeId, Tree};

/// Every line of a tree, `in_package_info` for `code_reference::get_kind_enum` (the file is a `package-info.java`).
///
/// A field's type element and modifier list come from `field::declaration_heads` (a field of a declaration of n
/// names would walk back n fields; `tests/psi.rs` checks the two equal).
pub fn getters_to_string(tree: &Tree, in_package_info: bool) -> String {
    let mut heads = std::collections::HashMap::new();
    for id in tree.descendants(tree.root()) {
        if tree.node_kind(id) == Some(NodeKind::FIELD)
            && !heads.contains_key(&id)
            && let Some(parent) = tree.parent(id)
        {
            heads.extend(field::declaration_heads(tree, parent));
        }
    }
    let mut out = String::new();
    for id in tree.descendants(tree.root()) {
        node_getters(tree, id, in_package_info, heads.get(&id).copied(), &mut out);
    }
    out
}

struct Lines<'o> {
    out: &'o mut String,
    id: NodeId,
}

impl Lines<'_> {
    fn raw(&mut self, getter: &str, answer: std::fmt::Arguments) {
        let _ = writeln!(self.out, "{} {getter} {answer}", self.id.0);
    }

    fn node(&mut self, getter: &str, node: Option<NodeId>) {
        match node {
            Some(n) => self.raw(getter, format_args!("{}", n.0)),
            None => self.raw(getter, format_args!("-")),
        }
    }

    fn bool(&mut self, getter: &str, value: bool) {
        self.raw(getter, format_args!("{value}"));
    }

    fn count(&mut self, getter: &str, value: Option<u32>) {
        match value {
            Some(n) => self.raw(getter, format_args!("{n}")),
            None => self.raw(getter, format_args!("-")),
        }
    }

    fn list(&mut self, getter: &str, nodes: impl Iterator<Item = NodeId>) {
        let _ = write!(self.out, "{} {getter} ", self.id.0);
        let mut empty = true;
        for n in nodes {
            if !empty {
                self.out.push(',');
            }
            let _ = write!(self.out, "{}", n.0);
            empty = false;
        }
        if empty {
            self.out.push('.');
        }
        self.out.push('\n');
    }

    fn modifiers(&mut self, getter: &str, modifiers: Modifiers) {
        let _ = write!(self.out, "{} {getter} ", self.id.0);
        if modifiers.is_empty() {
            self.out.push('.');
        }
        for (i, m) in modifiers.iter().enumerate() {
            if i > 0 {
                self.out.push(',');
            }
            self.out.push_str(m.name());
        }
        self.out.push('\n');
    }

    fn text(&mut self, getter: &str, text: &str) {
        let _ = write!(self.out, "{} {getter} ", self.id.0);
        for c in text.chars() {
            match c {
                '\n' => self.out.push_str("\\n"),
                '\\' => self.out.push_str("\\\\"),
                c => self.out.push(c),
            }
        }
        self.out.push('\n');
    }

    /// `getModifierList()` and `hasModifierProperty` of each modifier, of an owner.
    fn owner(&mut self, tree: &Tree, owner: NodeId) {
        self.node("modifierList", get_modifier_list(tree, owner));
        let modifiers = Modifier::ALL
            .into_iter()
            .filter(|&m| has_modifier_property(tree, owner, m))
            .fold(Modifiers::EMPTY, Modifiers::with);
        self.modifiers("mods", modifiers);
    }
}

fn node_getters(tree: &Tree, id: NodeId, in_package_info: bool, head: Option<NodeId>, out: &mut String) {
    let Some(kind) = tree.node_kind(id) else { return };
    let mut l = Lines { out, id };
    match kind {
        NodeKind::CLASS
        | NodeKind::ANONYMOUS_CLASS
        | NodeKind::ENUM_CONSTANT_INITIALIZER
        | NodeKind::IMPLICIT_CLASS => {
            l.bool("isInterface", class::is_interface(tree, id));
            l.bool("isEnum", class::is_enum(tree, id));
            l.bool("isRecord", class::is_record(tree, id));
            l.bool("isAnnotationType", class::is_annotation_type(tree, id));
            l.node("nameIdentifier", get_name_identifier(tree, id));
            l.node("extendsList", class::get_extends_list(tree, id));
            l.node("implementsList", class::get_implements_list(tree, id));
            l.node("permitsList", class::get_permits_list(tree, id));
            l.node("recordHeader", class::get_record_header(tree, id));
            l.node("typeParameterList", class::get_type_parameter_list(tree, id));
            l.node("containingClass", get_containing_class(tree, id));
            l.owner(tree, id);
            if kind == NodeKind::ANONYMOUS_CLASS {
                l.node("baseClassReference", anonymous_class::get_base_class_reference(tree, id));
                l.node("argumentList", anonymous_class::get_argument_list(tree, id));
            }
        }
        NodeKind::EXTENDS_LIST
        | NodeKind::IMPLEMENTS_LIST
        | NodeKind::PERMITS_LIST
        | NodeKind::THROWS_LIST
        | NodeKind::EXTENDS_BOUND_LIST
        | NodeKind::PROVIDES_WITH_LIST => l.list("refs", reference_list::get_reference_elements(tree, id)),
        NodeKind::METHOD | NodeKind::ANNOTATION_METHOD => {
            l.bool("isConstructor", method::is_constructor(tree, id));
            l.node("returnTypeElement", method::get_return_type_element(tree, id));
            l.node("parameterList", method::get_parameter_list(tree, id));
            l.node("typeParameterList", method::get_type_parameter_list(tree, id));
            l.node("throwsList", method::get_throws_list(tree, id));
            l.node("body", method::get_body(tree, id));
            l.bool("isVarArgs", method::is_var_args(tree, id));
            l.bool("isCompactConstructor", method::is_compact_constructor(tree, id));
            if !method::is_constructor(tree, id) {
                l.count("returnDims", method::get_return_type_array_dimensions(tree, id));
            }
            l.node("containingClass", get_containing_class(tree, id));
            l.node("nameIdentifier", get_name_identifier(tree, id));
            l.owner(tree, id);
        }
        NodeKind::FIELD => {
            let head = head.unwrap_or(id);
            let type_element = field::get_type_element_with_head(tree, head);
            l.node("typeElement", type_element);
            l.node("nameIdentifier", get_name_identifier(tree, id));
            l.node("initializer", get_initializer(tree, id));
            if type_element.is_some() {
                l.count("dims", field::get_type_array_dimensions_with_head(tree, id, head));
            }
            l.node("containingClass", get_containing_class(tree, id));
            let list = field::get_modifier_list_with_head(tree, head);
            l.node("modifierList", list);
            let modifiers = list.map_or(Modifiers::EMPTY, |list| modifier_list::calc_modifiers(tree, list));
            l.modifiers("mods", modifiers);
        }
        NodeKind::ENUM_CONSTANT => {
            l.node("typeElement", get_type_element(tree, id));
            l.node("nameIdentifier", get_name_identifier(tree, id));
            l.node("initializer", get_initializer(tree, id));
            l.node("argumentList", enum_constant::get_argument_list(tree, id));
            l.node("initializingClass", enum_constant::get_initializing_class(tree, id));
            l.node("containingClass", get_containing_class(tree, id));
            l.owner(tree, id);
        }
        NodeKind::LOCAL_VARIABLE | NodeKind::RESOURCE_VARIABLE => {
            l.node("typeElement", get_type_element(tree, id));
            l.node("nameIdentifier", get_name_identifier(tree, id));
            l.node("initializer", get_initializer(tree, id));
            if get_type_element(tree, id).is_some_and(|t| !type_element::is_inferred_type(tree, t)) {
                l.count("dims", local_variable::get_type_array_dimensions(tree, id));
            }
            l.owner(tree, id);
        }
        NodeKind::PARAMETER => {
            l.node("typeElement", get_type_element(tree, id));
            l.node("nameIdentifier", get_name_identifier(tree, id));
            if get_type_element(tree, id).is_some_and(|t| !type_element::is_inferred_type(tree, t)) {
                l.count("dims", parameter::get_type_array_dimensions(tree, id));
            }
            l.bool("isVarArgs", parameter::is_var_args(tree, id));
            l.owner(tree, id);
        }
        NodeKind::RECORD_COMPONENT => {
            l.node("typeElement", get_type_element(tree, id));
            l.node("nameIdentifier", get_name_identifier(tree, id));
            l.count("dims", record_component::get_type_array_dimensions(tree, id));
            l.bool("isVarArgs", record_component::is_var_args(tree, id));
            l.node("containingClass", get_containing_class(tree, id));
            l.owner(tree, id);
        }
        NodeKind::PATTERN_VARIABLE => {
            l.node("typeElement", get_type_element(tree, id));
            l.node("nameIdentifier", get_name_identifier(tree, id));
            l.owner(tree, id);
        }
        NodeKind::RECEIVER_PARAMETER => {
            l.node("typeElement", get_type_element(tree, id));
            l.owner(tree, id);
        }
        NodeKind::DECONSTRUCTION_PATTERN_VARIABLE | NodeKind::TYPE_PARAMETER => {
            l.node("nameIdentifier", get_name_identifier(tree, id));
            l.owner(tree, id);
        }
        NodeKind::CLASS_INITIALIZER => {
            l.node("containingClass", get_containing_class(tree, id));
            l.owner(tree, id);
        }
        NodeKind::MODULE | NodeKind::REQUIRES_STATEMENT => l.owner(tree, id),
        NodeKind::MODIFIER_LIST => {
            let explicit = Modifier::ALL
                .into_iter()
                .filter(|&m| modifier_list::has_explicit_modifier(tree, id, m))
                .fold(Modifiers::EMPTY, Modifiers::with);
            l.modifiers("explicit", explicit);
            l.modifiers("modifiers", modifier_list::calc_modifiers(tree, id));
            l.list("annotations", modifier_list::get_annotations(tree, id));
        }
        NodeKind::TYPE => {
            l.node("innermost", type_element::get_innermost_component_reference_element(tree, id));
            let inferred = type_element::is_inferred_type(tree, id);
            l.bool("inferred", inferred);
            let unnamed_pattern = tree.first_child(id).is_none()
                && tree.parent(id).and_then(|p| tree.node_kind(p)) == Some(NodeKind::UNNAMED_PATTERN);
            if !inferred && !unnamed_pattern {
                let ty = type_element::calculate_type(tree, id);
                let kind = match ty.kind {
                    _ if ty.array_dimensions > 0 => {
                        if ty.ellipsis {
                            "ellipsis"
                        } else {
                            "array"
                        }
                    }
                    TypeKind::Null => "null",
                    TypeKind::Primitive(_) => "primitive",
                    TypeKind::ClassReference(_) => "class",
                    TypeKind::Inferred(_) => "inferred",
                    TypeKind::Wildcard => "wildcard",
                    TypeKind::Intersection => "intersection",
                    TypeKind::Disjunction => "disjunction",
                    TypeKind::Diamond => "diamond",
                };
                l.raw("type", format_args!("{kind} {}", ty.array_dimensions));
            }
        }
        NodeKind::JAVA_CODE_REFERENCE => {
            l.node("name", code_reference::get_reference_name_element(tree, id));
            l.node("qualifier", code_reference::get_qualifier(tree, id));
            l.node("params", code_reference::get_parameter_list(tree, id));
            l.bool("qualified", code_reference::is_qualified(tree, id));
            let kind = code_reference::get_kind_enum(tree, id, in_package_info);
            l.raw("kind", format_args!("{}", kind_name(kind)));
        }
        NodeKind::IMPORT_STATIC_REFERENCE => {
            l.node("name", import_static_reference::get_reference_name_element(tree, id));
            l.node("classReference", import_static_reference::get_class_reference(tree, id));
        }
        NodeKind::REFERENCE_EXPRESSION => {
            l.node("qualifier", reference_expression::get_qualifier_expression(tree, id));
            l.node("name", reference_expression::get_reference_name_element(tree, id));
            l.node("params", reference_expression::get_parameter_list(tree, id));
        }
        NodeKind::METHOD_REF_EXPRESSION => {
            l.node("qualifier", method_reference_expression::get_qualifier(tree, id));
            l.node("name", method_reference_expression::get_reference_name_element(tree, id));
            l.bool("isConstructor", method_reference_expression::is_constructor(tree, id));
        }
        NodeKind::THIS_EXPRESSION => l.node("qualifier", this_expression::get_qualifier(tree, id)),
        NodeKind::SUPER_EXPRESSION => l.node("qualifier", super_expression::get_qualifier(tree, id)),
        NodeKind::METHOD_CALL_EXPRESSION => {
            l.node("methodExpression", method_call_expression::get_method_expression(tree, id));
            l.node("argumentList", method_call_expression::get_argument_list(tree, id));
        }
        NodeKind::EXPRESSION_LIST => l.list("expressions", expression_list::get_expressions(tree, id)),
        NodeKind::NEW_EXPRESSION => {
            l.node("qualifier", new_expression::get_qualifier(tree, id));
            l.node("classReference", new_expression::get_class_reference(tree, id));
            l.node("classOrAnonymous", new_expression::get_class_or_anonymous_class_reference(tree, id));
            l.node("anonymousClass", new_expression::get_anonymous_class(tree, id));
            l.node("argumentList", new_expression::get_argument_list(tree, id));
            l.node("typeArgumentList", new_expression::get_type_argument_list(tree, id));
            l.list("arrayDimensions", new_expression::get_array_dimensions(tree, id));
            l.node("arrayInitializer", new_expression::get_array_initializer(tree, id));
            l.bool("isArrayCreation", new_expression::is_array_creation(tree, id));
        }
        NodeKind::IMPORT_STATEMENT | NodeKind::IMPORT_STATIC_STATEMENT => {
            l.bool("isOnDemand", import_statement::is_on_demand(tree, id));
            l.node("importReference", import_statement::get_import_reference(tree, id));
        }
        NodeKind::IMPORT_MODULE_STATEMENT => l.bool("isOnDemand", import_statement::is_on_demand(tree, id)),
        NodeKind::PACKAGE_STATEMENT => {
            l.node("packageReference", package_statement::get_package_reference(tree, id));
            l.text("packageName", &package_statement::get_package_name(tree, id));
        }
        NodeKind::ANNOTATION => {
            let name = annotation::get_name_reference_element(tree, id);
            l.node("nameReference", name);
            match name {
                Some(name) => l.text("referenceText", &java_source_util::get_reference_text(tree, name)),
                None => l.raw("referenceText", format_args!("-")),
            }
            l.node("parameterList", annotation::get_parameter_list(tree, id));
        }
        NodeKind::TYPE_CAST_EXPRESSION => {
            l.node("castType", type_cast_expression::get_cast_type(tree, id));
            l.node("operand", type_cast_expression::get_operand(tree, id));
        }
        NodeKind::INSTANCE_OF_EXPRESSION => {
            l.node("operand", instance_of_expression::get_operand(tree, id));
            l.node("checkType", instance_of_expression::get_check_type(tree, id));
            l.node("pattern", instance_of_expression::get_pattern(tree, id));
        }
        NodeKind::LAMBDA_EXPRESSION => {
            l.node("parameterList", lambda_expression::get_parameter_list(tree, id));
            l.node("body", lambda_expression::get_body(tree, id));
        }
        NodeKind::PARAMETER_LIST => l.list("parameters", parameter_list::get_parameters(tree, id)),
        NodeKind::RECORD_HEADER => l.list("components", record_header::get_record_components(tree, id)),
        NodeKind::TYPE_PARAMETER_LIST => l.list("typeParameters", type_parameter_list::get_type_parameters(tree, id)),
        _ => {}
    }
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::CLASS_NAME_KIND => "CLASS_NAME_KIND",
        Kind::PACKAGE_NAME_KIND => "PACKAGE_NAME_KIND",
        Kind::CLASS_OR_PACKAGE_NAME_KIND => "CLASS_OR_PACKAGE_NAME_KIND",
        Kind::CLASS_FQ_NAME_KIND => "CLASS_FQ_NAME_KIND",
        Kind::CLASS_FQ_OR_PACKAGE_NAME_KIND => "CLASS_FQ_OR_PACKAGE_NAME_KIND",
        Kind::CLASS_IN_QUALIFIED_NEW_KIND => "CLASS_IN_QUALIFIED_NEW_KIND",
    }
}
