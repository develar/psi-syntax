//! The composite PSI element types of `StarlarkElementTypes`.

macro_rules! nodes {
    ($($name:ident = $debug:literal, $psi:literal;)*) => {
        #[allow(non_camel_case_types)]
        #[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
        #[repr(u8)]
        pub enum NodeKind {
            $($name,)*
        }

        impl NodeKind {
            pub const ALL: &'static [NodeKind] = &[$(NodeKind::$name,)*];

            pub const fn debug_name(self) -> &'static str {
                match self { $(NodeKind::$name => $debug,)* }
            }

            pub const fn psi_class(self) -> &'static str {
                match self { $(NodeKind::$name => $psi,)* }
            }
        }
    };
}

nodes! {
    FILE = "FILE", "FILE";
    ERROR_ELEMENT = "ERROR_ELEMENT", "PsiErrorElement";
    ASSIGNMENT_STATEMENT = "ASSIGNMENT_STATEMENT", "StarlarkAssignmentStatement";
    AUG_ASSIGNMENT_STATEMENT = "AUG_ASSIGNMENT_STATEMENT", "StarlarkAugAssignmentStatement";
    BREAK_STATEMENT = "BREAK_STATEMENT", "StarlarkBreakStatement";
    CONTINUE_STATEMENT = "CONTINUE_STATEMENT", "StarlarkContinueStatement";
    EXPRESSION_STATEMENT = "EXPRESSION_STATEMENT", "StarlarkExpressionStatement";
    FOR_STATEMENT = "FOR_STATEMENT", "StarlarkForStatement";
    IF_STATEMENT = "IF_STATEMENT", "StarlarkIfStatement";
    LOAD_STATEMENT = "LOAD_STATEMENT", "StarlarkLoadStatement";
    FILENAME_LOAD_VALUE = "FILENAME_LOAD_VALUE", "StarlarkFilenameLoadValue";
    NAMED_LOAD_VALUE = "NAMED_LOAD_VALUE", "StarlarkNamedLoadValue";
    STRING_LOAD_VALUE = "STRING_LOAD_VALUE", "StarlarkStringLoadValue";
    PASS_STATEMENT = "PASS_STATEMENT", "StarlarkAugAssignmentStatement";
    RETURN_STATEMENT = "RETURN_STATEMENT", "StarlarkReturnStatement";
    STATEMENT_LIST = "STATEMENT_LIST", "StarlarkStatementList";
    FUNCTION_DECLARATION = "FUNCTION_DECLARATION", "StarlarkFunctionDeclaration";
    MANDATORY_PARAMETER = "MANDATORY_PARAMETER", "StarlarkMandatoryParameter";
    OPTIONAL_PARAMETER = "OPTIONAL_PARAMETER", "StarlarkOptionalParameter";
    VARIADIC_PARAMETER = "VARIADIC_PARAMETER", "StarlarkVariadicParameter";
    KEYWORD_VARIADIC_PARAMETER = "KEYWORD_VARIADIC_PARAMETER", "StarlarkKeywordVariadicParameter";
    KEYWORD_ONLY_BOUNDARY = "KEYWORD_ONLY_BOUNDARY", "StarlarkKeywordOnlyBoundary";
    PARAMETER_LIST = "PARAMETER_LIST", "StarlarkParameterList";
    ARGUMENT_EXPRESSION = "ARGUMENT_EXPRESSION", "StarlarkArgumentExpression";
    ARGUMENT_LIST = "ARGUMENT_LIST", "StarlarkArgumentList";
    BINARY_EXPRESSION = "BINARY_EXPRESSION", "StarlarkBinaryExpression";
    CALL_EXPRESSION = "CALL_EXPRESSION", "StarlarkCallExpression";
    CONDITIONAL_EXPRESSION = "CONDITIONAL_EXPRESSION", "StarlarkConditionalExpression";
    DICT_COMP_EXPRESSION = "DICT_COMP_EXPRESSION", "StarlarkDictCompExpression";
    DICT_LITERAL_EXPRESSION = "DICT_LITERAL_EXPRESSION", "StarlarkDictLiteralExpression";
    DOUBLE_STAR_EXPRESSION = "DOUBLE_STAR_EXPRESSION", "StarlarkDoubleStarExpression";
    EMPTY_EXPRESSION = "EMPTY_EXPRESSION", "StarlarkEmptyExpression";
    FALSE_LITERAL_EXPRESSION = "FALSE_LITERAL_EXPRESSION", "StarlarkFalseLiteralExpression";
    FLOAT_LITERAL_EXPRESSION = "FLOAT_LITERAL_EXPRESSION", "StarlarkFloatLiteralExpression";
    GENERATOR_EXPRESSION = "GENERATOR_EXPRESSION", "StarlarkGeneratorExpression";
    GLOB_EXPRESSION = "GLOB_EXPRESSION", "StarlarkGlobExpression";
    TARGET_EXPRESSION = "TARGET_EXPRESSION", "StarlarkTargetExpression";
    INTEGER_LITERAL_EXPRESSION = "INTEGER_LITERAL_EXPRESSION", "StarlarkIntegerLiteralExpression";
    KEY_VALUE_EXPRESSION = "KEY_VALUE_EXPRESSION", "StarlarkKeyValueExpression";
    LAMBDA_EXPRESSION = "LAMBDA_EXPRESSION", "StarlarkLambdaExpression";
    LIST_COMP_EXPRESSION = "LIST_COMP_EXPRESSION", "StarlarkListCompExpression";
    LIST_LITERAL_EXPRESSION = "LIST_LITERAL_EXPRESSION", "StarlarkListLiteralExpression";
    NAMED_ARGUMENT_EXPRESSION = "NAMED_ARGUMENT_EXPRESSION", "StarlarkNamedArgumentExpression";
    NONE_LITERAL_EXPRESSION = "NONE_LITERAL_EXPRESSION", "StarlarkNoneLiteralExpression";
    PARENTHESIZED_EXPRESSION = "PARENTHESIZED_EXPRESSION", "StarlarkParenthesizedExpression";
    PREFIX_EXPRESSION = "PREFIX_EXPRESSION", "StarlarkPrefixExpression";
    REFERENCE_EXPRESSION = "REFERENCE_EXPRESSION", "StarlarkReferenceExpression";
    SLICE_EXPRESSION = "SLICE_EXPRESSION", "StarlarkSliceExpression";
    SLICE_ITEM = "SLICE_ITEM", "StarlarkSliceItem";
    STAR_ARGUMENT_EXPRESSION = "STAR_ARGUMENT_EXPRESSION", "StarlarkStarArgumentExpression";
    STAR_EXPRESSION = "STAR_EXPRESSION", "StarlarkStarExpression";
    STRING_LITERAL_EXPRESSION = "STRING_LITERAL_EXPRESSION", "StarlarkStringLiteralExpression";
    SUBSCRIPTION_EXPRESSION = "SUBSCRIPTION_EXPRESSION", "StarlarkSubscriptionExpression";
    TRUE_LITERAL_EXPRESSION = "TRUE_LITERAL_EXPRESSION", "StarlarkTrueLiteralExpression";
    TUPLE_EXPRESSION = "TUPLE_EXPRESSION", "StarlarkTupleExpression";
}

impl NodeKind {
    pub const fn is_left_bound(self) -> bool {
        matches!(self, NodeKind::ERROR_ELEMENT)
    }

    pub const fn is_named(self) -> bool {
        matches!(
            self,
            NodeKind::FUNCTION_DECLARATION
                | NodeKind::MANDATORY_PARAMETER
                | NodeKind::OPTIONAL_PARAMETER
                | NodeKind::VARIADIC_PARAMETER
                | NodeKind::KEYWORD_VARIADIC_PARAMETER
                | NodeKind::NAMED_LOAD_VALUE
                | NodeKind::TARGET_EXPRESSION
        )
    }
}

impl psi_builder::NodeType for NodeKind {
    #[inline(always)]
    fn index(self) -> u16 {
        self as u16
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
