//! Fehlertyp und Quelltextpositionen.

use std::fmt;

/// Position im Quelltext (1-basiert).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub line: usize,
    pub col: usize,
}

impl Span {
    pub fn new(line: usize, col: usize) -> Self {
        Span { line, col }
    }
}

/// Ein Fehler aus einer beliebigen Compilerphase.
#[derive(Debug, Clone)]
pub struct CompileError {
    pub msg: String,
    pub span: Span,
}

pub type Result<T> = std::result::Result<T, CompileError>;

impl CompileError {
    pub fn new(msg: impl Into<String>, span: Span) -> Self {
        CompileError {
            msg: msg.into(),
            span,
        }
    }

    /// Formatiert den Fehler im Stil von rustc, mit Quelltextzeile und Markierung.
    pub fn render(&self, src: &str, file: &str) -> String {
        let line_text = src
            .lines()
            .nth(self.span.line.saturating_sub(1))
            .unwrap_or("");
        let num = self.span.line.to_string();
        let pad = " ".repeat(num.len());
        let marker: String = line_text
            .chars()
            .take(self.span.col.saturating_sub(1))
            .map(|c| if c == '\t' { '\t' } else { ' ' })
            .collect();
        format!(
            "fehler: {msg}\n{pad}--> {file}:{line}:{col}\n{pad} |\n{num} | {line_text}\n{pad} | {marker}^\n",
            msg = self.msg,
            line = self.span.line,
            col = self.span.col,
        )
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.span.line, self.span.col, self.msg)
    }
}

impl std::error::Error for CompileError {}
