//! `PsiModifierListImpl` (`java/java-psi-impl/src/com/intellij/psi/impl/source/PsiModifierListImpl.java`): a
//! `MODIFIER_LIST`'s explicit and implicit modifiers and its annotations.

use super::modifier::{Modifier, Modifiers};
use super::{class, field, is_psi_class, is_psi_file, java_shared_impl_util, method, parameter, type_element};
use crate::node_kind::NodeKind;
use crate::tree::{NodeId, Tree};

/// `hasModifierProperty(name)` (`PsiModifierListImpl.java:111`): whether `calc_modifiers` holds it.
pub fn has_modifier_property(tree: &Tree, list: NodeId, name: Modifier) -> bool {
    calc_modifiers(tree, list).contains(name)
}

/// `calcModifiers()` (`PsiModifierListImpl.java:119`): the explicit modifiers, the implicit ones, and
/// `PACKAGE_LOCAL` when neither has `public`, `protected` or `private`. `PsiAugmentProvider.transformModifierProperties`
/// is left out (the identity without providers).
pub fn calc_modifiers(tree: &Tree, list: NodeId) -> Modifiers {
    let explicit = calc_explicit_modifiers(tree, list);
    let modifiers = explicit.union(calc_implicit_modifiers(tree, list, explicit));
    if modifiers.contains(Modifier::PUBLIC)
        || modifiers.contains(Modifier::PROTECTED)
        || modifiers.contains(Modifier::PRIVATE)
    {
        modifiers
    } else {
        modifiers.with(Modifier::PACKAGE_LOCAL)
    }
}

/// `calcExplicitModifiers()` (`PsiModifierListImpl.java:129`): the modifier of each keyword child (annotations are no
/// modifiers). `public private` holds both.
pub fn calc_explicit_modifiers(tree: &Tree, list: NodeId) -> Modifiers {
    tree.children(list)
        .filter_map(|c| tree.token_kind(c).and_then(Modifier::of_keyword))
        .fold(Modifiers::EMPTY, Modifiers::with)
}

