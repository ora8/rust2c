# rust2c

Ein kleiner Compiler, geschrieben in Rust, der eine **Teilmenge von Rust nach C11** übersetzt.
Das erzeugte C-Programm wird anschliessend mit einem gewöhnlichen C-Compiler (`gcc`, `clang`)
zu einer ausführbaren Datei.

Das Projekt hat Lerncharakter: Jede Compilerphase steht in einem eigenen, überschaubaren Modul,
und es gibt keine externen Abhängigkeiten.

```text
Quelltext ─► Lexer ─► Parser ─► Typprüfung ─► Codegenerierung ─► C-Code ─► cc ─► Programm
             Tokens    AST      AST + Typen
```

## Schnellstart

```bash
cargo build --release
```

Übersetzen und direkt ausführen:

```bash
cargo run -- beispiele/sieb.rs --run
```

Nur den C-Code erzeugen (schreibt `beispiele/sieb.c`):

```bash
cargo run -- beispiele/sieb.rs
```

C-Code auf die Standardausgabe schreiben:

```bash
cargo run -- beispiele/hallo.rs -o -
```

### Optionen

| Option        | Bedeutung                                                         |
|---------------|-------------------------------------------------------------------|
| `-o <datei>`  | C-Ausgabe in `<datei>` (`-` = Standardausgabe), Standard: `<quelle>.c` |
| `-r`, `--run` | mit dem C-Compiler übersetzen und sofort ausführen                |
| `--cc <cc>`   | C-Compiler wählen (Standard: Umgebungsvariable `CC`, sonst `cc`)  |
| `-h`, `--help`| Hilfe                                                             |

Bei `--run` gibt `rust2c` den Exit-Code des Programms weiter.

## Beispiel

```rust
fn main() {
    let x: i32 = 7;
    println!("{}", x * 3);
}
```

wird zu (gekürzt):

```c
static void rs_main(void) {
    int32_t v1_x = 7;
    int32_t _t2;
    if (__builtin_mul_overflow(v1_x, 3, &_t2)) rs_panic("attempt to multiply with overflow", 3, 22);
    printf("%lld\n", (long long)_t2);
}
```

Weitere Programme liegen in [`beispiele/`](beispiele/):

| Datei           | Zeigt                                                          |
|-----------------|----------------------------------------------------------------|
| `hallo.rs`      | `println!` mit Platzhaltern                                    |
| `fakultaet.rs`  | Rekursion, `u64`, `for i in 0..=20u64`                         |
| `fibonacci.rs`  | `while`, `loop`/`break`, benannte Platzhalter `{n}`            |
| `sieb.rs`       | Sieb des Eratosthenes mit `[bool; 101]`, `usize`, `.len()`     |
| `rundgang.rs`   | fast alle Sprachmittel: Shadowing, Bitoperationen, `as`, `if` als Wert, Blöcke als Ausdrücke, `continue`, Grenzwerte |

Alle Beispiele sind **gültiges Rust**: Ein Test übersetzt sie zusätzlich mit `rustc` und
prüft, dass beide Programme Zeichen für Zeichen dasselbe ausgeben.

## Unterstützte Sprache

- **Funktionen** mit Parametern (auch `mut`) und Rückgabewert, Rekursion, beliebige
  Reihenfolge der Definitionen; `fn main()` ist der Einstiegspunkt.
- **Typen:** `i8` `i16` `i32` `i64` `isize` `u8` `u16` `u32` `u64` `usize` `bool`
  sowie Arrays `[T; N]` aus Ganzzahlen oder `bool`.
- **Variablen:** `let`, `let mut`, optionale Typangabe, Shadowing.
- **Literale:** dezimal, `0x`, `0o`, `0b`, `_` als Trenner, Typsuffixe (`255u8`, `3usize`).
- **Operatoren** mit der Rangfolge von Rust:
  `+ - * / %`, `& | ^ << >>`, `== != < <= > >=`, `&& ||` (Kurzschluss), `! -`, `as`,
  zusammengesetzte Zuweisungen `+=`, `<<=`, …
- **Kontrollfluss:** `if`/`else if`/`else` (auch als Wert), `while`, `loop`,
  `for i in a..b` und `a..=b`, `break`, `continue`, `return`.
- **Blöcke als Ausdrücke:** `let m = { let n = 3; n * n };`, letzter Ausdruck ohne `;` als
  Rückgabewert.
- **Arrays:** `[wert; n]`, `[a, b, c]`, Indizierung `a[i]` (Index vom Typ `usize`), `a.len()`.
- **Ausgabe:** `println!`, `print!`, `panic!` mit `{}`, `{0}`, `{name}` und `{{ }}`.
- **Kommentare:** `//` und (verschachtelbar) `/* */`.

