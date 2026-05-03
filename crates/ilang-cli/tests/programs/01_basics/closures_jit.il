// Closures with i64 / f64 / bool captures — runs on both
// interpreter and JIT (Stage A). Heap-typed captures (string,
// objects, arrays) are tested separately in `closures.il` with
// `// jit: skip`.
use test

let factor = 10
let scale = fn(x: i64): i64 { x * factor }
test.expect(scale(3), 30)
test.expect(scale(7), 70)

// Capture-by-value: shadowing the outer doesn't affect captured.
let factor = 999
test.expect(scale(3), 30)

// Closures returned from functions.
fn make_adder(n: i64): fn(i64): i64 {
    fn(x: i64): i64 { x + n }
}
let add5 = make_adder(5)
let add100 = make_adder(100)
test.expect(add5(3), 8)
test.expect(add100(3), 103)

// Closures + array.map.
let xs: i64[] = [1, 2, 3, 4]
let mult = 7
let ys = xs.map(fn(x: i64): i64 { x * mult })
test.expect(ys[0], 7)
test.expect(ys[1], 14)
test.expect(ys[2], 21)
test.expect(ys[3], 28)

// f64 capture.
let bias = 0.5
let shift = fn(x: f64): f64 { x + bias }
test.expectF64(shift(1.0), 1.5)

// bool capture.
let flag = true
let pick = fn(a: i64, b: i64): i64 { if flag { a } else { b } }
test.expect(pick(7, 99), 7)

// Top-level fn assigned to a variable (trampoline closure).
fn double(n: i64): i64 { n * 2 }
let f = double
test.expect(f(21), 42)
let doubled = xs.map(double)
test.expect(doubled[3], 8)
