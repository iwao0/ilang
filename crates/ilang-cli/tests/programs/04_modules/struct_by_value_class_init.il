// Regression: passing a multi-field struct by value as a positional
// init argument used to crash the Cranelift verifier — the call site
// pushed a single pointer while `clif_signature_for` declared the
// CRepr param spread across HFA float registers (4×f64 for a `Quad`)
// or i64 chunks. Symptom was either "got N, expected M" (arg count
// mismatch) or "arg has type i64, expected f64" (slot-type mismatch).
//
// `lower_new_object` now mirrors `lower_call`'s CRepr by-value
// expansion before invoking the init, matching the declared signature.
use std.test as test

pub struct Quad {
    r: f64
    g: f64
    b: f64
    a: f64
}

pub struct Pair {
    x: f64
    y: f64
}

pub class QuadHolder {
    pub q: Quad
    pub init(q: Quad) { this.q = q }
}

pub class PairHolder {
    pub p: Pair
    pub init(p: Pair) { this.p = p }
}

let q = Quad { r: 0.1, g: 0.2, b: 0.3, a: 0.4 }
let h = new QuadHolder(q)
test.expectF64(h.q.r, 0.1)
test.expectF64(h.q.g, 0.2)
test.expectF64(h.q.b, 0.3)
test.expectF64(h.q.a, 0.4)

let p = Pair { x: 1.5, y: 2.5 }
let ph = new PairHolder(p)
test.expectF64(ph.p.x, 1.5)
test.expectF64(ph.p.y, 2.5)
