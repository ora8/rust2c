// Iterative Fibonacci-Zahlen mit while und loop/break.
fn fib(n: u32) -> u64 {
    let mut a: u64 = 0;
    let mut b: u64 = 1;
    let mut i: u32 = 0;
    while i < n {
        let t = a + b;
        a = b;
        b = t;
        i += 1;
    }
    a
}

fn main() {
    let mut n: u32 = 0;
    loop {
        if n > 90 {
            break;
        }
        if n % 10 == 0 {
            println!("fib({n}) = {}", fib(n));
        }
        n += 1;
    }
}
