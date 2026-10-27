//! Source locations and compile-time diagnostics.

/// A 1-based line and column in the single source file. Columns count Unicode
/// scalar values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub line: u32,
    pub col: u32,
}

/// One compile error, rendered as `PATH:LINE:COLUMN: error: MESSAGE`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub span: Span,
    pub message: String,
}

impl Diagnostic {
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Diagnostic {
            span,
            message: message.into(),
        }
    }

    pub fn render(&self, path: &str) -> String {
        format!("{path}:{self}")
    }
}

/// `LINE:COLUMN: error: MESSAGE`, for source without a path.
impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{}:{}: error: {}",
            self.span.line, self.span.col, self.message
        )
    }
}
