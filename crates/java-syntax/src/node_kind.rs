//! The composite element types: every `JavaSyntaxElementType` constant, the composite ones of
//! `JavaDocSyntaxElementType` (its lazily parsed types are tokens, see `TokenKind`), `JShellSyntaxElementType`, the
//! error element and the two roots, with their debug names (what `DebugUtil.nodeTreeAsElementTypeToString` prints) and
//! the PSI class each becomes (what `DebugUtil.psiToString` prints, see `dump`).

/// What PSI's `toString()` prints after the class name of a composite (see `dump::psi_to_string`).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Label {
    /// The class name alone (`PsiCodeBlock`).
    Plain,
    /// `<class>:<text>` (`PsiModifierList:public static`).
    Text,
    /// `<class>: <text>`, with a space (`PsiPolyadicExpression: a || b || c`).
    SpaceText,
    /// `<class>:` with nothing after it (`PsiReferenceLink:`).
    Empty,
    /// `<class>:<name>`: the text of its name identifier (`PsiClass:Foo`, `PsiMethod:bar`).
    Name,
    /// `PsiJavaModule:<module name>`.
    ModuleName,
    /// `PsiPackageStatement:<package name>`.
    PackageName,
    /// `PsiTypeElement:DIAMOND`.
    Diamond,
    /// `PsiSwitchExpression: <selector>`.
    SwitchSelector,
    /// `PsiDocTag:<tag name>`, `PsiInlineDocTag:<tag name>`.
    DocTagName,
    /// `PsiSnippetAttribute:<attribute name>`.
    SnippetAttributeName,
    /// `PsiJavaFile:<file name>`.
    File,
    /// `PsiErrorElement:<message>`.
    Error,
    /// `PsiElement(<debug name>)`: no PSI class of its own.
    Element,
}

macro_rules! nodes {
    ($($name:ident = $debug:literal, $psi:literal, $label:expr;)*) => {
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

            /// `SyntaxElementType.toString()`: what `DebugUtil.nodeTreeAsElementTypeToString` prints.
            pub const fn debug_name(self) -> &'static str {
                match self { $(NodeKind::$name => $debug,)* }
            }

            /// The PSI class name `DebugUtil.psiToString` prints (empty for `Label::Element`).
            pub const fn psi_class(self) -> &'static str {
                match self { $(NodeKind::$name => $psi,)* }
            }

            /// What PSI's `toString()` prints after the class name.
            pub const fn label(self) -> Label {
                match self { $(NodeKind::$name => $label,)* }
            }
        }
    };
}

