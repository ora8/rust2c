//! Codegenerierung: übersetzt den typgeprüften AST nach C11.
//!
//! Grundidee: Ein Ausdruck wird in einen *reinen* C-Ausdruck (ohne
//! Seiteneffekte) übersetzt. Alles mit Seiteneffekten – Funktionsaufrufe,
//! überlaufgeprüfte Arithmetik, Bereichsprüfungen, `if`/Blöcke als Wert –
//! wird vorher als Anweisung ausgegeben und landet in einer temporären
//! Variablen (`_tN`). So bleibt die Auswertungsreihenfolge von Rust
//! (links nach rechts) erhalten, obwohl sie in C teilweise unspezifiziert ist.
//!
//! Laufzeitfehler verhalten sich wie im Debug-Modus von Rust: Überlauf,
//! Division durch null und Indexfehler beenden das Programm mit Exit-Code 101.

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::ast::*;
use crate::error::Span;
use crate::types::{IntTy, Type};

const PRELUDE: &str = r#"/* Erzeugt von rust2c – nicht von Hand bearbeiten. */
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#if defined(__GNUC__) || defined(__clang__)
#define RS_UNUSED __attribute__((unused))
#else
#define RS_UNUSED
#endif

RS_UNUSED static inline _Noreturn void rs_panic(const char *msg, int line, int col) {
    fflush(stdout);
    fprintf(stderr, "thread 'main' panicked at %d:%d:\n%s\n", line, col, msg);
    exit(101);
}

RS_UNUSED static inline _Noreturn void rs_panic_bounds(uint64_t index, uint64_t len, int line, int col) {
    fflush(stdout);
    fprintf(stderr, "thread 'main' panicked at %d:%d:\nindex out of bounds: the len is %llu but the index is %llu\n",
            line, col, (unsigned long long)len, (unsigned long long)index);
    exit(101);
}
"#;

fn c_type(t: &Type) -> &'static str {
    match t {
        Type::Int(it) => it.c_type(),
        Type::Bool => "bool",
        Type::Unit | Type::Never => "void",
        Type::Array(..) => unreachable!("Arrays haben keinen skalaren C-Typ"),
    }
}

fn int_of(t: &Type) -> IntTy {
    t.as_int()
        .expect("von der Typprüfung sichergestellt: Ganzzahltyp")
}

fn has_value(t: &Type) -> bool {
    !matches!(t, Type::Unit | Type::Never)
}

/// C-Literal für eine nichtnegative Zahl vom Typ `it`.
///
/// Kleine Werte bleiben ein schlichtes `int`-Literal: Die Operanden haben
/// dank Typprüfung immer denselben Typ, und alle Stellen, an denen der
/// genaue C-Typ zählt (Überlaufprüfung, Bitoperationen), casten selbst.
fn int_literal(it: IntTy, v: u128) -> String {
    if v <= i32::MAX as u128 {
        v.to_string()
    } else if it.is_signed() {
        format!("{v}LL")
    } else {
        format!("{v}ULL")
    }
}

/// C-Literal für `-v` (v ≤ |min|).
fn neg_literal(it: IntTy, v: u128) -> String {
    if v == 0 {
        "0".to_string()
    } else if v <= i32::MAX as u128 {
        format!("(-{v})")
    } else if v <= i64::MAX as u128 {
        format!("(({})-{v}LL)", it.c_type())
    } else {
        // -9223372036854775808 ist in C kein gültiges Literal
        format!("(({})(-{}LL - 1))", it.c_type(), v - 1)
    }
}

/// Entfernt ein äusseres Klammerpaar, das den ganzen Ausdruck umschliesst
/// (`(a < b)` → `a < b`), damit `if ((a < b))` nicht entsteht.
fn strip_parens(s: &str) -> &str {
    let Some(inner) = s.strip_prefix('(').and_then(|x| x.strip_suffix(')')) else {
        return s;
    };
    let mut depth = 0i32;
    for c in inner.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return s; // z. B. `(a) + (b)`
                }
            }
            _ => {}
        }
    }
    if depth == 0 { inner } else { s }
}

/// Maskiert Text für einen C-Formatstring von `printf`.
fn escape_format(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '%' => out.push_str("%%"),
            '?' => out.push_str("\\?"), // Trigraphen vermeiden
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                let _ = write!(out, "\\{:03o}", c as u32);
            }
            c => out.push(c),
        }
    }
}

