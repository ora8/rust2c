// Ein Rundgang durch die unterstützten Sprachmittel.

fn ggt(a: i64, b: i64) -> i64 {
    if b == 0 { a } else { ggt(b, a % b) }
}

fn ist_gerade(n: i32) -> bool {
    // In Rust bindet `&` stärker als `==`.
    n & 1 == 0
}

fn collatz_schritte(start: u64) -> u32 {
    let mut n = start;
    let mut schritte: u32 = 0;
    while n != 1 {
        n = if n % 2 == 0 { n / 2 } else { 3 * n + 1 };
        schritte += 1;
    }
    schritte
}

fn einordnen(x: i32) -> i32 {
    if x < 0 {
        return -1;
    } else if x == 0 {
        0
    } else {
        1
    }
}

fn main() {
    println!("ggt(1071, 462) = {}", ggt(1071, 462));

    // Shadowing
    let x = 10;
    let x = x * 3;
    println!("x = {x}");

    for n in 1..=10u64 {
        println!("collatz({n}) braucht {} Schritte", collatz_schritte(n));
    }

    // Bitoperationen
    let mut bits: u8 = 0b1010_0101;
    bits ^= 0xFF;
    println!("bits = {}, verschoben = {}, negiert = {}", bits, bits << 2, !bits);
    println!("gerade(7) = {}, gerade(8) = {}", ist_gerade(7), ist_gerade(8));

    // Ganzzahldivision rundet wie in C gegen null
    let negativ = -17;
    println!("{} / 5 = {}, {} % 5 = {}", negativ, negativ / 5, negativ, negativ % 5);

    // Umwandlungen mit `as` schneiden ab
    let gross: i32 = 300;
    println!("300 als u8 = {}, -1 als u32 = {}", gross as u8, -1i32 as u32);
    println!("true als i32 = {}", true as i32);

    // Arrays, Indizes und Blöcke als Ausdrücke
    let a = [3, 1, 4, 1, 5, 9, 2, 6];
    let mut summe = 0;
    let mut maximum = a[0];
    for i in 0..a.len() {
        summe += a[i];
        if a[i] > maximum {
            maximum = a[i];
        }
    }
    let mittel = {
        let n = a.len() as i32;
        summe / n
    };
    println!("Summe = {summe}, Maximum = {maximum}, Mittel = {mittel}");

    // Kurzschlussauswertung: ggt wird nicht aufgerufen
    let ok = false && ggt(1, 0) == 1;
    println!("ok = {ok}, Vorzeichen: {} {} {}", einordnen(-5), einordnen(0), einordnen(7));

    // Escape-Sequenzen und geschweifte Klammern
    println!("{{wörtlich}} 100% \"zitiert\"\tTab");

    // Grenzwerte
    let klein: i64 = -9223372036854775808;
    let gross_u: u64 = 18446744073709551615;
    println!("i64::MIN = {klein}, u64::MAX = {gross_u}");

    let mut zaehler = 0;
    while zaehler < 100 {
        zaehler += 1;
        if zaehler % 7 != 0 {
            continue;
        }
        if zaehler > 30 {
            break;
        }
        print!("{zaehler} ");
    }
    println!();
}
