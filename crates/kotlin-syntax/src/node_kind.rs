//! The composite element types: every `KtNodeTypes` constant (with the `KtStubElementTypes` debug names where the
//! node type is a stub type, e.g. `FUN`), the file root, the error element, the code fragments, and the KDoc element
//! types of `KDocElementTypes`.

macro_rules! nodes {
    ($($name:ident = $debug:literal,)*) => {
        /// A composite node type.
        #[allow(non_camel_case_types)]
        #[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
        #[repr(u8)]
        pub enum NodeKind {
            $($name,)*
        }

        impl NodeKind {
            /// Every node type, in declaration order.
            pub const ALL: &'static [NodeKind] = &[$(NodeKind::$name,)*];

            /// `IElementType.toString()`: what the PSI dump prints for a composite of this type.
            pub const fn debug_name(self) -> &'static str {
                match self { $(NodeKind::$name => $debug,)* }
            }
        }
    };
}

nodes! {
    // The roots: KtNodeTypes.KT_FILE (printed as "KtFile: <name>"), the code fragments.
    KT_FILE = "FILE",
    TYPE_CODE_FRAGMENT = "TYPE_CODE_FRAGMENT",
    EXPRESSION_CODE_FRAGMENT = "EXPRESSION_CODE_FRAGMENT",
    BLOCK_CODE_FRAGMENT = "BLOCK_CODE_FRAGMENT",
    // TokenType.ERROR_ELEMENT (printed as "PsiErrorElement:<message>").
    ERROR_ELEMENT = "ERROR_ELEMENT",

    CLASS = "CLASS",
    FUN = "FUN",
    PROPERTY = "PROPERTY",
    DESTRUCTURING_DECLARATION = "DESTRUCTURING_DECLARATION",
    DESTRUCTURING_DECLARATION_ENTRY = "DESTRUCTURING_DECLARATION_ENTRY",
    OBJECT_DECLARATION = "OBJECT_DECLARATION",
    TYPEALIAS = "TYPEALIAS",
    COMPANION_BLOCK = "COMPANION_BLOCK",
    ENUM_ENTRY = "ENUM_ENTRY",
    CLASS_INITIALIZER = "CLASS_INITIALIZER",
    SCRIPT_INITIALIZER = "SCRIPT_INITIALIZER",
    SECONDARY_CONSTRUCTOR = "SECONDARY_CONSTRUCTOR",
    PRIMARY_CONSTRUCTOR = "PRIMARY_CONSTRUCTOR",
    CONTEXT_RECEIVER = "CONTEXT_RECEIVER",
    CONTEXT_PARAMETER_LIST = "CONTEXT_PARAMETER_LIST",
    TYPE_PARAMETER_LIST = "TYPE_PARAMETER_LIST",
    TYPE_PARAMETER = "TYPE_PARAMETER",
    SUPER_TYPE_LIST = "SUPER_TYPE_LIST",
    DELEGATED_SUPER_TYPE_ENTRY = "DELEGATED_SUPER_TYPE_ENTRY",
    SUPER_TYPE_CALL_ENTRY = "SUPER_TYPE_CALL_ENTRY",
    SUPER_TYPE_ENTRY = "SUPER_TYPE_ENTRY",
    PROPERTY_DELEGATE = "PROPERTY_DELEGATE",
    CONSTRUCTOR_CALLEE = "CONSTRUCTOR_CALLEE",
    VALUE_PARAMETER_LIST = "VALUE_PARAMETER_LIST",
    VALUE_PARAMETER = "VALUE_PARAMETER",
    CLASS_BODY = "CLASS_BODY",
    IMPORT_LIST = "IMPORT_LIST",
    FILE_ANNOTATION_LIST = "FILE_ANNOTATION_LIST",
    IMPORT_DIRECTIVE = "IMPORT_DIRECTIVE",
    IMPORT_ALIAS = "IMPORT_ALIAS",
    MODIFIER_LIST = "MODIFIER_LIST",
    ANNOTATION = "ANNOTATION",
    ANNOTATION_ENTRY = "ANNOTATION_ENTRY",
    ANNOTATION_TARGET = "ANNOTATION_TARGET",
    TYPE_ARGUMENT_LIST = "TYPE_ARGUMENT_LIST",
    VALUE_ARGUMENT_LIST = "VALUE_ARGUMENT_LIST",
    VALUE_ARGUMENT = "VALUE_ARGUMENT",
    CONTRACT_EFFECT_LIST = "CONTRACT_EFFECT_LIST",
    CONTRACT_EFFECT = "CONTRACT_EFFECT",
    LAMBDA_ARGUMENT = "LAMBDA_ARGUMENT",
    VALUE_ARGUMENT_NAME = "VALUE_ARGUMENT_NAME",
    TYPE_REFERENCE = "TYPE_REFERENCE",
    USER_TYPE = "USER_TYPE",
    DYNAMIC_TYPE = "DYNAMIC_TYPE",
    FUNCTION_TYPE = "FUNCTION_TYPE",
    FUNCTION_TYPE_RECEIVER = "FUNCTION_TYPE_RECEIVER",
    NULLABLE_TYPE = "NULLABLE_TYPE",
    INTERSECTION_TYPE = "INTERSECTION_TYPE",
    TYPE_PROJECTION = "TYPE_PROJECTION",
    PROPERTY_ACCESSOR = "PROPERTY_ACCESSOR",
    BACKING_FIELD = "BACKING_FIELD",
    INITIALIZER_LIST = "INITIALIZER_LIST",
    TYPE_CONSTRAINT_LIST = "TYPE_CONSTRAINT_LIST",
    TYPE_CONSTRAINT = "TYPE_CONSTRAINT",
    CONSTRUCTOR_DELEGATION_CALL = "CONSTRUCTOR_DELEGATION_CALL",
    CONSTRUCTOR_DELEGATION_REFERENCE = "CONSTRUCTOR_DELEGATION_REFERENCE",
    NULL = "NULL",
    BOOLEAN_CONSTANT = "BOOLEAN_CONSTANT",
    FLOAT_CONSTANT = "FLOAT_CONSTANT",
    CHARACTER_CONSTANT = "CHARACTER_CONSTANT",
    INTEGER_CONSTANT = "INTEGER_CONSTANT",
    STRING_TEMPLATE = "STRING_TEMPLATE",
    LONG_STRING_TEMPLATE_ENTRY = "LONG_STRING_TEMPLATE_ENTRY",
    SHORT_STRING_TEMPLATE_ENTRY = "SHORT_STRING_TEMPLATE_ENTRY",
    LITERAL_STRING_TEMPLATE_ENTRY = "LITERAL_STRING_TEMPLATE_ENTRY",
    ESCAPE_STRING_TEMPLATE_ENTRY = "ESCAPE_STRING_TEMPLATE_ENTRY",
    STRING_INTERPOLATION_PREFIX = "STRING_INTERPOLATION_PREFIX",
    PARENTHESIZED = "PARENTHESIZED",
    RETURN = "RETURN",
    THROW = "THROW",
    CONTINUE = "CONTINUE",
    BREAK = "BREAK",
    IF = "IF",
    CONDITION = "CONDITION",
    THEN = "THEN",
    ELSE = "ELSE",
    TRY = "TRY",
    CATCH = "CATCH",
    FINALLY = "FINALLY",
    FOR = "FOR",
    WHILE = "WHILE",
    DO_WHILE = "DO_WHILE",
    LOOP_RANGE = "LOOP_RANGE",
    BODY = "BODY",
    BLOCK = "BLOCK",
    LAMBDA_EXPRESSION = "LAMBDA_EXPRESSION",
    FUNCTION_LITERAL = "FUNCTION_LITERAL",
    ANNOTATED_EXPRESSION = "ANNOTATED_EXPRESSION",
    REFERENCE_EXPRESSION = "REFERENCE_EXPRESSION",
    ENUM_ENTRY_SUPERCLASS_REFERENCE_EXPRESSION = "ENUM_ENTRY_SUPERCLASS_REFERENCE_EXPRESSION",
    OPERATION_REFERENCE = "OPERATION_REFERENCE",
    LABEL = "LABEL",
    LABEL_QUALIFIER = "LABEL_QUALIFIER",
    THIS_EXPRESSION = "THIS_EXPRESSION",
    SUPER_EXPRESSION = "SUPER_EXPRESSION",
    BINARY_EXPRESSION = "BINARY_EXPRESSION",
    BINARY_WITH_TYPE = "BINARY_WITH_TYPE",
    IS_EXPRESSION = "IS_EXPRESSION",
    PREFIX_EXPRESSION = "PREFIX_EXPRESSION",
    POSTFIX_EXPRESSION = "POSTFIX_EXPRESSION",
    LABELED_EXPRESSION = "LABELED_EXPRESSION",
    CALL_EXPRESSION = "CALL_EXPRESSION",
    ARRAY_ACCESS_EXPRESSION = "ARRAY_ACCESS_EXPRESSION",
    INDICES = "INDICES",
    DOT_QUALIFIED_EXPRESSION = "DOT_QUALIFIED_EXPRESSION",
    CALLABLE_REFERENCE_EXPRESSION = "CALLABLE_REFERENCE_EXPRESSION",
    CLASS_LITERAL_EXPRESSION = "CLASS_LITERAL_EXPRESSION",
    SAFE_ACCESS_EXPRESSION = "SAFE_ACCESS_EXPRESSION",
    OBJECT_LITERAL = "OBJECT_LITERAL",
    WHEN = "WHEN",
    WHEN_ENTRY = "WHEN_ENTRY",
    WHEN_ENTRY_GUARD = "WHEN_ENTRY_GUARD",
    WHEN_CONDITION_IN_RANGE = "WHEN_CONDITION_IN_RANGE",
    WHEN_CONDITION_IS_PATTERN = "WHEN_CONDITION_IS_PATTERN",
    // KtNodeTypes.WHEN_CONDITION_EXPRESSION prints as WHEN_CONDITION_WITH_EXPRESSION.
    WHEN_CONDITION_EXPRESSION = "WHEN_CONDITION_WITH_EXPRESSION",
    COLLECTION_LITERAL_EXPRESSION = "COLLECTION_LITERAL_EXPRESSION",
    PACKAGE_DIRECTIVE = "PACKAGE_DIRECTIVE",
    SCRIPT = "SCRIPT",

    // KDocElementTypes.
    KDOC_SECTION = "KDOC_SECTION",
    KDOC_TAG = "KDOC_TAG",
    KDOC_NAME = "KDOC_NAME",
}

