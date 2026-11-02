//! `AbstractLexer.cs`: the lexeme's start, the errors of the token being lexed (`_errors`, `AddError`, `MakeError`,
//! `GetErrors`) and the lexeme's text and width. An error's offset is from the lexeme's start, widths are bytes.

use super::Lexer;
use crate::error_code::{ErrorCode, XmlParseErrorCode};
use crate::green::{DiagArg, DiagnosticId};

impl<'t> Lexer<'t> {
    /// `Start()`.
    #[inline(always)]
    pub(crate) fn start(&mut self) {
        self.lexeme_start_position = self.text_window.position();
        self.errors.clear();
    }

    /// `HasErrors`.
    #[inline(always)]
    pub(crate) fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// `GetErrors()`: a copy of the errors so far, into `out` (cleared first; empty is Roslyn's `null`).
    #[inline]
    pub(crate) fn get_errors(&self, out: &mut Vec<DiagnosticId>) {
        out.clear();
        out.extend_from_slice(&self.errors);
    }

    /// `AddError(position, width, code, args)`.
    pub(crate) fn add_error_at(&mut self, position: u32, width: u32, code: ErrorCode, args: &[DiagArg]) {
        let error = self.make_error_at(position, width, code, args);
        self.add_error(Some(error));
    }

    /// `AddError(code, args)`: zero wide at the lexeme's start.
    pub(crate) fn add_error_code(&mut self, code: ErrorCode, args: &[DiagArg]) {
        let error = self.make_error_code(code, args);
        self.add_error(Some(error));
    }

    /// `AddError(XmlParseErrorCode code, args)`.
    pub(crate) fn add_xml_error(&mut self, code: XmlParseErrorCode, args: &[DiagArg]) {
        let error = self.make_xml_error(code, args);
        self.add_error(Some(error));
    }

    /// `AddError(SyntaxDiagnosticInfo? error)`.
    #[inline]
    pub(crate) fn add_error(&mut self, error: Option<DiagnosticId>) {
        if let Some(error) = error {
            self.errors.push(error);
        }
    }

    /// `MakeError(position, width, code, args)`.
    pub(crate) fn make_error_at(
        &mut self,
        position: u32,
        width: u32,
        code: ErrorCode,
        args: &[DiagArg],
    ) -> DiagnosticId {
        let offset = self.get_lexeme_offset_from_position(position);
        self.arena.make_diagnostic(offset, width as i32, code, args)
    }

    /// `GetLexemeOffsetFromPosition(position)`.
    fn get_lexeme_offset_from_position(&self, position: u32) -> i32 {
        if position >= self.lexeme_start_position {
            (position - self.lexeme_start_position) as i32
        } else {
            position as i32
        }
    }

    /// `GetNonInternedLexemeText()` (and `GetInternedLexemeText()`): the bytes of the lexeme so far.
    #[inline]
    pub(crate) fn get_lexeme_text(&self) -> &'t [u8] {
        self.text_window.get_text(self.lexeme_start_position)
    }

    /// `CurrentLexemeWidth`, in bytes.
    #[inline]
    pub(crate) fn current_lexeme_width(&self) -> u32 {
        self.text_window.position() - self.lexeme_start_position
    }

    /// `MakeError(code, args)`: `new SyntaxDiagnosticInfo(code, args)`, offset and width 0.
    pub(crate) fn make_error_code(&mut self, code: ErrorCode, args: &[DiagArg]) -> DiagnosticId {
        self.arena.make_diagnostic(0, 0, code, args)
    }

    /// `MakeError(XmlParseErrorCode code, args)`: `new XmlSyntaxDiagnosticInfo(0, 0, code, args)`.
    pub(crate) fn make_xml_error(&mut self, code: XmlParseErrorCode, args: &[DiagArg]) -> DiagnosticId {
        self.arena.make_xml_diagnostic(0, 0, code, args)
    }
}
