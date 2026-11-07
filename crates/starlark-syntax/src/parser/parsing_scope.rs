//! `ParsingScope` from the IntelliJ Starlark parser.

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub(crate) struct ParsingScope {
    pub(crate) is_function: bool,
    pub(crate) is_suite: bool,
    pub(crate) is_after_semicolon: bool,
}

impl ParsingScope {
    pub(crate) fn with_function(&self) -> ParsingScope {
        ParsingScope { is_function: false, ..self.clone() }
    }

    pub(crate) fn with_suite(&self) -> ParsingScope {
        ParsingScope { is_suite: true, ..self.clone() }
    }
}
