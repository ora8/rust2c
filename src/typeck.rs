//! Typprüfung und Namensauflösung.
//!
//! Prüft das Programm nach den Regeln von Rust (soweit die Teilmenge reicht)
//! und trägt dabei den Typ jedes Ausdrucks in [`Expr::ty`] ein.
//!
//! Typinferenz ist bewusst einfach gehalten: Ganzzahl-Literale ohne Suffix
//! übernehmen den *erwarteten* Typ aus dem Kontext (Typangabe, Parameter,
//! anderer Operand) und sind sonst `i32` – wie in Rust.

use std::collections::HashMap;

use crate::ast::*;
use crate::error::{CompileError, Result, Span};
use crate::types::{IntTy, Type};

#[derive(Debug, Clone)]
pub struct FnSig {
    pub params: Vec<Type>,
    pub ret: Type,
}

#[derive(Debug, Clone)]
struct VarInfo {
    ty: Type,
    mutable: bool,
}

fn err<T>(msg: impl Into<String>, span: Span) -> Result<T> {
    Err(CompileError::new(msg, span))
}

/// Prüft das ganze Programm.
pub fn check(program: &mut Program) -> Result<()> {
    let mut fns: HashMap<String, FnSig> = HashMap::new();
    for f in &program.functions {
        if fns.contains_key(&f.name) {
            return err(
                format!("Funktion `{}` ist mehrfach definiert", f.name),
                f.span,
            );
        }
        for (i, p) in f.params.iter().enumerate() {
            if !p.ty.is_scalar() {
                return err(
                    format!("Parametertyp `{}` wird nicht unterstützt", p.ty),
                    p.span,
                );
            }
            if f.params[..i].iter().any(|q| q.name == p.name) {
                return err(format!("Parameter `{}` ist doppelt", p.name), p.span);
            }
        }
        if !(f.ret.is_scalar() || f.ret == Type::Unit) {
            return err(
                format!("Rückgabetyp `{}` wird nicht unterstützt", f.ret),
                f.span,
            );
        }
        fns.insert(
            f.name.clone(),
            FnSig {
                params: f.params.iter().map(|p| p.ty.clone()).collect(),
                ret: f.ret.clone(),
            },
        );
    }
    match program.functions.iter().find(|f| f.name == "main") {
        None => return err("keine Funktion `main` gefunden", Span::new(1, 1)),
        Some(m) if !m.params.is_empty() || m.ret != Type::Unit => {
            return err(
                "`main` darf weder Parameter noch Rückgabewert haben",
                m.span,
            );
        }
        Some(_) => {}
    }

    let mut checker = Checker {
        fns,
        scopes: Vec::new(),
        ret: Type::Unit,
        loops: Vec::new(),
    };
    for f in &mut program.functions {
        checker.function(f)?;
    }
    Ok(())
}

struct Checker {
    fns: HashMap<String, FnSig>,
    scopes: Vec<HashMap<String, VarInfo>>,
    /// Rückgabetyp der aktuellen Funktion.
    ret: Type,
    /// Pro umschliessender Schleife: kommt darin ein `break` vor?
    loops: Vec<bool>,
}

/// Ist `e` ein Ganzzahlausdruck, dessen Typ allein vom Kontext abhängt
/// (z. B. `3`, `-1`, `2 * 8`)?
fn is_untyped_int(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Int(_, None) => true,
        ExprKind::Unary(_, x) => is_untyped_int(x),
        ExprKind::Binary(op, l, r) => {
            !op.is_comparison()
                && !matches!(op, BinOp::And | BinOp::Or)
                && is_untyped_int(l)
                && (matches!(op, BinOp::Shl | BinOp::Shr) || is_untyped_int(r))
        }
        _ => false,
    }
}

/// Ergebnistyp eines binären Operators (auch für `op=`).
fn binop_type(op: BinOp, lt: &Type, rt: &Type, span: Span) -> Result<Type> {
    let ok = match op {
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
            matches!(lt, Type::Int(_)) && lt == rt
        }
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => lt.is_scalar() && lt == rt,
        BinOp::Shl | BinOp::Shr => matches!(lt, Type::Int(_)) && matches!(rt, Type::Int(_)),
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            lt.is_scalar() && lt == rt
        }
        BinOp::And | BinOp::Or => *lt == Type::Bool && *rt == Type::Bool,
    };
    if !ok {
        return err(
            format!(
                "Operator `{}` ist nicht anwendbar auf `{lt}` und `{rt}`",
                op.symbol()
            ),
            span,
        );
    }
    Ok(if op.is_comparison() {
        Type::Bool
    } else {
        lt.clone()
    })
}

