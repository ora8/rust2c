//! Fehlermeldungen des Compilers.

fn fehler(src: &str) -> String {
    match rust2c::compile(src) {
        Ok(_) => panic!("hätte fehlschlagen sollen:\n{src}"),
        Err(e) => e.msg,
    }
}

fn assert_fehler(src: &str, teil: &str) {
    let msg = fehler(src);
    assert!(msg.contains(teil), "erwartet `{teil}`, Meldung war: {msg}");
}

#[test]
fn unbekannte_namen() {
    assert_fehler("fn main() { let x = y; }", "unbekannte Variable `y`");
    assert_fehler("fn main() { foo(); }", "unbekannte Funktion `foo`");
    assert_fehler("fn haupt() {}", "keine Funktion `main`");
}

#[test]
fn typfehler() {
    assert_fehler(
        "fn main() { let x: bool = 1; }",
        "erwartet `bool`, gefunden `i32`",
    );
    assert_fehler(
        "fn main() { let a: i64 = 1; let b: i32 = 2; let c = a + b; }",
        "nicht anwendbar auf `i64` und `i32`",
    );
    assert_fehler("fn main() { if 1 { } }", "Bedingung");
    assert_fehler(
        "fn f() -> i32 { true } fn main() {}",
        "muss `i32` zurückgeben",
    );
    assert_fehler(
        "fn main() { let x = if true { 1 } else { false }; }",
        "verschiedene Typen",
    );
    assert_fehler("fn main() { let x = if true { 1 }; }", "ohne `else`");
    assert_fehler(
        "fn main() { let x: u8 = 256; }",
        "ausserhalb des Wertebereichs von `u8`",
    );
    assert_fehler(
        "fn main() { let x = -1 as u32; }",
        "vorzeichenlosen Typ `u32`",
    );
    assert_fehler(
        "fn main() { let x: u32 = 5; let y = -x; }",
        "nicht anwendbar",
    );
}

#[test]
fn veraenderlichkeit() {
    assert_fehler("fn main() { let x = 1; x = 2; }", "nicht veränderlich");
    assert_fehler(
        "fn main() { let a = [0; 3]; a[0] = 1; }",
        "nicht veränderlich",
    );
    assert_fehler(
        "fn main() { for i in 0..3 { i += 1; } }",
        "nicht veränderlich",
    );
    // `mut`-Parameter sind erlaubt
    assert!(rust2c::compile("fn f(mut x: i32) -> i32 { x += 1; x } fn main() { f(1); }").is_ok());
}

#[test]
fn funktionsaufrufe() {
    assert_fehler(
        "fn f(a: i32) {} fn main() { f(); }",
        "erwartet 1 Argument(e)",
    );
    assert_fehler(
        "fn f(a: i32) {} fn main() { f(true); }",
        "Argument 1 von `f`",
    );
    assert_fehler("fn f() {} fn f() {} fn main() {}", "mehrfach definiert");
}

#[test]
fn schleifen() {
    assert_fehler("fn main() { break; }", "`break` ausserhalb einer Schleife");
    assert_fehler(
        "fn main() { continue; }",
        "`continue` ausserhalb einer Schleife",
    );
}

#[test]
fn arrays() {
    assert_fehler("fn main() { let a = [1, 2]; let b = a; }", "nur indiziert");
    assert_fehler(
        "fn main() { let a = [1, 2]; let i: i32 = 0; let x = a[i]; }",
        "`usize`",
    );
    assert_fehler(
        "fn main() { let a = [1, 2]; let x = a[2]; }",
        "ausserhalb des Arrays",
    );
    assert_fehler(
        "fn main() { let a = [1, true]; }",
        "erwartet `i32`, gefunden `bool`",
    );
}

#[test]
fn syntaxfehler() {
    assert_fehler("fn main() { let x = 1 }", "`;` erwartet");
    assert_fehler("fn main() { let x = (1 + 2; }", "`)` erwartet");
    assert_fehler("fn main() { foo!(); }", "unbekanntes Makro");
    assert_fehler("fn main() { println!(\"{:>5}\", 1); }", "Formatoptionen");
    assert_fehler("fn main() { let s = \"text\"; }", "Ausdruck erwartet");
}

#[test]
fn position_und_darstellung() {
    let src = "fn main() {\n    let x = 1;\n    x = 2;\n}\n";
    let e = rust2c::compile(src).unwrap_err();
    assert_eq!((e.span.line, e.span.col), (3, 5));
    let text = e.render(src, "test.rs");
    assert!(text.contains("--> test.rs:3:5"), "{text}");
    assert!(text.contains("3 |     x = 2;"), "{text}");
}
