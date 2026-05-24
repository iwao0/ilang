// Struct literal syntax: `T { f1: v1, f2: v2 }`. For top-level
// `struct` / `union` (CRepr) the literal must initialize every
// declared field — partial / missing fields are a type error so a
// silent zero-init can't slip through. Wrong field name, wrong
// value type, and duplicate field name are all caught at type
// check.
use std.test

struct Point {
    x: i32
    y: i32
}

pub struct Rect {
    width: i32
    height: i32
}

union Value {
    i: i64
    f: f64
}

// Construction via literal. Field declaration order vs. literal
// order is irrelevant — only the name set matters.
let p = Point { x: 3, y: 4 }
test.expect(p.x as i64, 3)
test.expect(p.y as i64, 4)

let r = Rect { height: 20, width: 10 }
test.expect(r.width as i64, 10)
test.expect(r.height as i64, 20)

// Union literal — single field is required (giving zero fields
// would also miss the only declared name).
let v = Value { i: 42 }
test.expect(v.i, 42)