impl psi_builder::NodeType for NodeKind {
    #[inline(always)]
    fn index(self) -> u8 {
        self as u8
    }

    #[inline(always)]
    fn from_index(index: usize) -> Option<NodeKind> {
        NodeKind::ALL.get(index).copied()
    }

    #[inline]
    fn debug_name(self) -> &'static str {
        NodeKind::debug_name(self)
    }
}

impl NodeKind {
    /// `IElementType.isLeftBound()`: the `KtLeftBoundNodeType`s and `TokenType.ERROR_ELEMENT`. An empty marker of such
    /// a type binds the whitespace before it to its left (PsiBuilderImpl.processDone).
    pub const fn is_left_bound(self) -> bool {
        matches!(
            self,
            NodeKind::CONSTRUCTOR_DELEGATION_CALL
                | NodeKind::CONSTRUCTOR_DELEGATION_REFERENCE
                | NodeKind::ERROR_ELEMENT
        )
    }

    /// A collapsed marker of this type is a chameleon the tree reparses from its text, as PSI does when it expands a
    /// lazy element: `BlockExpressionElementType` (BLOCK, via `KotlinParser.parseBlockExpression`) and
    /// `LambdaExpressionElementType` (LAMBDA_EXPRESSION, via `KotlinParser.parseLambdaExpression`).
    pub const fn is_lazy_reparseable(self) -> bool {
        matches!(self, NodeKind::BLOCK | NodeKind::LAMBDA_EXPRESSION)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_names_are_unique() {
        let mut names: Vec<&str> = NodeKind::ALL.iter().map(|n| n.debug_name()).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len());
        assert!(NodeKind::ALL.len() < 256);
    }
}