impl Checker {
    fn lookup(&self, name: &str, span: Span) -> Result<VarInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.get(name) {
                return Ok(v.clone());
            }
        }
        err(format!("unbekannte Variable `{name}`"), span)
    }

    fn declare(&mut self, name: &str, ty: Type, mutable: bool) {
        self.scopes
            .last_mut()
            .expect("es gibt immer einen Gültigkeitsbereich")
            .insert(name.to_string(), VarInfo { ty, mutable });
    }

    fn function(&mut self, f: &mut Function) -> Result<()> {
        self.ret = f.ret.clone();
        self.scopes = vec![HashMap::new()];
        for p in &f.params {
            self.declare(&p.name, p.ty.clone(), p.mutable);
        }
        let ret = f.ret.clone();
        let body_ty = self.block(&mut f.body, Some(&ret))?;
        if !body_ty.coerces_to(&ret) {
            let span = f.body.tail.as_ref().map_or(f.body.span, |t| t.span);
            return err(
                format!(
                    "Funktion `{}` muss `{ret}` zurückgeben, der Rumpf liefert aber `{body_ty}`",
                    f.name
                ),
                span,
            );
        }
        Ok(())
    }

    fn block(&mut self, b: &mut Block, expected: Option<&Type>) -> Result<Type> {
        self.scopes.push(HashMap::new());
        let mut diverges = false;
        for stmt in &mut b.stmts {
            match stmt {
                Stmt::Let {
                    name,
                    mutable,
                    ty,
                    init,
                    span,
                } => {
                    let t = self.let_init(ty.clone(), init)?;
                    if matches!(t, Type::Unit | Type::Never) {
                        return err(
                            format!("Variablen vom Typ `{t}` werden nicht unterstützt"),
                            *span,
                        );
                    }
                    *ty = Some(t.clone());
                    self.declare(name, t, *mutable);
                }
                Stmt::Expr { expr, semi } => {
                    let t = self.expr(expr, None)?;
                    if !*semi && !t.coerces_to(&Type::Unit) {
                        return err(
                            format!("erwartet `()`, gefunden `{t}` (fehlt ein `;`?)"),
                            expr.span,
                        );
                    }
                    if t == Type::Never {
                        diverges = true;
                    }
                }
            }
        }
        let t = match &mut b.tail {
            Some(e) => self.expr(e, expected)?,
            None if diverges => Type::Never,
            None => Type::Unit,
        };
        self.scopes.pop();
        Ok(t)
    }

    fn let_init(&mut self, ty: Option<Type>, init: &mut Expr) -> Result<Type> {
        match ty {
            Some(Type::Array(elem, n)) => self.array_init(init, Some((&elem, n))),
            Some(t) => {
                let it = self.expr(init, Some(&t))?;
                if !it.coerces_to(&t) {
                    return err(format!("erwartet `{t}`, gefunden `{it}`"), init.span);
                }
                Ok(t)
            }
            None if matches!(
                init.kind,
                ExprKind::ArrayRepeat(..) | ExprKind::ArrayList(..)
            ) =>
            {
                self.array_init(init, None)
            }
            None => self.expr(init, None),
        }
    }

    fn array_init(&mut self, init: &mut Expr, expected: Option<(&Type, u64)>) -> Result<Type> {
        let span = init.span;
        let t = match &mut init.kind {
            ExprKind::ArrayRepeat(value, n) => {
                let et = self.expr(value, expected.map(|(e, _)| e))?;
                if !et.is_scalar() {
                    return err(
                        "Array-Elemente müssen Ganzzahlen oder `bool` sein",
                        value.span,
                    );
                }
                if *n == 0 {
                    return err("leere Arrays werden nicht unterstützt", span);
                }
                Type::Array(Box::new(et), *n)
            }
            ExprKind::ArrayList(items) => {
                let mut et: Option<Type> = expected.map(|(e, _)| e.clone());
                for item in items.iter_mut() {
                    let t = self.expr(item, et.as_ref())?;
                    match &et {
                        Some(x) if *x != t => {
                            return err(format!("erwartet `{x}`, gefunden `{t}`"), item.span);
                        }
                        Some(_) => {}
                        None => {
                            if !t.is_scalar() {
                                return err(
                                    "Array-Elemente müssen Ganzzahlen oder `bool` sein",
                                    item.span,
                                );
                            }
                            et = Some(t);
                        }
                    }
                }
                let et = et.expect("Array-Literale sind nie leer");
                Type::Array(Box::new(et), items.len() as u64)
            }
            _ => {
                return err(
                    "Array-Literal (`[wert; n]` oder `[a, b, ...]`) erwartet",
                    span,
                );
            }
        };
        if let Some((e, n)) = expected {
            let want = Type::Array(Box::new(e.clone()), n);
            if t != want {
                return err(format!("erwartet `{want}`, gefunden `{t}`"), span);
            }
        }
        init.ty = t.clone();
        Ok(t)
    }

    /// Prüft eine Array-Variable (für Indizierung und `.len()`).
    fn array_var(&mut self, base: &mut Expr, need_mut: bool) -> Result<Type> {
        let ExprKind::Var(name) = &base.kind else {
            return err("nur Array-Variablen können indiziert werden", base.span);
        };
        let v = self.lookup(name, base.span)?;
        let Type::Array(elem, _) = &v.ty else {
            return err(
                format!("`{name}` ist kein Array, sondern `{}`", v.ty),
                base.span,
            );
        };
        if need_mut && !v.mutable {
            return err(
                format!(
                    "Array `{name}` ist nicht veränderlich; deklariere es mit `let mut {name}`"
                ),
                base.span,
            );
        }
        let elem = (**elem).clone();
        base.ty = v.ty;
        Ok(elem)
    }

    fn index(&mut self, base: &mut Expr, idx: &mut Expr, need_mut: bool) -> Result<Type> {
        let elem = self.array_var(base, need_mut)?;
        if let (ExprKind::Int(v, _), Type::Array(_, n)) = (&idx.kind, &base.ty)
            && *v >= u128::from(*n)
        {
            return err(
                format!("Index {v} liegt ausserhalb des Arrays der Länge {n}"),
                idx.span,
            );
        }
        let usize_ty = Type::Int(IntTy::Usize);
        let it = self.expr(idx, Some(&usize_ty))?;
        if !it.coerces_to(&usize_ty) {
            return err(
                format!("Index muss vom Typ `usize` sein, gefunden `{it}`"),
                idx.span,
            );
        }
        Ok(elem)
    }

    /// Prüft das Ziel einer Zuweisung.
    fn place(&mut self, p: &mut Expr) -> Result<Type> {
        let span = p.span;
        let t = match &mut p.kind {
            ExprKind::Var(name) => {
                let v = self.lookup(name, span)?;
                if matches!(v.ty, Type::Array(..)) {
                    return err("Arrays können nicht als Ganzes zugewiesen werden", span);
                }
                if !v.mutable {
                    return err(
                        format!(
                            "`{name}` ist nicht veränderlich; deklariere die Variable mit `let mut {name}`"
                        ),
                        span,
                    );
                }
                v.ty
            }
            ExprKind::Index(base, idx) => self.index(base, idx, true)?,
            _ => return err("ungültiges Ziel für eine Zuweisung", span),
        };
        p.ty = t.clone();
        Ok(t)
    }

    /// Prüft zwei Operanden gleichen Typs; ein typloses Literal links
    /// übernimmt den Typ des rechten Operanden.
    fn operands(
        &mut self,
        l: &mut Expr,
        r: &mut Expr,
        expected: Option<&Type>,
    ) -> Result<(Type, Type)> {
        if is_untyped_int(l) && !is_untyped_int(r) {
            let rt = self.expr(r, expected)?;
            let lt = self.expr(l, Some(&rt))?;
            Ok((lt, rt))
        } else {
            let lt = self.expr(l, expected)?;
            let rt = self.expr(r, Some(&lt))?;
            Ok((lt, rt))
        }
    }

    fn expect_type(&mut self, e: &mut Expr, want: &Type, what: &str) -> Result<()> {
        let t = self.expr(e, Some(want))?;
        if !t.coerces_to(want) {
            return err(format!("{what}: erwartet `{want}`, gefunden `{t}`"), e.span);
        }
        Ok(())
    }

    fn expr(&mut self, e: &mut Expr, expected: Option<&Type>) -> Result<Type> {
        let span = e.span;
        let t = match &mut e.kind {
            ExprKind::Int(v, suffix) => {
                let it = suffix
                    .or_else(|| expected.and_then(Type::as_int))
                    .unwrap_or(IntTy::I32);
                if *v > it.max_value() {
                    return err(
                        format!("Literal `{v}` liegt ausserhalb des Wertebereichs von `{it}`"),
                        span,
                    );
                }
                Type::Int(it)
            }
            ExprKind::Bool(_) => Type::Bool,
            ExprKind::Var(name) => {
                let v = self.lookup(name, span)?;
                if matches!(v.ty, Type::Array(..)) {
                    return err(
                        format!(
                            "das Array `{name}` kann nur indiziert oder mit `.len()` verwendet werden"
                        ),
                        span,
                    );
                }
                v.ty
            }
            ExprKind::Unary(UnOp::Neg, inner) => {
                if let ExprKind::Int(v, suffix) = inner.kind {
                    let it = suffix
                        .or_else(|| expected.and_then(Type::as_int))
                        .unwrap_or(IntTy::I32);
                    if !it.is_signed() {
                        return err(
                            format!("`-` ist auf den vorzeichenlosen Typ `{it}` nicht anwendbar"),
                            span,
                        );
                    }
                    if v > it.min_magnitude() {
                        return err(
                            format!("Literal `-{v}` liegt ausserhalb des Wertebereichs von `{it}`"),
                            span,
                        );
                    }
                    inner.ty = Type::Int(it);
                    Type::Int(it)
                } else {
                    let t = self.expr(inner, expected)?;
                    match t {
                        Type::Int(it) if it.is_signed() => t,
                        _ => return err(format!("`-` ist auf `{t}` nicht anwendbar"), span),
                    }
                }
            }
            ExprKind::Unary(UnOp::Not, inner) => {
                let t = self.expr(inner, expected)?;
                if !t.is_scalar() {
                    return err(format!("`!` ist auf `{t}` nicht anwendbar"), span);
                }
                t
            }
            ExprKind::Binary(op, l, r) => {
                let op = *op;
                let (lt, rt) = match op {
                    BinOp::Shl | BinOp::Shr => (self.expr(l, expected)?, self.expr(r, None)?),
                    BinOp::And | BinOp::Or => (
                        self.expr(l, Some(&Type::Bool))?,
                        self.expr(r, Some(&Type::Bool))?,
                    ),
                    _ if op.is_comparison() => self.operands(l, r, None)?,
                    _ => self.operands(l, r, expected)?,
                };
                binop_type(op, &lt, &rt, span)?
            }
            ExprKind::Assign(op, place, value) => {
                let pt = self.place(place)?;
                match *op {
                    None => {
                        let vt = self.expr(value, Some(&pt))?;
                        if !vt.coerces_to(&pt) {
                            return err(
                                format!("`{vt}` kann nicht an `{pt}` zugewiesen werden"),
                                value.span,
                            );
                        }
                    }
                    Some(o) => {
                        let vt = if matches!(o, BinOp::Shl | BinOp::Shr) {
                            self.expr(value, None)?
                        } else {
                            self.expr(value, Some(&pt))?
                        };
                        binop_type(o, &pt, &vt, span)?;
                    }
                }
                Type::Unit
            }
            ExprKind::Call(name, args) => {
                let Some(sig) = self.fns.get(name.as_str()).cloned() else {
                    return err(format!("unbekannte Funktion `{name}`"), span);
                };
                if args.len() != sig.params.len() {
                    return err(
                        format!(
                            "`{name}` erwartet {} Argument(e), übergeben wurden {}",
                            sig.params.len(),
                            args.len()
                        ),
                        span,
                    );
                }
                for (i, (arg, pt)) in args.iter_mut().zip(&sig.params).enumerate() {
                    self.expect_type(arg, pt, &format!("Argument {} von `{name}`", i + 1))?;
                }
                sig.ret
            }
            ExprKind::Index(base, idx) => self.index(base, idx, false)?,
            ExprKind::Len(base) => {
                self.array_var(base, false)?;
                Type::Int(IntTy::Usize)
            }
            ExprKind::Cast(inner, target) => {
                let target = target.clone();
                if !matches!(target, Type::Int(_)) {
                    return err(
                        format!("Umwandlung nach `{target}` wird nicht unterstützt"),
                        span,
                    );
                }
                // Wie in Rust: ein typloses Literal erhält den Zieltyp (`-1 as u32` ist ein Fehler).
                let hint = is_untyped_int(inner).then_some(&target);
                let t = self.expr(inner, hint)?;
                if !t.is_scalar() {
                    return err(
                        format!("`{t}` kann nicht nach `{target}` umgewandelt werden"),
                        span,
                    );
                }
                target
            }
            ExprKind::ArrayRepeat(..) | ExprKind::ArrayList(..) => {
                return err(
                    "Array-Literale sind nur zur Initialisierung mit `let` erlaubt",
                    span,
                );
            }
            ExprKind::If(cond, then, els) => {
                self.expect_type(cond, &Type::Bool, "Bedingung")?;
                match els {
                    None => {
                        let tt = self.block(then, Some(&Type::Unit))?;
                        if !tt.coerces_to(&Type::Unit) {
                            return err(
                                format!(
                                    "`if` ohne `else` darf keinen Wert liefern (gefunden `{tt}`)"
                                ),
                                then.tail.as_ref().map_or(span, |t| t.span),
                            );
                        }
                        Type::Unit
                    }
                    Some(els) => {
                        let tt = self.block(then, expected)?;
                        let else_expected = if tt == Type::Never {
                            expected.cloned()
                        } else {
                            Some(tt.clone())
                        };
                        let et = self.expr(els, else_expected.as_ref())?;
                        if tt == Type::Never {
                            et
                        } else if et == Type::Never || et == tt {
                            tt
                        } else {
                            return err(
                                format!(
                                    "`if`- und `else`-Zweig haben verschiedene Typen: `{tt}` und `{et}`"
                                ),
                                els.span,
                            );
                        }
                    }
                }
            }
            ExprKind::While(cond, body) => {
                self.expect_type(cond, &Type::Bool, "Bedingung")?;
                self.loop_body(body)?;
                Type::Unit
            }
            ExprKind::Loop(body) => {
                if self.loop_body(body)? {
                    Type::Unit
                } else {
                    Type::Never
                }
            }
            ExprKind::For {
                var,
                start,
                end,
                body,
                ..
            } => {
                let (st, et) = self.operands(start, end, None)?;
                if !matches!(st, Type::Int(_)) || st != et {
                    return err(
                        format!(
                            "Bereichsgrenzen müssen denselben Ganzzahltyp haben (`{st}` und `{et}`)"
                        ),
                        span,
                    );
                }
                self.scopes.push(HashMap::new());
                self.declare(var, st, false);
                let r = self.loop_body(body);
                self.scopes.pop();
                r?;
                Type::Unit
            }
            ExprKind::Block(b) => self.block(b, expected)?,
            ExprKind::Break => {
                let Some(has_break) = self.loops.last_mut() else {
                    return err("`break` ausserhalb einer Schleife", span);
                };
                *has_break = true;
                Type::Never
            }
            ExprKind::Continue => {
                if self.loops.is_empty() {
                    return err("`continue` ausserhalb einer Schleife", span);
                }
                Type::Never
            }
            ExprKind::Return(value) => {
                let ret = self.ret.clone();
                match value {
                    Some(v) => self.expect_type(v, &ret, "Rückgabewert")?,
                    None if ret != Type::Unit => {
                        return err(format!("`return` ohne Wert, erwartet wird `{ret}`"), span);
                    }
                    None => {}
                }
                Type::Never
            }
            ExprKind::Print { kind, args, .. } => {
                let kind = *kind;
                for arg in args.iter_mut() {
                    let t = self.expr(arg, None)?;
                    if !t.is_scalar() {
                        return err(format!("`{t}` kann nicht ausgegeben werden"), arg.span);
                    }
                }
                if kind == PrintKind::Panic {
                    Type::Never
                } else {
                    Type::Unit
                }
            }
        };
        e.ty = t.clone();
        Ok(t)
    }

    /// Prüft einen Schleifenrumpf; liefert, ob darin `break` vorkommt.
    fn loop_body(&mut self, body: &mut Block) -> Result<bool> {
        self.loops.push(false);
        let r = self.block(body, Some(&Type::Unit));
        let has_break = self.loops.pop().unwrap_or(false);
        let t = r?;
        if !t.coerces_to(&Type::Unit) {
            return err(
                format!("ein Schleifenrumpf darf keinen Wert liefern (gefunden `{t}`)"),
                body.tail.as_ref().map_or(body.span, |t| t.span),
            );
        }
        Ok(has_break)
    }
}