/// `calcImplicitModifiers(explicitModifiers)` (`PsiModifierListImpl.java:149`), by the list's parent:
/// - a class: `public static` in an interface; an interface `abstract`, and `static` unless top level; a value class
///   without `abstract` `final`; a record `final`, and `static` unless top level; an enum `static` unless top level,
///   `sealed` when a constant has a body else `final`, and `abstract` when one of its own methods is;
/// - a method of an interface: `public` unless `private`, and then `abstract` unless `default` or `static`; a
///   constructor of an enum `private`;
/// - a record component `final`; an enum constant `public static final`; a field of an interface `public static
///   final`, of a value class `final` unless `static`;
/// - a catch parameter of a disjunction type (`catch (A | B e)`) `final`; a resource variable `final`.
///
/// "Top level" is a class whose parent is the file; a local class is not, nor a class of an implicit class.
pub fn calc_implicit_modifiers(tree: &Tree, list: NodeId, explicit: Modifiers) -> Modifiers {
    let mut implicit = Modifiers::EMPTY;
    let Some(parent) = tree.parent(list) else { return implicit };
    match tree.node_kind(parent) {
        _ if is_psi_class(tree, parent) => {
            // `parent.getContext()`: a class without a stub is its parent's.
            let grand_parent = tree.parent(parent);
            let top_level = grand_parent.is_some_and(|g| is_psi_file(tree, g));
            if grand_parent.is_some_and(|g| is_psi_class(tree, g) && class::is_interface(tree, g)) {
                implicit = implicit.with(Modifier::PUBLIC).with(Modifier::STATIC);
            }
            if class::is_interface(tree, parent) {
                implicit = implicit.with(Modifier::ABSTRACT);
                // nested or local interface is implicitly static
                if !top_level {
                    implicit = implicit.with(Modifier::STATIC);
                }
            } else if explicit.contains(Modifier::VALUE) && !explicit.contains(Modifier::ABSTRACT) {
                implicit = implicit.with(Modifier::FINAL);
            }
            if class::is_record(tree, parent) {
                if !top_level {
                    implicit = implicit.with(Modifier::STATIC);
                }
                implicit = implicit.with(Modifier::FINAL);
            }
            if class::is_enum(tree, parent) {
                if !top_level {
                    implicit = implicit.with(Modifier::STATIC);
                }
                let has_sub_class = class::get_own_fields(tree, parent).any(|f| {
                    tree.node_kind(f) == Some(NodeKind::ENUM_CONSTANT)
                        && super::enum_constant::get_initializing_class(tree, f).is_some()
                });
                implicit = implicit.with(if has_sub_class { Modifier::SEALED } else { Modifier::FINAL });
                if class::get_own_methods(tree, parent)
                    .any(|m| super::has_modifier_property(tree, m, Modifier::ABSTRACT))
                {
                    implicit = implicit.with(Modifier::ABSTRACT);
                }
            }
        }
        Some(NodeKind::METHOD | NodeKind::ANNOTATION_METHOD) => {
            let a_class = method::get_containing_class(tree, parent);
            if a_class.is_some_and(|c| class::is_interface(tree, c)) {
                if !explicit.contains(Modifier::PRIVATE) {
                    implicit = implicit.with(Modifier::PUBLIC);
                    if !explicit.contains(Modifier::DEFAULT) && !explicit.contains(Modifier::STATIC) {
                        implicit = implicit.with(Modifier::ABSTRACT);
                    }
                }
            } else if a_class.is_some_and(|c| class::is_enum(tree, c)) && method::is_constructor(tree, parent) {
                implicit = implicit.with(Modifier::PRIVATE);
            }
        }
        Some(NodeKind::RECORD_COMPONENT) => implicit = implicit.with(Modifier::FINAL),
        Some(NodeKind::ENUM_CONSTANT) => {
            implicit = implicit.with(Modifier::PUBLIC).with(Modifier::STATIC).with(Modifier::FINAL);
        }
        Some(NodeKind::FIELD) => {
            if let Some(a_class) = field::get_containing_class(tree, parent) {
                if class::is_interface(tree, a_class) {
                    implicit = implicit.with(Modifier::PUBLIC).with(Modifier::STATIC).with(Modifier::FINAL);
                } else if class::is_value_class(tree, a_class) && !explicit.contains(Modifier::STATIC) {
                    implicit = implicit.with(Modifier::FINAL);
                }
            }
        }
        // `PsiPatternVariable extends PsiParameter`, but its parent is a pattern, never a catch section.
        Some(NodeKind::PARAMETER | NodeKind::PATTERN_VARIABLE)
            if tree.parent(parent).and_then(|g| tree.node_kind(g)) == Some(NodeKind::CATCH_SECTION)
                && is_disjunction_type(tree, parent) =>
        {
            implicit = implicit.with(Modifier::FINAL);
        }
        Some(NodeKind::RESOURCE_VARIABLE) => implicit = implicit.with(Modifier::FINAL),
        _ => {}
    }
    implicit
}

/// `parameter.getType() instanceof PsiDisjunctionType` (`PsiParameterImpl.java:171`): the type element's type is a
/// disjunction and no C-style bracket follows the name, which would make it an array of one. A parameter without a
/// type element or a name has none (IntelliJ asserts or throws).
fn is_disjunction_type(tree: &Tree, parameter: NodeId) -> bool {
    let Some(type_element) = parameter::get_type_element(tree, parameter) else { return false };
    let Some(name) = super::get_name_identifier(tree, parameter) else { return false };
    type_element::calculate_type(tree, type_element).kind == type_element::TypeKind::Disjunction
        && java_shared_impl_util::collect_annotations(tree, name).next().is_none()
}

/// `hasExplicitModifier(name)` (`PsiModifierListImpl.java:249`): whether a child is the modifier's keyword; never for
/// `PACKAGE_LOCAL`.
pub fn has_explicit_modifier(tree: &Tree, list: NodeId, name: Modifier) -> bool {
    name.keyword().is_some_and(|keyword| tree.first_child_of_kind(list, keyword).is_some())
}

/// `getAnnotations()` (`PsiModifierListImpl.java:344`): the `ANNOTATION` children in order (no
/// `PsiAugmentProvider.collectAugments`).
pub fn get_annotations(tree: &Tree, list: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    super::children_of_kind(tree, list, NodeKind::ANNOTATION)
}