/// Erzeugt das vollständige C-Programm.
pub fn generate(program: &Program) -> String {
    let mut out = String::from(PRELUDE);
    out.push('\n');
    for f in &program.functions {
        let params: Vec<&str> = f.params.iter().map(|p| c_type(&p.ty)).collect();
        let params = if params.is_empty() {
            "void".to_string()
        } else {
            params.join(", ")
        };
        let _ = writeln!(out, "static {} rs_{}({params});", c_type(&f.ret), f.name);
    }
    out.push('\n');
    for f in &program.functions {
        let mut g = FnGen::default();
        g.function(f);
        out.push_str(&g.out);
        out.push('\n');
    }
    out.push_str("int main(void) {\n    rs_main();\n    return 0;\n}\n");
    out
}

/// Wohin der Wert eines Blocks bzw. Ausdrucks geht.
enum Target {
    /// Wert wird nicht gebraucht.
    Discard,
    /// Wert in diese C-Variable schreiben.
    Assign(String),
    /// Wert mit `return` zurückgeben.
    Return,
}

#[derive(Default)]
struct FnGen {
    out: String,
    indent: usize,
    counter: usize,
    /// Rust-Name → C-Name, pro Gültigkeitsbereich.
    scopes: Vec<HashMap<String, String>>,
}

impl FnGen {
    fn line(&mut self, s: &str) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn fresh(&mut self, prefix: &str) -> String {
        self.counter += 1;
        format!("{prefix}{}", self.counter)
    }

    /// Vergibt einen eindeutigen C-Namen. Damit funktioniert Shadowing
    /// (`let x = x + 1;`) und Rust-Namen kollidieren nie mit C-Schlüsselwörtern.
    fn declare(&mut self, name: &str) -> String {
        self.counter += 1;
        let c = format!("v{}_{name}", self.counter);
        self.scopes
            .last_mut()
            .expect("es gibt immer einen Gültigkeitsbereich")
            .insert(name.to_string(), c.clone());
        c
    }

    fn lookup(&self, name: &str) -> String {
        self.scopes
            .iter()
            .rev()
            .find_map(|s| s.get(name).cloned())
            .expect("von der Typprüfung sichergestellt: Variable existiert")
    }

    fn temp(&mut self, ty: &Type, value: &str) -> String {
        let t = self.fresh("_t");
        self.line(&format!("{} {t} = {value};", c_type(ty)));
        t
    }

    fn indented(&mut self, f: impl FnOnce(&mut Self)) {
        self.indent += 1;
        f(self);
        self.indent -= 1;
    }

    /// Führt `f` aus und fängt den dabei erzeugten Code separat ab.
    /// Liefert (abgefangener Code, Ergebnis von `f`).
    fn capture(
        &mut self,
        extra_indent: usize,
        f: impl FnOnce(&mut Self) -> String,
    ) -> (String, String) {
        let saved = std::mem::take(&mut self.out);
        self.indent += extra_indent;
        let r = f(self);
        self.indent -= extra_indent;
        let code = std::mem::replace(&mut self.out, saved);
        (code, r)
    }

    fn panic_call(&self, msg: &str, span: Span) -> String {
        format!("rs_panic(\"{msg}\", {}, {});", span.line, span.col)
    }

    // ----- Funktionen und Blöcke ---------------------------------------------

    fn function(&mut self, f: &Function) {
        self.scopes.push(HashMap::new());
        let params: Vec<String> = f
            .params
            .iter()
            .map(|p| format!("{} {}", c_type(&p.ty), self.declare(&p.name)))
            .collect();
        let params = if params.is_empty() {
            "void".to_string()
        } else {
            params.join(", ")
        };
        self.line(&format!(
            "static {} rs_{}({params}) {{",
            c_type(&f.ret),
            f.name
        ));
        let target = if f.ret == Type::Unit {
            Target::Discard
        } else {
            Target::Return
        };
        self.indented(|g| {
            g.block_body(&f.body, &target);
            if f.ret != Type::Unit && f.body.tail.as_ref().is_none_or(|t| t.ty == Type::Never) {
                // Rumpf endet mit `return`/Endlosschleife; beruhigt den C-Compiler.
                g.line("__builtin_unreachable();");
            }
        });
        self.line("}");
        self.scopes.pop();
    }

    fn block_body(&mut self, b: &Block, target: &Target) {
        self.scopes.push(HashMap::new());
        for stmt in &b.stmts {
            self.stmt(stmt);
        }
        if let Some(tail) = &b.tail {
            self.expr_into(tail, target);
        }
        self.scopes.pop();
    }