### Semantik wie in Rust (Debug-Modus)

Der erzeugte Code bildet das Verhalten von Rust nach, nicht das von C:

- **Überlauf** bei `+ - *`, unärem `-`, Division und Schiebeweite bricht das Programm
  mit Exit-Code **101** ab (Meldung wie `attempt to add with overflow`).
- **Division durch null** und **Indexfehler** ebenso
  (`index out of bounds: the len is 3 but the index is 5`).
- **Auswertungsreihenfolge** strikt von links nach rechts, auch bei Funktionsargumenten,
  wo C sie offen lässt.
- `as` schneidet ab bzw. erweitert wie in Rust; `<<` arbeitet intern vorzeichenlos,
  damit kein undefiniertes Verhalten in C entsteht.
- `&` bindet stärker als `==` (`n & 1 == 0` bedeutet `(n & 1) == 0`).

Der erzeugte C-Code übersetzt mit `gcc` und `clang` unter `-std=c11 -Wall -Wextra` ohne
Warnungen. Er verwendet die Builtins `__builtin_*_overflow` und benötigt daher GCC oder Clang.

### Fehlermeldungen

Fehler werden mit Position und Quelltextzeile gemeldet, ähnlich wie bei `rustc`:

```text
fehler: `zaehler` ist nicht veränderlich; deklariere die Variable mit `let mut zaehler`
 --> falsch.rs:3:5
  |
3 |     zaehler += 1;
  |     ^
```

Geprüft werden unter anderem Typen von Operanden, Argumenten und Rückgabewerten, Wertebereiche
von Literalen, Veränderlichkeit, `break`/`continue` ausserhalb von Schleifen, konstante
Array-Indizes und die Zuordnung der Formatstring-Platzhalter.

## Grenzen

Bewusst **nicht** enthalten (noch nicht): Strukturen, Enums, `match`, Referenzen und
Borrowing, Zeichenketten als Werte, Gleitkommazahlen, `char`, Generics, Traits, Closures,
Module, Arrays als Funktionsparameter, Formatoptionen wie `{:>5}`.

Die Typinferenz ist einfach: Ein Literal ohne Suffix übernimmt den Typ aus dem unmittelbaren
Kontext (Typangabe, Parameter, anderer Operand, Ziel von `as`), sonst ist es `i32`.
Rust schliesst Typen auch aus späterer Verwendung; hier braucht es dann eine Angabe, z. B.
`let mut i: u32 = 0;` oder `for i in 0..=20u64`.

## Aufbau

| Datei             | Aufgabe                                                            |
|-------------------|--------------------------------------------------------------------|
| `src/lexer.rs`    | Zerlegt den Quelltext in Tokens (mit Zeile/Spalte)                 |
| `src/parser.rs`   | Rekursiver Abstieg mit Präzedenzklettern, erzeugt den AST          |
| `src/ast.rs`      | Datenstrukturen des Syntaxbaums                                    |
| `src/typeck.rs`   | Namensauflösung, Typprüfung, trägt Typen in den AST ein            |
| `src/codegen.rs`  | Übersetzt den typisierten AST nach C11                             |
| `src/types.rs`    | Typen der Quellsprache und ihre C-Entsprechungen                   |
| `src/error.rs`    | Fehlertyp und Darstellung der Fehlermeldungen                      |
| `src/main.rs`     | Kommandozeilenprogramm                                             |

**Codegenerierung:** Ein Ausdruck wird zu einem *reinen* C-Ausdruck. Alles mit
Seiteneffekten (Funktionsaufrufe, geprüfte Arithmetik, `if` als Wert) wird vorher als
Anweisung ausgegeben und in einer temporären Variablen `_tN` abgelegt. So bleibt die
Auswertungsreihenfolge erhalten. Rust-Variablen erhalten eindeutige C-Namen
(`v3_zaehler`); das ermöglicht Shadowing, und Kollisionen mit C-Schlüsselwörtern sind
ausgeschlossen. Funktionen heissen in C `rs_<name>`.

## Tests

```bash
cargo test
```

- Unit-Tests für Lexer, Parser (Rangfolge, Formatstrings) und Codegenerierung,
- `tests/beispiele.rs`: übersetzt alle Beispiele, vergleicht mit `*.erwartet` und mit `rustc`,
- `tests/laufzeit.rs`: Überlauf, Division durch null, Indexfehler, `panic!`,
  Auswertungsreihenfolge, Kurzschluss, Bereiche bis zum Maximalwert,
- `tests/fehler.rs`: Fehlermeldungen und ihre Positionen.

Für die Integrationstests muss ein C-Compiler (`cc` oder `$CC`) vorhanden sein. Der Vergleich
mit `rustc` wird übersprungen, wenn `rustc` fehlt.
