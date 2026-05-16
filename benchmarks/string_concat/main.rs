// Match the other languages' O(n²) `s = s + "x"` shape rather than
// pre-reserving capacity. Each iteration produces a fresh `String`.
const N: i64 = 500_000;

fn main() {
    let mut s = String::new();
    for _ in 0..N {
        s = s + "x";
    }
    println!("{}", s.len());
}
