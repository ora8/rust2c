//! Rekursiv absteigender Parser mit Präzedenzklettern für binäre Operatoren.
//!
//! Die Operatorrangfolge entspricht Rust (von schwach nach stark):
//! Zuweisung, `||`, `&&`, Vergleiche, `|`, `^`, `&`, `<<`/`>>`, `+`/`-`,
//! `*`/`/`/`%`, `as`, unäres `-`/`!`, Postfix (`()`, `[]`, `.len()`).

use crate::ast::*;
use crate::error::{CompileError, Result, Span};
use crate::lexer::{Tok, Token, lex};
use crate::types::{IntTy, Type};

pub fn parse(src: &str) -> Result<Program> {
    let tokens = lex(src)?;
    Parser { tokens, pos: 0 }.program()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

fn binop(t: &Tok) -> Option<(BinOp, u8)> {
    Some(match t {
        Tok::OrOr => (BinOp::Or, 1),
        Tok::AndAnd => (BinOp::And, 2),
        Tok::EqEq => (BinOp::Eq, 3),
        Tok::Ne => (BinOp::Ne, 3),
        Tok::Lt => (BinOp::Lt, 3),
        Tok::Le => (BinOp::Le, 3),
        Tok::Gt => (BinOp::Gt, 3),
        Tok::Ge => (BinOp::Ge, 3),
        Tok::Pipe => (BinOp::BitOr, 4),
        Tok::Caret => (BinOp::BitXor, 5),
        Tok::Amp => (BinOp::BitAnd, 6),
        Tok::Shl => (BinOp::Shl, 7),
        Tok::Shr => (BinOp::Shr, 7),
        Tok::Plus => (BinOp::Add, 8),
        Tok::Minus => (BinOp::Sub, 8),
        Tok::Star => (BinOp::Mul, 9),
        Tok::Slash => (BinOp::Div, 9),
        Tok::Percent => (BinOp::Rem, 9),
        _ => return None,
    })
}

/// `Some(None)` für `=`, `Some(Some(op))` für `op=`.
fn assign_op(t: &Tok) -> Option<Option<BinOp>> {
    Some(match t {
        Tok::Eq => None,
        Tok::PlusEq => Some(BinOp::Add),
        Tok::MinusEq => Some(BinOp::Sub),
        Tok::StarEq => Some(BinOp::Mul),
        Tok::SlashEq => Some(BinOp::Div),
        Tok::PercentEq => Some(BinOp::Rem),
        Tok::AmpEq => Some(BinOp::BitAnd),
        Tok::PipeEq => Some(BinOp::BitOr),
        Tok::CaretEq => Some(BinOp::BitXor),
        Tok::ShlEq => Some(BinOp::Shl),
        Tok::ShrEq => Some(BinOp::Shr),
        _ => return None,
    })
}

enum ArgRef {
    Next,
    Index(usize),
    Named(String),
}

enum RawPiece {
    Lit(String),
    Arg(ArgRef),
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Zerlegt einen Formatstring wie `"x = {}, y = {y}, {{wörtlich}}"`.
fn parse_format(s: &str, span: Span) -> Result<Vec<RawPiece>> {
    let err = |m: &str| Err(CompileError::new(m.to_string(), span));
    let mut out = Vec::new();
    let mut lit = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                lit.push('{');
            }
            '{' => {
                let mut inner = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(ch) => inner.push(ch),
                        None => return err("nicht geschlossenes `{` im Formatstring"),
                    }
                }
                if !lit.is_empty() {
                    out.push(RawPiece::Lit(std::mem::take(&mut lit)));
                }
                let inner = inner.trim();
                let arg = if inner.is_empty() {
                    ArgRef::Next
                } else if inner.contains(':') {
                    return err("Formatoptionen (`{:...}`) werden nicht unterstützt");
                } else if inner.chars().all(|c| c.is_ascii_digit()) {
                    match inner.parse() {
                        Ok(i) => ArgRef::Index(i),
                        Err(_) => return err("ungültiger Argumentindex im Formatstring"),
                    }
                } else if is_ident(inner) {
                    ArgRef::Named(inner.to_string())
                } else {
                    return Err(CompileError::new(
                        format!("ungültiger Platzhalter `{{{inner}}}` im Formatstring"),
                        span,
                    ));
                };
                out.push(RawPiece::Arg(arg));
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                lit.push('}');
            }
            '}' => return err("einzelnes `}` im Formatstring (als `}}` schreiben)"),
            _ => lit.push(c),
        }
    }
    if !lit.is_empty() {
        out.push(RawPiece::Lit(lit));
    }
    Ok(out)
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.tokens[self.pos].tok
    }

    fn span(&self) -> Span {
        self.tokens[self.pos].span
    }

    fn bump(&mut self) -> Token {
        let t = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn at(&self, t: &Tok) -> bool {
        self.peek() == t
    }

    fn eat(&mut self, t: &Tok) -> bool {
        if self.at(t) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn error<T>(&self, msg: impl Into<String>) -> Result<T> {
        Err(CompileError::new(msg, self.span()))
    }

    fn expect(&mut self, t: &Tok) -> Result<Span> {
        if self.at(t) {
            Ok(self.bump().span)
        } else {
            self.error(format!("{t} erwartet, gefunden {}", self.peek()))
        }
    }

    fn ident(&mut self) -> Result<(String, Span)> {
        match self.peek().clone() {
            Tok::Ident(name) => {
                let span = self.bump().span;
                Ok((name, span))
            }
            other => self.error(format!("Bezeichner erwartet, gefunden {other}")),
        }
    }

    // ----- Deklarationen -------------------------------------------------

    fn program(mut self) -> Result<Program> {
        let mut functions = Vec::new();
        while !self.at(&Tok::Eof) {
            functions.push(self.function()?);
        }
        Ok(Program { functions })
    }

    fn function(&mut self) -> Result<Function> {
        let span = self.expect(&Tok::Fn)?;
        let (name, _) = self.ident()?;
        self.expect(&Tok::LParen)?;
        let mut params = Vec::new();
        while !self.at(&Tok::RParen) {
            let mutable = self.eat(&Tok::Mut);
            let (pname, pspan) = self.ident()?;
            self.expect(&Tok::Colon)?;
            let ty = self.ty()?;
            params.push(Param {
                name: pname,
                mutable,
                ty,
                span: pspan,
            });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(&Tok::RParen)?;
        let ret = if self.eat(&Tok::Arrow) {
            self.ty()?
        } else {
            Type::Unit
        };
        let body = self.block()?;
        Ok(Function {
            name,
            params,
            ret,
            body,
            span,
        })
    }

    fn ty(&mut self) -> Result<Type> {
        match self.peek().clone() {
            Tok::Ident(name) => {
                let t = if name == "bool" {
                    Type::Bool
                } else if let Some(it) = IntTy::from_name(&name) {
                    Type::Int(it)
                } else {
                    return self.error(format!("unbekannter Typ `{name}`"));
                };
                self.bump();
                Ok(t)
            }
            Tok::LParen => {
                self.bump();
                self.expect(&Tok::RParen)?;
                Ok(Type::Unit)
            }
            Tok::LBracket => {
                self.bump();
                let elem = self.ty()?;
                self.expect(&Tok::Semi)?;
                let n = self.array_len()?;
                self.expect(&Tok::RBracket)?;
                Ok(Type::Array(Box::new(elem), n))
            }
            other => self.error(format!("Typ erwartet, gefunden {other}")),
        }
    }

    fn array_len(&mut self) -> Result<u64> {
        match self.peek().clone() {
            Tok::Int(v, None | Some(IntTy::Usize)) => {
                let n = u64::try_from(v)
                    .map_err(|_| CompileError::new("Array-Länge ist zu gross", self.span()))?;
                self.bump();
                Ok(n)
            }
            other => self.error(format!(
                "Array-Länge muss ein Ganzzahl-Literal sein, gefunden {other}"
            )),
        }
    }

    // ----- Blöcke und Anweisungen ------------------------------------------

    fn block(&mut self) -> Result<Block> {
        let span = self.expect(&Tok::LBrace)?;
        let mut stmts = Vec::new();
        let mut tail = None;
        loop {
            match self.peek() {
                Tok::RBrace => {
                    self.bump();
                    break;
                }
                Tok::Semi => {
                    self.bump();
                }
                Tok::Let => stmts.push(self.let_stmt()?),
                _ => {
                    if tail.is_some() {
                        return self.error(format!("`;` erwartet, gefunden {}", self.peek()));
                    }
                    let block_like = matches!(
                        self.peek(),
                        Tok::If | Tok::While | Tok::Loop | Tok::For | Tok::LBrace
                    );
                    let expr = if block_like {
                        self.primary()?
                    } else {
                        self.expr()?
                    };
                    if self.eat(&Tok::Semi) {
                        stmts.push(Stmt::Expr { expr, semi: true });
                    } else if self.at(&Tok::RBrace) {
                        tail = Some(Box::new(expr));
                    } else if block_like {
                        stmts.push(Stmt::Expr { expr, semi: false });
                    } else {
                        return self.error(format!("`;` erwartet, gefunden {}", self.peek()));
                    }
                }
            }
        }
        Ok(Block { stmts, tail, span })
    }

    fn let_stmt(&mut self) -> Result<Stmt> {
        let span = self.expect(&Tok::Let)?;
        let mutable = self.eat(&Tok::Mut);
        let (name, _) = self.ident()?;
        let ty = if self.eat(&Tok::Colon) {
            Some(self.ty()?)
        } else {
            None
        };
        if !self.at(&Tok::Eq) {
            return self.error(format!(
                "`=` erwartet (Variablen müssen initialisiert werden), gefunden {}",
                self.peek()
            ));
        }
        self.bump();
        let init = self.expr()?;
        self.expect(&Tok::Semi)?;
        Ok(Stmt::Let {
            name,
            mutable,
            ty,
            init,
            span,
        })
    }

    // ----- Ausdrücke -----------------------------------------------------

    fn expr(&mut self) -> Result<Expr> {
        let lhs = self.binary(0)?;
        let Some(op) = assign_op(self.peek()) else {
            return Ok(lhs);
        };
        let span = self.bump().span;
        let rhs = self.expr()?;
        Ok(Expr::new(
            ExprKind::Assign(op, Box::new(lhs), Box::new(rhs)),
            span,
        ))
    }

    fn binary(&mut self, min_prec: u8) -> Result<Expr> {
        let mut lhs = self.cast()?;
        let mut last_was_cmp = false;
        while let Some((op, prec)) = binop(self.peek()) {
            if prec < min_prec {
                break;
            }
            if op.is_comparison() && last_was_cmp {
                return self.error("Vergleichsoperatoren können nicht verkettet werden");
            }
            last_was_cmp = op.is_comparison();
            let span = self.bump().span;
            let rhs = self.binary(prec + 1)?;
            lhs = Expr::new(ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)), span);
        }
        Ok(lhs)
    }

    fn cast(&mut self) -> Result<Expr> {
        let mut e = self.unary()?;
        while self.at(&Tok::As) {
            let span = self.bump().span;
            let ty = self.ty()?;
            e = Expr::new(ExprKind::Cast(Box::new(e), ty), span);
        }
        Ok(e)
    }

    fn unary(&mut self) -> Result<Expr> {
        let op = match self.peek() {
            Tok::Minus => UnOp::Neg,
            Tok::Bang => UnOp::Not,
            _ => return self.postfix(),
        };
        let span = self.bump().span;
        let operand = self.unary()?;
        Ok(Expr::new(ExprKind::Unary(op, Box::new(operand)), span))
    }

    fn postfix(&mut self) -> Result<Expr> {
        let mut e = self.primary()?;
        loop {
            match self.peek() {
                Tok::LBracket => {
                    let span = self.bump().span;
                    let idx = self.expr()?;
                    self.expect(&Tok::RBracket)?;
                    e = Expr::new(ExprKind::Index(Box::new(e), Box::new(idx)), span);
                }
                Tok::Dot => {
                    let span = self.bump().span;
                    let (method, mspan) = self.ident()?;
                    if method != "len" {
                        return Err(CompileError::new(
                            format!("Methode `{method}` wird nicht unterstützt (nur `.len()`)"),
                            mspan,
                        ));
                    }
                    self.expect(&Tok::LParen)?;
                    self.expect(&Tok::RParen)?;
                    e = Expr::new(ExprKind::Len(Box::new(e)), span);
                }
                _ => return Ok(e),
            }
        }
    }

    fn comma_list(&mut self, close: &Tok) -> Result<Vec<Expr>> {
        let mut items = Vec::new();
        while !self.at(close) {
            items.push(self.expr()?);
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(close)?;
        Ok(items)
    }

    fn primary(&mut self) -> Result<Expr> {
        let span = self.span();
        let kind = match self.peek().clone() {
            Tok::Int(v, suffix) => {
                self.bump();
                ExprKind::Int(v, suffix)
            }
            Tok::True => {
                self.bump();
                ExprKind::Bool(true)
            }
            Tok::False => {
                self.bump();
                ExprKind::Bool(false)
            }
            Tok::Ident(name) => {
                self.bump();
                if self.at(&Tok::Bang) {
                    return self.macro_call(name, span);
                }
                if self.eat(&Tok::LParen) {
                    let args = self.comma_list(&Tok::RParen)?;
                    ExprKind::Call(name, args)
                } else {
                    ExprKind::Var(name)
                }
            }
            Tok::LParen => {
                self.bump();
                if self.at(&Tok::RParen) {
                    return self.error("`()` als Wert wird nicht unterstützt");
                }
                let e = self.expr()?;
                self.expect(&Tok::RParen)?;
                return Ok(e);
            }
            Tok::LBracket => {
                self.bump();
                if self.at(&Tok::RBracket) {
                    return self.error("leere Arrays werden nicht unterstützt");
                }
                let first = self.expr()?;
                if self.eat(&Tok::Semi) {
                    let n = self.array_len()?;
                    self.expect(&Tok::RBracket)?;
                    ExprKind::ArrayRepeat(Box::new(first), n)
                } else {
                    let mut items = vec![first];
                    if self.eat(&Tok::Comma) {
                        items.extend(self.comma_list(&Tok::RBracket)?);
                    } else {
                        self.expect(&Tok::RBracket)?;
                    }
                    ExprKind::ArrayList(items)
                }
            }
            Tok::LBrace => ExprKind::Block(self.block()?),
            Tok::If => return self.if_expr(),
            Tok::While => {
                self.bump();
                let cond = self.expr()?;
                let body = self.block()?;
                ExprKind::While(Box::new(cond), body)
            }
            Tok::Loop => {
                self.bump();
                ExprKind::Loop(self.block()?)
            }
            Tok::For => {
                self.bump();
                let (var, _) = self.ident()?;
                self.expect(&Tok::In)?;
                let start = self.binary(0)?;
                let inclusive = if self.eat(&Tok::DotDotEq) {
                    true
                } else if self.eat(&Tok::DotDot) {
                    false
                } else {
                    return self.error(format!(
                        "`..` oder `..=` erwartet (nur Zahlenbereiche werden unterstützt), gefunden {}",
                        self.peek()
                    ));
                };
                let end = self.binary(0)?;
                let body = self.block()?;
                ExprKind::For {
                    var,
                    start: Box::new(start),
                    end: Box::new(end),
                    inclusive,
                    body,
                }
            }
            Tok::Break => {
                self.bump();
                ExprKind::Break
            }
            Tok::Continue => {
                self.bump();
                ExprKind::Continue
            }
            Tok::Return => {
                self.bump();
                let value = if matches!(
                    self.peek(),
                    Tok::Semi | Tok::RBrace | Tok::RParen | Tok::Comma
                ) {
                    None
                } else {
                    Some(Box::new(self.expr()?))
                };
                ExprKind::Return(value)
            }
            other => return self.error(format!("Ausdruck erwartet, gefunden {other}")),
        };
        Ok(Expr::new(kind, span))
    }

    fn if_expr(&mut self) -> Result<Expr> {
        let span = self.expect(&Tok::If)?;
        let cond = self.expr()?;
        let then = self.block()?;
        let els = if self.eat(&Tok::Else) {
            if self.at(&Tok::If) {
                Some(Box::new(self.if_expr()?))
            } else {
                let bspan = self.span();
                Some(Box::new(Expr::new(ExprKind::Block(self.block()?), bspan)))
            }
        } else {
            None
        };
        Ok(Expr::new(ExprKind::If(Box::new(cond), then, els), span))
    }

    fn macro_call(&mut self, name: String, span: Span) -> Result<Expr> {
        let kind = match name.as_str() {
            "println" => PrintKind::Println,
            "print" => PrintKind::Print,
            "panic" => PrintKind::Panic,
            _ => {
                return Err(CompileError::new(
                    format!("unbekanntes Makro `{name}!` (unterstützt: println!, print!, panic!)"),
                    span,
                ));
            }
        };
        self.expect(&Tok::Bang)?;
        self.expect(&Tok::LParen)?;
        let fmt_span = self.span();
        let raw = match self.peek().clone() {
            Tok::Str(s) => {
                self.bump();
                parse_format(&s, fmt_span)?
            }
            Tok::RParen if kind != PrintKind::Print => Vec::new(),
            other => return self.error(format!("Formatstring erwartet, gefunden {other}")),
        };
        let mut args = Vec::new();
        while self.eat(&Tok::Comma) {
            if self.at(&Tok::RParen) {
                break;
            }
            args.push(self.expr()?);
        }
        self.expect(&Tok::RParen)?;

        // Platzhalter den Argumenten zuordnen; benannte Platzhalter (`{x}`)
        // werden als zusätzliche Argumente angehängt.
        let explicit = args.len();
        let mut used = vec![false; explicit];
        let mut next = 0;
        let mut named: Vec<(String, usize)> = Vec::new();
        let mut pieces = Vec::new();
        for piece in raw {
            let idx = match piece {
                RawPiece::Lit(s) => {
                    pieces.push(FmtPiece::Lit(s));
                    continue;
                }
                RawPiece::Arg(ArgRef::Next) => {
                    next += 1;
                    next - 1
                }
                RawPiece::Arg(ArgRef::Index(i)) => i,
                RawPiece::Arg(ArgRef::Named(n)) => {
                    if let Some(&(_, i)) = named.iter().find(|(m, _)| *m == n) {
                        pieces.push(FmtPiece::Arg(i));
                        continue;
                    }
                    args.push(Expr::new(ExprKind::Var(n.clone()), fmt_span));
                    named.push((n, args.len() - 1));
                    pieces.push(FmtPiece::Arg(args.len() - 1));
                    continue;
                }
            };
            if idx >= explicit {
                return Err(CompileError::new(
                    format!(
                        "Formatstring verlangt mindestens {} Argument(e), es gibt aber nur {explicit}",
                        idx + 1
                    ),
                    fmt_span,
                ));
            }
            used[idx] = true;
            pieces.push(FmtPiece::Arg(idx));
        }
        if let Some(i) = used.iter().position(|u| !u) {
            return Err(CompileError::new(
                "Argument wird im Formatstring nicht verwendet",
                args[i].span,
            ));
        }
        Ok(Expr::new(ExprKind::Print { kind, pieces, args }, span))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tail(src: &str) -> Expr {
        let p = parse(&format!("fn main() {{ {src} }}")).unwrap();
        *p.functions[0].body.tail.clone().unwrap()
    }

    /// Gibt einen Ausdruck vollständig geklammert aus.
    fn show(e: &Expr) -> String {
        match &e.kind {
            ExprKind::Int(v, _) => v.to_string(),
            ExprKind::Var(n) => n.clone(),
            ExprKind::Binary(op, l, r) => format!("({} {} {})", show(l), op.symbol(), show(r)),
            ExprKind::Unary(UnOp::Neg, x) => format!("(-{})", show(x)),
            ExprKind::Unary(UnOp::Not, x) => format!("(!{})", show(x)),
            ExprKind::Cast(x, t) => format!("({} as {t})", show(x)),
            ExprKind::Index(a, i) => format!("{}[{}]", show(a), show(i)),
            other => format!("{other:?}"),
        }
    }

    #[test]
    fn praezedenz_wie_in_rust() {
        assert_eq!(show(&tail("1 + 2 * 3")), "(1 + (2 * 3))");
        assert_eq!(show(&tail("a - b - c")), "((a - b) - c)");
        // In Rust bindet `&` stärker als `==` (anders als in C).
        assert_eq!(show(&tail("n & 1 == 0")), "((n & 1) == 0)");
        assert_eq!(show(&tail("-x as u8")), "((-x) as u8)");
        assert_eq!(show(&tail("a || b && c")), "(a || (b && c))");
        assert_eq!(show(&tail("1 << 2 + 3")), "(1 << (2 + 3))");
        assert_eq!(show(&tail("a[i + 1] * 2")), "(a[(i + 1)] * 2)");
    }

    #[test]
    fn vergleiche_nicht_verkettbar() {
        assert!(parse("fn main() { let x = 1 < 2 < 3; }").is_err());
    }

    #[test]
    fn formatstring_mit_benannten_platzhaltern() {
        let e = tail("println!(\"{} und {x} {{}}\", 1)");
        let ExprKind::Print { pieces, args, .. } = e.kind else {
            panic!("kein println")
        };
        assert_eq!(args.len(), 2);
        assert_eq!(
            pieces,
            vec![
                FmtPiece::Arg(0),
                FmtPiece::Lit(" und ".into()),
                FmtPiece::Arg(1),
                FmtPiece::Lit(" {}".into()),
            ]
        );
        assert!(parse("fn main() { println!(\"{} {}\", 1); }").is_err());
        assert!(parse("fn main() { println!(\"x\", 1); }").is_err());
    }

    #[test]
    fn blockartige_anweisungen_ohne_semikolon() {
        let p = parse("fn main() { if a { } while b { } c }").unwrap();
        assert_eq!(p.functions[0].body.stmts.len(), 2);
        assert!(p.functions[0].body.tail.is_some());
    }
}
