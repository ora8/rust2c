// Sieb des Eratosthenes: alle Primzahlen bis 100.
fn main() {
    let mut ist_prim = [true; 101];
    ist_prim[0] = false;
    ist_prim[1] = false;

    let mut i: usize = 2;
    while i * i < ist_prim.len() {
        if ist_prim[i] {
            let mut j = i * i;
            while j < ist_prim.len() {
                ist_prim[j] = false;
                j += i;
            }
        }
        i += 1;
    }

    let mut anzahl = 0;
    for k in 0..ist_prim.len() {
        if ist_prim[k] {
            print!("{} ", k);
            anzahl += 1;
        }
    }
    println!();
    println!("{anzahl} Primzahlen gefunden");
}
