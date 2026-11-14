//! rust2c – ein kleiner Compiler, der eine Teilmenge von Rust nach C übersetzt.
//!
//! Die Pipeline:
//!
//! ```text
//! Quelltext ─► lexer ─► parser ─► typeck ─► codegen ─► C11-Quelltext
//!              Tokens    AST      AST+Typen
//! ```

pub mod ast;
pub mod codegen;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod typeck;
pub mod types;

pub use error::{CompileError, Span};

/// Übersetzt ein Programm vollständig nach C.
pub fn compile(src: &str) -> Result<String, CompileError> {
    let mut program = parser::parse(src)?;
    typeck::check(&mut program)?;
    Ok(codegen::generate(&program))
}