    fn stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let { name, ty, init, .. } => {
                let ty = ty.as_ref().expect("von der Typprüfung gesetzt");
                match (ty, &init.kind) {
                    (Type::Array(elem, n), ExprKind::ArrayRepeat(value, _)) => {
                        let v = self.value(value);
                        let c = self.declare(name);
                        let i = self.fresh("_i");
                        self.line(&format!("{} {c}[{n}];", c_type(elem)));
                        self.line(&format!(
                            "for (size_t {i} = 0; {i} < {n}; {i}++) {c}[{i}] = {v};"
                        ));
                    }
                    (Type::Array(elem, n), ExprKind::ArrayList(items)) => {
                        let vals: Vec<String> = items.iter().map(|x| self.value(x)).collect();
                        let c = self.declare(name);
                        self.line(&format!(
                            "{} {c}[{n}] = {{{}}};",
                            c_type(elem),
                            vals.join(", ")
                        ));
                    }
                    _ => {
                        let v = self.value(init);
                        let c = self.declare(name);
                        self.line(&format!("{} {c} = {v};", c_type(ty)));
                    }
                }
            }
            Stmt::Expr { expr, .. } => self.expr_into(expr, &Target::Discard),
        }
    }

    /// Übersetzt `e` und leitet seinen Wert an `target` weiter.
    fn expr_into(&mut self, e: &Expr, target: &Target) {
        match &e.kind {
            ExprKind::If(cond, then, els) => self.if_chain(cond, then, els.as_deref(), target),
            ExprKind::Block(b) => {
                self.line("{");
                self.indented(|g| g.block_body(b, target));
                self.line("}");
            }
            _ => {
                let v = self.expr(e);
                if !has_value(&e.ty) {
                    return;
                }
                match target {
                    // Der verbleibende Ausdruck ist rein – er kann entfallen.
                    Target::Discard => {}
                    Target::Assign(t) => self.line(&format!("{t} = {v};")),
                    Target::Return => self.line(&format!("return {v};")),
                }
            }
        }
    }

    /// `if … else if … else …` – nach Möglichkeit als flache `else if`-Kette.
    fn if_chain(&mut self, cond: &Expr, then: &Block, els: Option<&Expr>, target: &Target) {
        let c = self.value(cond);
        self.line(&format!("if ({}) {{", strip_parens(&c)));
        let mut then = then;
        let mut els = els;
        let mut extra_closes = 0;
        loop {
            self.indented(|g| g.block_body(then, target));
            match els {
                None => {
                    self.line("}");
                    break;
                }
                Some(Expr {
                    kind: ExprKind::If(c2, t2, e2),
                    ..
                }) => {
                    let (pre, cv) = self.capture(1, |g| g.value(c2));
                    if pre.is_empty() {
                        self.line(&format!("}} else if ({}) {{", strip_parens(&cv)));
                    } else {
                        // Bedingung braucht vorbereitende Anweisungen
                        self.line("} else {");
                        self.out.push_str(&pre);
                        self.indent += 1;
                        extra_closes += 1;
                        self.line(&format!("if ({}) {{", strip_parens(&cv)));
                    }
                    then = t2;
                    els = e2.as_deref();
                }
                Some(other) => {
                    self.line("} else {");
                    self.indented(|g| g.expr_into(other, target));
                    self.line("}");
                    break;
                }
            }
        }
        for _ in 0..extra_closes {
            self.indent -= 1;
            self.line("}");
        }
    }

    // ----- Ausdrücke -----------------------------------------------------

    /// Wie [`Self::expr`], liefert aber für `!`-Ausdrücke einen Platzhalter,
    /// damit der erzeugte (unerreichbare) C-Code gültig bleibt.
    fn value(&mut self, e: &Expr) -> String {
        let v = self.expr(e);
        if e.ty == Type::Never {
            "0".to_string()
        } else {
            v
        }
    }

    /// Übersetzt `e`; Seiteneffekte werden als Anweisungen ausgegeben.
    /// Liefert einen reinen C-Ausdruck (leer bei `()` und `!`).
    fn expr(&mut self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::Int(v, _) => int_literal(int_of(&e.ty), *v),
            ExprKind::Bool(b) => b.to_string(),
            ExprKind::Var(name) => self.lookup(name),
            ExprKind::Unary(UnOp::Neg, inner) => {
                let it = int_of(&e.ty);
                if let ExprKind::Int(v, _) = inner.kind {
                    return neg_literal(it, v);
                }
                let a = self.value(inner);
                let t = self.fresh("_t");
                self.line(&format!("{} {t};", it.c_type()));
                let p = self.panic_call("attempt to negate with overflow", e.span);
                self.line(&format!(
                    "if (__builtin_sub_overflow(({})0, {a}, &{t})) {p}",
                    it.c_type()
                ));
                t
            }
            ExprKind::Unary(UnOp::Not, inner) => {
                let a = self.value(inner);
                match &e.ty {
                    Type::Bool => format!("(!{a})"),
                    t => format!("(({})~{a})", c_type(t)),
                }
            }
            ExprKind::Binary(op, l, r) => self.binary(*op, l, r, e),
            ExprKind::Assign(op, place, value) => {
                // Rust wertet bei Zuweisungen zuerst die rechte Seite aus.
                let v = self.value(value);
                let p = self.place(place);
                match op {
                    None => self.line(&format!("{p} = {v};")),
                    Some(o) => {
                        let nv = self.arith(*o, &p, &v, &place.ty, e.span);
                        self.line(&format!("{p} = {nv};"));
                    }
                }
                String::new()
            }
            ExprKind::Call(name, args) => {
                let vals: Vec<String> = args.iter().map(|a| self.value(a)).collect();
                let call = format!("rs_{name}({})", vals.join(", "));
                if has_value(&e.ty) {
                    self.temp(&e.ty, &call)
                } else {
                    self.line(&format!("{call};"));
                    String::new()
                }
            }
            ExprKind::Index(base, idx) => {
                let p = self.index_place(base, idx, e.span);
                self.temp(&e.ty, &p)
            }
            ExprKind::Len(base) => match &base.ty {
                Type::Array(_, n) => int_literal(IntTy::Usize, u128::from(*n)),
                _ => unreachable!("von der Typprüfung sichergestellt: Array"),
            },
            ExprKind::Cast(inner, target) => {
                let a = self.value(inner);
                format!("(({})({a}))", c_type(target))
            }
            ExprKind::ArrayRepeat(..) | ExprKind::ArrayList(..) => {
                unreachable!("Array-Literale kommen nur in `let` vor")
            }
            ExprKind::If(cond, then, els) => {
                if has_value(&e.ty) {
                    let t = self.fresh("_t");
                    self.line(&format!("{} {t};", c_type(&e.ty)));
                    self.if_chain(cond, then, els.as_deref(), &Target::Assign(t.clone()));
                    t
                } else {
                    self.if_chain(cond, then, els.as_deref(), &Target::Discard);
                    String::new()
                }
            }
            ExprKind::Block(b) => {
                if has_value(&e.ty) {
                    let t = self.fresh("_t");
                    self.line(&format!("{} {t};", c_type(&e.ty)));
                    let target = Target::Assign(t.clone());
                    self.line("{");
                    self.indented(|g| g.block_body(b, &target));
                    self.line("}");
                    t
                } else {
                    self.line("{");
                    self.indented(|g| g.block_body(b, &Target::Discard));
                    self.line("}");
                    String::new()
                }
            }
            ExprKind::While(cond, body) => {
                let (pre, c) = self.capture(1, |g| g.value(cond));
                if pre.is_empty() {
                    self.line(&format!("while ({}) {{", strip_parens(&c)));
                } else {
                    self.line("for (;;) {");
                    self.out.push_str(&pre);
                    self.indented(|g| g.line(&format!("if (!({c})) break;")));
                }
                self.indented(|g| g.block_body(body, &Target::Discard));
                self.line("}");
                String::new()
            }
            ExprKind::Loop(body) => {
                self.line("for (;;) {");
                self.indented(|g| g.block_body(body, &Target::Discard));
                self.line("}");
                String::new()
            }
            ExprKind::For {
                var,
                start,
                end,
                inclusive,
                body,
            } => {
                self.for_range(var, start, end, *inclusive, body);
                String::new()
            }
            ExprKind::Break => {
                self.line("break;");
                String::new()
            }
            ExprKind::Continue => {
                self.line("continue;");
                String::new()
            }
            ExprKind::Return(value) => {
                match value {
                    Some(v) if has_value(&v.ty) => {
                        let r = self.value(v);
                        self.line(&format!("return {r};"));
                    }
                    Some(v) => {
                        self.expr(v);
                        self.line("return;");
                    }
                    None => self.line("return;"),
                }
                String::new()
            }
            ExprKind::Print { kind, pieces, args } => {
                self.print(*kind, pieces, args, e.span);
                String::new()
            }
        }
    }

    fn binary(&mut self, op: BinOp, l: &Expr, r: &Expr, e: &Expr) -> String {
        let a = self.value(l);
        if matches!(op, BinOp::And | BinOp::Or) {
            // Kurzschlussauswertung: Code der rechten Seite darf nur bedingt laufen.
            let (pre, b) = self.capture(1, |g| g.value(r));
            let sym = op.symbol();
            if pre.is_empty() {
                return format!("({a} {sym} {b})");
            }
            let t = self.temp(&Type::Bool, &a);
            let neg = if op == BinOp::And { "" } else { "!" };
            self.line(&format!("if ({neg}{t}) {{"));
            self.out.push_str(&pre);
            self.indented(|g| g.line(&format!("{t} = {b};")));
            self.line("}");
            return t;
        }
        let b = self.value(r);
        if op.is_comparison() {
            return format!("({a} {} {b})", op.symbol());
        }
        self.arith(op, &a, &b, &e.ty, e.span)
    }

    /// Arithmetik-, Bit- und Schiebeoperationen mit den Prüfungen von Rust.
    fn arith(&mut self, op: BinOp, a: &str, b: &str, ty: &Type, span: Span) -> String {
        let ct = c_type(ty);
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul => {
                let (builtin, what) = match op {
                    BinOp::Add => ("add", "add"),
                    BinOp::Sub => ("sub", "subtract"),
                    _ => ("mul", "multiply"),
                };
                let t = self.fresh("_t");
                self.line(&format!("{ct} {t};"));
                let p = self.panic_call(&format!("attempt to {what} with overflow"), span);
                self.line(&format!(
                    "if (__builtin_{builtin}_overflow({a}, {b}, &{t})) {p}"
                ));
                t
            }
            BinOp::Div | BinOp::Rem => {
                let it = int_of(ty);
                let (zero, overflow) = if op == BinOp::Div {
                    (
                        "attempt to divide by zero",
                        "attempt to divide with overflow",
                    )
                } else {
                    (
                        "attempt to calculate the remainder with a divisor of zero",
                        "attempt to calculate the remainder with overflow",
                    )
                };
                let p = self.panic_call(zero, span);
                self.line(&format!("if ({b} == 0) {p}"));
                if it.is_signed() {
                    let p = self.panic_call(overflow, span);
                    self.line(&format!("if ({a} == {} && {b} == -1) {p}", it.c_min()));
                }
                let t = self.fresh("_t");
                self.line(&format!("{ct} {t} = {a} {} {b};", op.symbol()));
                t
            }
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => {
                format!("(({ct})({a} {} {b}))", op.symbol())
            }
            BinOp::Shl | BinOp::Shr => {
                let it = int_of(ty);
                let what = if op == BinOp::Shl { "left" } else { "right" };
                let p = self.panic_call(&format!("attempt to shift {what} with overflow"), span);
                self.line(&format!("if ((uint64_t)({b}) >= {}) {p}", it.bits()));
                if op == BinOp::Shl {
                    // Im vorzeichenlosen Typ schieben: kein undefiniertes Verhalten in C.
                    format!("(({ct})(({}){a} << {b}))", it.c_unsigned())
                } else {
                    format!("(({ct})({a} >> {b}))")
                }
            }
            _ => unreachable!("kein arithmetischer Operator: {op:?}"),
        }
    }

    /// Zuweisungsziel als C-Lvalue.
    fn place(&mut self, p: &Expr) -> String {
        match &p.kind {
            ExprKind::Var(name) => self.lookup(name),
            ExprKind::Index(base, idx) => self.index_place(base, idx, p.span),
            _ => unreachable!("von der Typprüfung sichergestellt: gültiges Ziel"),
        }
    }

    fn index_place(&mut self, base: &Expr, idx: &Expr, span: Span) -> String {
        let ExprKind::Var(name) = &base.kind else {
            unreachable!("von der Typprüfung sichergestellt: Array-Variable")
        };
        let Type::Array(_, n) = &base.ty else {
            unreachable!("von der Typprüfung sichergestellt: Array")
        };
        let arr = self.lookup(name);
        let i = self.value(idx);
        if matches!(idx.kind, ExprKind::Int(..)) {
            // Konstanter Index – schon von der Typprüfung kontrolliert.
            return format!("{arr}[{i}]");
        }
        self.line(&format!(
            "if ({i} >= {n}ULL) rs_panic_bounds({i}, {n}ULL, {}, {});",
            span.line, span.col
        ));
        format!("{arr}[{i}]")
    }

    fn for_range(&mut self, var: &str, start: &Expr, end: &Expr, inclusive: bool, body: &Block) {
        let ct = c_type(&start.ty);
        let s = self.value(start);
        let e = self.value(end);
        self.line("{");
        self.indent += 1;
        let st = self.fresh("_s");
        let en = self.fresh("_e");
        let c = self.fresh("_c");
        self.line(&format!("{ct} {st} = {s};"));
        self.line(&format!("{ct} {en} = {e};"));
        if inclusive {
            // Ohne Überlauf bis einschliesslich zum Maximalwert des Typs zählen.
            let d = self.fresh("_d");
            self.line(&format!("bool {d} = {st} > {en};"));
            self.line(&format!(
                "for ({ct} {c} = {st}; !{d}; {d} = ({c} == {en}), {c} = {d} ? {c} : ({ct})({c} + 1)) {{"
            ));
        } else {
            self.line(&format!("for ({ct} {c} = {st}; {c} < {en}; {c}++) {{"));
        }
        self.indented(|g| {
            g.scopes.push(HashMap::new());
            let v = g.declare(var);
            g.line(&format!("{ct} {v} = {c};"));
            g.block_body(body, &Target::Discard);
            g.scopes.pop();
        });
        self.line("}");
        self.indent -= 1;
        self.line("}");
    }

    fn print(&mut self, kind: PrintKind, pieces: &[FmtPiece], args: &[Expr], span: Span) {
        let vals: Vec<String> = args.iter().map(|a| self.value(a)).collect();
        let mut fmt = String::new();
        let mut cargs = Vec::new();
        for piece in pieces {
            match piece {
                FmtPiece::Lit(s) => escape_format(s, &mut fmt),
                FmtPiece::Arg(i) => {
                    let v = &vals[*i];
                    match &args[*i].ty {
                        Type::Bool => {
                            fmt.push_str("%s");
                            cargs.push(format!("({v} ? \"true\" : \"false\")"));
                        }
                        Type::Int(it) if it.is_signed() => {
                            fmt.push_str("%lld");
                            cargs.push(format!("(long long){v}"));
                        }
                        _ => {
                            fmt.push_str("%llu");
                            cargs.push(format!("(unsigned long long){v}"));
                        }
                    }
                }
            }
        }
        let cargs: String = cargs.iter().map(|a| format!(", {a}")).collect();
        match kind {
            PrintKind::Print => self.line(&format!("printf(\"{fmt}\"{cargs});")),
            PrintKind::Println => self.line(&format!("printf(\"{fmt}\\n\"{cargs});")),
            PrintKind::Panic => {
                self.line("fflush(stdout);");
                self.line(&format!(
                    "fprintf(stderr, \"thread 'main' panicked at {}:{}:\\n{fmt}\\n\"{cargs});",
                    span.line, span.col
                ));
                self.line("exit(101);");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literale() {
        assert_eq!(int_literal(IntTy::I32, 5), "5");
        assert_eq!(int_literal(IntTy::U8, 255), "255");
        assert_eq!(int_literal(IntTy::U64, 1 << 40), "1099511627776ULL");
        assert_eq!(
            neg_literal(IntTy::I32, 2147483648),
            "((int32_t)-2147483648LL)"
        );
        assert_eq!(
            neg_literal(IntTy::I64, 9223372036854775808),
            "((int64_t)(-9223372036854775807LL - 1))"
        );
    }

    #[test]
    fn klammern_entfernen() {
        assert_eq!(strip_parens("(a < b)"), "a < b");
        assert_eq!(strip_parens("(a) + (b)"), "(a) + (b)");
        assert_eq!(strip_parens("((x))"), "(x)");
        assert_eq!(strip_parens("v1_x"), "v1_x");
    }

    #[test]
    fn formatstring_maskierung() {
        let mut s = String::new();
        escape_format("100% \"ok\"??\n", &mut s);
        assert_eq!(s, "100%% \\\"ok\\\"\\?\\?\\n");
    }
}
