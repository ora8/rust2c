// Rekursive Fakultät mit u64 – 20! ist das grösste, das in u64 passt.
fn fakultaet(n: u64) -> u64 {
    if n <= 1 { 1 } else { n * fakultaet(n - 1) }
}

fn main() {
    for i in 0..=20u64 {
        println!("{i}! = {}", fakultaet(i));
    }
}