nodes! {
    // The roots: JavaSyntaxElementType.JAVA_FILE (printed as "PsiJavaFile:<name>"), and the IFileElementType "test.java.file" of
    // JavaParsingTestConfiguratorBase, which a parser entry other than the file parser runs under (the parser-partial
    // goldens).
    JAVA_FILE = "java.FILE", "PsiJavaFile", Label::File;
    TEST_JAVA_FILE = "test.java.file", "PsiJavaFile", Label::File;
    // SyntaxTokenTypes.ERROR_ELEMENT (printed as "PsiErrorElement:<message>").
    ERROR_ELEMENT = "ERROR_ELEMENT", "PsiErrorElement", Label::Error;
    // JavaSyntaxElementType.
    CLASS = "CLASS", "PsiClass", Label::Name;
    IMPLICIT_CLASS = "IMPLICIT_CLASS", "PsiImplicitClass", Label::Plain;
    ANONYMOUS_CLASS = "ANONYMOUS_CLASS", "PsiAnonymousClass", Label::Plain;
    // `PsiEnumConstantInitializerImpl.toString()`, its parenthesis unbalanced.
    ENUM_CONSTANT_INITIALIZER = "ENUM_CONSTANT_INITIALIZER", "PsiAnonymousClass (PsiEnumConstantInitializerImpl)):", Label::Plain;
    TYPE_PARAMETER_LIST = "TYPE_PARAMETER_LIST", "PsiTypeParameterList", Label::Plain;
    TYPE_PARAMETER = "TYPE_PARAMETER", "PsiTypeParameter", Label::Name;
    IMPORT_LIST = "IMPORT_LIST", "PsiImportList", Label::Plain;
    IMPORT_STATEMENT = "IMPORT_STATEMENT", "PsiImportStatement", Label::Plain;
    IMPORT_STATIC_STATEMENT = "IMPORT_STATIC_STATEMENT", "PsiImportStaticStatement", Label::Plain;
    IMPORT_MODULE_STATEMENT = "IMPORT_MODULE_STATEMENT", "PsiImportModuleStatement", Label::Plain;
    MODIFIER_LIST = "MODIFIER_LIST", "PsiModifierList", Label::Text;
    ANNOTATION = "ANNOTATION", "PsiAnnotation", Label::Plain;
    NAME_VALUE_PAIR = "NAME_VALUE_PAIR", "PsiNameValuePair", Label::Plain;
    LITERAL_EXPRESSION = "LITERAL_EXPRESSION", "PsiLiteralExpression", Label::Text;
    ANNOTATION_PARAMETER_LIST = "ANNOTATION_PARAMETER_LIST", "PsiAnnotationParameterList", Label::Plain;
    EXTENDS_LIST = "EXTENDS_LIST", "PsiReferenceList", Label::Plain;
    IMPLEMENTS_LIST = "IMPLEMENTS_LIST", "PsiReferenceList", Label::Plain;
    FIELD = "FIELD", "PsiField", Label::Name;
    ENUM_CONSTANT = "ENUM_CONSTANT", "PsiEnumConstant", Label::Name;
    METHOD = "METHOD", "PsiMethod", Label::Name;
    ANNOTATION_METHOD = "ANNOTATION_METHOD", "PsiAnnotationMethod", Label::Name;
    CLASS_INITIALIZER = "CLASS_INITIALIZER", "PsiClassInitializer", Label::Plain;
    PARAMETER = "PARAMETER", "PsiParameter", Label::Name;
    PARAMETER_LIST = "PARAMETER_LIST", "PsiParameterList", Label::Text;
    EXTENDS_BOUND_LIST = "EXTENDS_BOUND_LIST", "PsiReferenceList", Label::Plain;
    THROWS_LIST = "THROWS_LIST", "PsiReferenceList", Label::Plain;
    LAMBDA_EXPRESSION = "LAMBDA_EXPRESSION", "PsiLambdaExpression", Label::Plain;
    METHOD_REF_EXPRESSION = "METHOD_REF_EXPRESSION", "PsiMethodReferenceExpression", Label::Plain;
    MODULE = "MODULE", "PsiJavaModule", Label::ModuleName;
    REQUIRES_STATEMENT = "REQUIRES_STATEMENT", "PsiRequiresStatement", Label::Plain;
    EXPORTS_STATEMENT = "EXPORTS_STATEMENT", "PsiPackageAccessibilityStatement", Label::Plain;
    OPENS_STATEMENT = "OPENS_STATEMENT", "PsiPackageAccessibilityStatement", Label::Plain;
    USES_STATEMENT = "USES_STATEMENT", "PsiUsesStatement", Label::Plain;
    PROVIDES_STATEMENT = "PROVIDES_STATEMENT", "PsiProvidesStatement", Label::Plain;
    PROVIDES_WITH_LIST = "PROVIDES_WITH_LIST", "PsiReferenceList", Label::Plain;
    RECORD_COMPONENT = "RECORD_COMPONENT", "PsiRecordComponent", Label::Name;
    RECORD_HEADER = "RECORD_HEADER", "PsiRecordHeader", Label::Plain;
    PERMITS_LIST = "PERMITS_LIST", "PsiReferenceList", Label::Plain;
    IMPORT_STATIC_REFERENCE = "IMPORT_STATIC_REFERENCE", "PsiImportStaticReferenceElement", Label::Text;
    TYPE = "TYPE", "PsiTypeElement", Label::Text;
    DIAMOND_TYPE = "DIAMOND_TYPE", "PsiTypeElement", Label::Diamond;
    REFERENCE_PARAMETER_LIST = "REFERENCE_PARAMETER_LIST", "PsiReferenceParameterList", Label::Plain;
    JAVA_CODE_REFERENCE = "JAVA_CODE_REFERENCE", "PsiJavaCodeReferenceElement", Label::Text;
    PACKAGE_STATEMENT = "PACKAGE_STATEMENT", "PsiPackageStatement", Label::PackageName;
    LOCAL_VARIABLE = "LOCAL_VARIABLE", "PsiLocalVariable", Label::Name;
    REFERENCE_EXPRESSION = "REFERENCE_EXPRESSION", "PsiReferenceExpression", Label::Text;
    THIS_EXPRESSION = "THIS_EXPRESSION", "PsiThisExpression", Label::Text;
    SUPER_EXPRESSION = "SUPER_EXPRESSION", "PsiSuperExpression", Label::Text;
    PARENTH_EXPRESSION = "PARENTH_EXPRESSION", "PsiParenthesizedExpression", Label::Text;
    METHOD_CALL_EXPRESSION = "METHOD_CALL_EXPRESSION", "PsiMethodCallExpression", Label::Text;
    TYPE_CAST_EXPRESSION = "TYPE_CAST_EXPRESSION", "PsiTypeCastExpression", Label::Text;
    PREFIX_EXPRESSION = "PREFIX_EXPRESSION", "PsiPrefixExpression", Label::Text;
    POSTFIX_EXPRESSION = "POSTFIX_EXPRESSION", "PsiPostfixExpression", Label::Text;
    BINARY_EXPRESSION = "BINARY_EXPRESSION", "PsiBinaryExpression", Label::Text;
    POLYADIC_EXPRESSION = "POLYADIC_EXPRESSION", "PsiPolyadicExpression", Label::SpaceText;
    CONDITIONAL_EXPRESSION = "CONDITIONAL_EXPRESSION", "PsiConditionalExpression", Label::Text;
    ASSIGNMENT_EXPRESSION = "ASSIGNMENT_EXPRESSION", "PsiAssignmentExpression", Label::Text;
    NEW_EXPRESSION = "NEW_EXPRESSION", "PsiNewExpression", Label::Text;
    ARRAY_ACCESS_EXPRESSION = "ARRAY_ACCESS_EXPRESSION", "PsiArrayAccessExpression", Label::Text;
    ARRAY_INITIALIZER_EXPRESSION = "ARRAY_INITIALIZER_EXPRESSION", "PsiArrayInitializerExpression", Label::Text;
    INSTANCE_OF_EXPRESSION = "INSTANCE_OF_EXPRESSION", "PsiInstanceofExpression", Label::Plain;
    CLASS_OBJECT_ACCESS_EXPRESSION = "CLASS_OBJECT_ACCESS_EXPRESSION", "PsiClassObjectAccessExpression", Label::Text;
    TEMPLATE_EXPRESSION = "TEMPLATE_EXPRESSION", "PsiTemplateExpression", Label::Plain;
    TEMPLATE = "TEMPLATE", "PsiTemplate", Label::Plain;
    EMPTY_EXPRESSION = "EMPTY_EXPRESSION", "PsiExpression(empty)", Label::Plain;
    EXPRESSION_LIST = "EXPRESSION_LIST", "PsiExpressionList", Label::Plain;
    EMPTY_STATEMENT = "EMPTY_STATEMENT", "PsiEmptyStatement", Label::Plain;
    BLOCK_STATEMENT = "BLOCK_STATEMENT", "PsiBlockStatement", Label::Plain;
    EXPRESSION_STATEMENT = "EXPRESSION_STATEMENT", "PsiExpressionStatement", Label::Plain;
    EXPRESSION_LIST_STATEMENT = "EXPRESSION_LIST_STATEMENT", "PsiExpressionListStatement", Label::Plain;
    DECLARATION_STATEMENT = "DECLARATION_STATEMENT", "PsiDeclarationStatement", Label::Plain;
    IF_STATEMENT = "IF_STATEMENT", "PsiIfStatement", Label::Plain;
    WHILE_STATEMENT = "WHILE_STATEMENT", "PsiWhileStatement", Label::Plain;
    FOR_STATEMENT = "FOR_STATEMENT", "PsiForStatement", Label::Plain;
    FOREACH_STATEMENT = "FOREACH_STATEMENT", "PsiForeachStatement", Label::Plain;
    FOREACH_PATTERN_STATEMENT = "FOREACH_PATTERN_STATEMENT", "PsiForeachPatternStatement", Label::Plain;
    DO_WHILE_STATEMENT = "DO_WHILE_STATEMENT", "PsiDoWhileStatement", Label::Plain;
    SWITCH_STATEMENT = "SWITCH_STATEMENT", "PsiSwitchStatement", Label::Plain;
    SWITCH_EXPRESSION = "SWITCH_EXPRESSION", "PsiSwitchExpression", Label::SwitchSelector;
    SWITCH_LABEL_STATEMENT = "SWITCH_LABEL_STATEMENT", "PsiSwitchLabelStatement", Label::Plain;
    SWITCH_LABELED_RULE = "SWITCH_LABELED_RULE", "PsiSwitchLabeledRule", Label::Plain;
    BREAK_STATEMENT = "BREAK_STATEMENT", "PsiBreakStatement", Label::Plain;
    YIELD_STATEMENT = "YIELD_STATEMENT", "PsiYieldStatement", Label::Plain;
    CONTINUE_STATEMENT = "CONTINUE_STATEMENT", "PsiContinueStatement", Label::Plain;
    RETURN_STATEMENT = "RETURN_STATEMENT", "PsiReturnStatement", Label::Plain;
    THROW_STATEMENT = "THROW_STATEMENT", "PsiThrowStatement", Label::Plain;
    SYNCHRONIZED_STATEMENT = "SYNCHRONIZED_STATEMENT", "PsiSynchronizedStatement", Label::Plain;
    TRY_STATEMENT = "TRY_STATEMENT", "PsiTryStatement", Label::Plain;
    RESOURCE_LIST = "RESOURCE_LIST", "PsiResourceList", Label::Text;
    RESOURCE_VARIABLE = "RESOURCE_VARIABLE", "PsiResourceVariable", Label::Name;
    RESOURCE_EXPRESSION = "RESOURCE_EXPRESSION", "PsiResourceExpression", Label::Plain;
    CATCH_SECTION = "CATCH_SECTION", "PsiCatchSection", Label::Plain;
    LABELED_STATEMENT = "LABELED_STATEMENT", "PsiLabeledStatement", Label::Plain;
    ASSERT_STATEMENT = "ASSERT_STATEMENT", "PsiAssertStatement", Label::Plain;
    ANNOTATION_ARRAY_INITIALIZER = "ANNOTATION_ARRAY_INITIALIZER", "PsiArrayInitializerMemberValue", Label::Text;
    RECEIVER_PARAMETER = "RECEIVER", "PsiReceiverParameter", Label::Plain;
    MODULE_REFERENCE = "MODULE_REFERENCE", "PsiJavaModuleReference", Label::Plain;
    TYPE_TEST_PATTERN = "TYPE_TEST_PATTERN", "PsiTypeTestPattern", Label::Plain;
    UNNAMED_PATTERN = "UNNAMED_PATTERN", "PsiUnnamedPattern", Label::Plain;
    PATTERN_VARIABLE = "PATTERN_VARIABLE", "PsiPatternVariable", Label::Name;
    DECONSTRUCTION_PATTERN = "DECONSTRUCTION_PATTERN", "PsiDeconstructionPattern", Label::Plain;
    DECONSTRUCTION_LIST = "DECONSTRUCTION_LIST", "PsiDeconstructionList", Label::Plain;
    DECONSTRUCTION_PATTERN_VARIABLE = "DECONSTRUCTION_PATTERN_VARIABLE", "PsiDeconstructionPatternVariable", Label::Plain;
    DEFAULT_CASE_LABEL_ELEMENT = "DEFAULT_CASE_LABEL_ELEMENT", "PsiDefaultLabelElement", Label::Plain;
    CASE_LABEL_ELEMENT_LIST = "CASE_LABEL_ELEMENT_LIST", "PsiCaseLabelElementList", Label::Plain;
    CODE_BLOCK = "CODE_BLOCK", "PsiCodeBlock", Label::Plain;
    MEMBERS = "MEMBERS", "", Label::Element;
    STATEMENTS = "STATEMENTS", "", Label::Element;
    EXPRESSION_TEXT = "EXPRESSION_TEXT", "", Label::Element;
    REFERENCE_TEXT = "REFERENCE_TEXT", "", Label::Element;
    TYPE_WITH_DISJUNCTIONS_TEXT = "TYPE_WITH_DISJUNCTIONS_TEXT", "", Label::Element;
    TYPE_WITH_CONJUNCTIONS_TEXT = "TYPE_WITH_CONJUNCTIONS_TEXT", "", Label::Element;
    DUMMY_ELEMENT = "DUMMY_ELEMENT", "", Label::Element;
    // JavaDocSyntaxElementType (its lazily parsed types are tokens, see `TokenKind`).
    DOC_TAG = "DOC_TAG", "PsiDocTag", Label::DocTagName;
    DOC_INLINE_TAG = "DOC_INLINE_TAG", "PsiInlineDocTag", Label::DocTagName;
    DOC_METHOD_OR_FIELD_REF = "DOC_METHOD_OR_FIELD_REF", "", Label::Element;
    DOC_FRAGMENT_REF = "DOC_FRAGMENT_REF", "", Label::Element;
    DOC_FRAGMENT_NAME = "DOC_FRAGMENT_NAME", "", Label::Element;
    DOC_PARAMETER_REF = "DOC_PARAMETER_REF", "", Label::Element;
    DOC_TAG_VALUE_ELEMENT = "DOC_TAG_VALUE_ELEMENT", "", Label::Element;
    DOC_SNIPPET_TAG = "DOC_SNIPPET_TAG", "PsiSnippetDocTag", Label::Plain;
    DOC_SNIPPET_TAG_VALUE = "DOC_SNIPPET_TAG_VALUE", "PsiSnippetDocTagValue", Label::Plain;
    DOC_SNIPPET_BODY = "DOC_SNIPPET_BODY", "PsiSnippetDocTagBody", Label::Plain;
    DOC_SNIPPET_ATTRIBUTE = "DOC_SNIPPET_ATTRIBUTE", "PsiSnippetAttribute", Label::SnippetAttributeName;
    DOC_SNIPPET_ATTRIBUTE_LIST = "DOC_SNIPPET_ATTRIBUTE_LIST", "PsiSnippetAttributeList", Label::Plain;
    DOC_SNIPPET_ATTRIBUTE_VALUE = "DOC_SNIPPET_ATTRIBUTE_VALUE", "PsiSnippetAttributeValue", Label::Text;
    DOC_MARKDOWN_CODE_BLOCK = "DOC_CODE_BLOCK", "PsiMarkdownCodeBlock", Label::Empty;
    DOC_MARKDOWN_REFERENCE_LINK = "DOC_REFERENCE_LINK", "PsiReferenceLink", Label::Empty;
    DOC_MARKDOWN_REFERENCE_LABEL = "DOC_REFERENCE_LABEL", "PsiReferenceLabel", Label::Empty;
    DOC_MARKDOWN_LINK = "DOC_MARKDOWN_LINK", "", Label::Element;
    // JShellSyntaxElementType.
    JSHELL_FILE = "JSHELL_FILE", "", Label::Element;
    JSHELL_ROOT_CLASS = "JSHELL_ROOT_CLASS", "", Label::Element;
    JSHELL_STATEMENTS_HOLDER = "JSHELL_STATEMENTS_HOLDER", "", Label::Element;
    JSHELL_IMPORT_HOLDER = "JSHELL_IMPORT_HOLDER", "", Label::Element;
}

impl NodeKind {
    /// `JavaWhitespaceOrCommentBindingPolicy.isLeftBound`: an empty marker of this type binds the whitespace before it
    /// to its left (`SyntaxTreeBuilderImpl.processDone`).
    pub const fn is_left_bound(self) -> bool {
        matches!(
            self,
            NodeKind::ERROR_ELEMENT
                | NodeKind::TYPE_PARAMETER_LIST
                | NodeKind::NAME_VALUE_PAIR
                | NodeKind::ANNOTATION_PARAMETER_LIST
                | NodeKind::EXTENDS_LIST
                | NodeKind::IMPLEMENTS_LIST
                | NodeKind::EXTENDS_BOUND_LIST
                | NodeKind::THROWS_LIST
                | NodeKind::PROVIDES_WITH_LIST
                | NodeKind::PERMITS_LIST
                | NodeKind::REFERENCE_PARAMETER_LIST
                | NodeKind::EMPTY_EXPRESSION
                | NodeKind::EXPRESSION_LIST
        )
    }
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
