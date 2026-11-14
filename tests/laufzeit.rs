//! Laufzeitverhalten: Semantik von Rust im erzeugten C-Code.

mod common;

fn run(src: &str) -> (Option<i32>, String, String) {
    let out = common::compile_and_run("laufzeit", src);
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn assert_panics(src: &str, msg: &str) {
    let (code, _, err) = run(src);
    assert_eq!(code, Some(101), "Programm sollte mit 101 enden:\n{src}");
    assert!(err.contains(msg), "erwartet `{msg}` in stderr, war:\n{err}");
}

#[test]
fn ueberlauf_bricht_ab() {
    assert_panics(
        "fn main() { let mut x: u8 = 250; x += 10; println!(\"{x}\"); }",
        "attempt to add with overflow",
    );
    assert_panics(
        "fn f(a: i32) -> i32 { a * 2 } fn main() { println!(\"{}\", f(2000000000)); }",
        "attempt to multiply with overflow",
    );
    assert_panics(
        "fn main() { let x: u32 = 0; let y = x - 1; println!(\"{y}\"); }",
        "attempt to subtract with overflow",
    );
    assert_panics(
        "fn main() { let x: i8 = -128; let y = -x; println!(\"{y}\"); }",
        "attempt to negate with overflow",
    );
}

#[test]
fn division_durch_null() {
    assert_panics(
        "fn teile(a: i32, b: i32) -> i32 { a / b } fn main() { println!(\"{}\", teile(1, 0)); }",
        "attempt to divide by zero",
    );
    assert_panics(
        "fn main() { let a: i32 = -2147483648; let b = -1; println!(\"{}\", a / b); }",
        "attempt to divide with overflow",
    );
}

#[test]
fn indexfehler() {
    assert_panics(
        "fn main() { let a = [1, 2, 3]; let mut i: usize = 0; while i < 5 { println!(\"{}\", a[i]); i += 1; } }",
        "index out of bounds: the len is 3 but the index is 3",
    );
}

#[test]
fn schiebeweite() {
    assert_panics(
        "fn main() { let s: u32 = 40; let x: u32 = 1; println!(\"{}\", x << s); }",
        "attempt to shift left with overflow",
    );
}

#[test]
fn panic_makro() {
    let (code, out, err) =
        run("fn main() { println!(\"vorher\"); let n = 3; panic!(\"kaputt: {n}\"); }");
    assert_eq!(code, Some(101));
    assert_eq!(out, "vorher\n");
    assert!(err.contains("kaputt: 3"), "{err}");
}

#[test]
fn auswertungsreihenfolge_links_nach_rechts() {
    let (_, out, _) = run("fn p(x: i32) -> i32 { print!(\"{x} \"); x }
         fn drei(a: i32, b: i32, c: i32) -> i32 { a + b + c }
         fn main() { let s = drei(p(1), p(2), p(3)) + p(4); println!(\"= {s}\"); }");
    assert_eq!(out, "1 2 3 4 = 10\n");
}

#[test]
fn kurzschluss_mit_seiteneffekt() {
    let (_, out, _) = run("fn laut(b: bool) -> bool { println!(\"laut({b})\"); b }
         fn main() {
             let a = false && laut(true);
             let b = true || laut(false);
             let c = true && laut(true);
             println!(\"{a} {b} {c}\");
         }");
    assert_eq!(out, "laut(true)\nfalse true true\n");
}

#[test]
fn inklusiver_bereich_bis_maximum() {
    let (code, out, _) = run(
        "fn main() { let mut n: u32 = 0; for i in 250..=255u8 { n += i as u32; } println!(\"{n}\"); }",
    );
    assert_eq!(code, Some(0));
    assert_eq!(out, "1515\n");
}

#[test]
fn rekursion_und_vorwaertsdeklaration() {
    let (_, out, _) = run("fn main() { println!(\"{} {}\", gerade(10), gerade(7)); }
         fn gerade(n: u32) -> bool { if n == 0 { true } else { ungerade(n - 1) } }
         fn ungerade(n: u32) -> bool { if n == 0 { false } else { gerade(n - 1) } }");
    assert_eq!(out, "true false\n");
}

#[test]
fn c_schluesselwoerter_als_namen() {
    let (_, out, _) = run(
        "fn main() { let int = 1; let char = 2; let printf = int + char; println!(\"{printf}\"); }",
    );
    assert_eq!(out, "3\n");
}

#[test]
fn if_als_wert_mit_seiteneffekten_in_bedingung() {
    let (_, out, _) = run("fn f(x: i32) -> i32 { x * 2 }
         fn main() {
             let x = 5;
             let y = if f(x) > 20 { 1 } else if f(x) > 5 { 2 } else { 3 };
             let mut z = 0;
             while f(z) < 10 { z += 1; }
             println!(\"{y} {z}\");
         }");
    assert_eq!(out, "2 5\n");
}
