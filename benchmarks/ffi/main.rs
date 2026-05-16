// Same shape as the C version — route `abs` through an `extern "C"`
// import so the cost of an FFI boundary is measurable.
unsafe extern "C" {
    fn abs(x: i32) -> i32;
}

const N: i64 = 100_000_000;

fn main() {
    let mut sum: i64 = 0;
    for i in 0..N {
        sum += unsafe { abs((i - N / 2) as i32) } as i64;
    }
    println!("{sum}");
}
