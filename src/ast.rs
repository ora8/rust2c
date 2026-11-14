//! Abstrakter Syntaxbaum.
//!
//! Der Parser erzeugt den Baum, die Typprüfung trägt in jedes [`Expr`] den
//! ermittelten Typ (`ty`) ein, und die Codegenerierung liest ihn.

use crate::error::Span;
use crate::types::{IntTy, Type};

#[derive(Debug, Clone)]
pub struct Program {
    pub functions: Vec<Function>,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Type,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub mutable: bool,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    /// Abschliessender Ausdruck ohne `;` – der Wert des Blocks.
    pub tail: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Let {
        name: String,
        mutable: bool,
        /// Angegebener Typ; nach der Typprüfung immer `Some`.
        ty: Option<Type>,
        init: Expr,
        span: Span,
    },
    Expr {
        expr: Expr,
        /// Endete die Anweisung mit `;`?
        semi: bool,
    },
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
    /// Von der Typprüfung gesetzt.
    pub ty: Type,
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Expr {
            kind,
            span,
            ty: Type::Unit,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
        }
    }

    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintKind {
    Print,
    Println,
    Panic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FmtPiece {
    Lit(String),
    /// Index in die Argumentliste.
    Arg(usize),
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    Int(u128, Option<IntTy>),
    Bool(bool),
    Var(String),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// `ziel = wert` bzw. `ziel op= wert`
    Assign(Option<BinOp>, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
    Index(Box<Expr>, Box<Expr>),
    /// `array.len()`
    Len(Box<Expr>),
    Cast(Box<Expr>, Type),
    /// `[wert; n]`
    ArrayRepeat(Box<Expr>, u64),
    /// `[a, b, c]`
    ArrayList(Vec<Expr>),
    /// Der `else`-Zweig ist ein `Block`- oder ein weiterer `If`-Ausdruck.
    If(Box<Expr>, Block, Option<Box<Expr>>),
    While(Box<Expr>, Block),
    Loop(Block),
    For {
        var: String,
        start: Box<Expr>,
        end: Box<Expr>,
        inclusive: bool,
        body: Block,
    },
    Block(Block),
    Break,
    Continue,
    Return(Option<Box<Expr>>),
    Print {
        kind: PrintKind,
        pieces: Vec<FmtPiece>,
        args: Vec<Expr>,
    },
}
