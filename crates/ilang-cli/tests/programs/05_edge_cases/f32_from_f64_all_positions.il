// Clean-round pin for f64 -> f32 narrowing (rounds 134/135): a bare
// float literal (typed f64) demotes to f32 in EVERY construction /
// storage position. Rounds 134/135 fixed the two that were broken
// (object fields, and the numeric-Optional wrap); the rest already
// coerced through lower_arg_to and friends. This pins them all so a
// future change to the coercion plumbing can't silently regress one.
//
// expect: ok
use std.test as test

enum Shape {
    dot
    pt: (f32, f32)
    rec: { x: f32, y: f32 }
}
fn ptx(s: Shape): f32 {
    match s {
        pt(x, y) { x }
        rec { x, y } { x }
        dot { return 0.0f32 }
    }
}
fn withDef(a: f32 = 3.5): f32 { a }

// dynamic array literal
let a: f32[] = [1.5, 2.5, 3.5]
test.expectF64(a[0] as f64, 1.5)
test.expectF64(a[2] as f64, 3.5)
// element store
a[1] = 9.5
test.expectF64(a[1] as f64, 9.5)

// fixed-length array literal
let fa: f32[2] = [4.5, 5.5]
test.expectF64(fa[1] as f64, 5.5)

// enum tuple payload + struct payload
test.expectF64(ptx(Shape.pt(6.5, 7.5)) as f64, 6.5)
test.expectF64(ptx(Shape.rec { x: 8.5, y: 9.5 }) as f64, 8.5)

// tuple literal
let t: (f32, i64) = (10.5, 1)
test.expectF64(t[0] as f64, 10.5)

// Set element dedup (structural f32)
let st = new Set<f32>()
st.add(1.5)
st.add(1.5)
st.add(2.5)
test.expect(st.size(), 2)
test.expectTrue(st.has(1.5))

// Map value
let m = new Map<string, f32>()
m["k"] = 5.5
test.expectF64(m["k"] as f64, 5.5)

// default parameter (omitted and explicit)
test.expectF64(withDef() as f64, 3.5)
test.expectF64(withDef(4.5) as f64, 4.5)

// closure return + variable reassignment
let cl = fn(): f32 { 5.5 }
test.expectF64(cl() as f64, 5.5)
let v: f32 = 0.0f32
v = 7.25
test.expectF64(v as f64, 7.25)

console.log("ok")
