// Enums whose payload itself contains another enum / a dynamic
// array / an Optional. The match arm has to walk one level
// deeper than the flat `enum_payload_tuple.il` cases, and the
// ARC cascade has to release the inner heap value when the
// outer enum cell dies. Per syntax.md §11.
use std.test as test

// Nested enum payload — the outer match binds an `Inner`, then
// matches it again inside the arm.
enum Inner {
    plain
    tagged: (i64)
}
enum Outer {
    nothing
    wrap: (Inner)
    pair: (Inner, string)
}

fn label(o: Outer): string {
    match o {
        nothing { "none" }
        wrap(i) {
            match i {
                plain { "wrap-plain" }
                tagged(n) { "wrap-tagged-" + n.toString() }
            }
        }
        pair(i, s) {
            match i {
                plain { "pair-plain-" + s }
                tagged(n) { "pair-" + n.toString() + "-" + s }
            }
        }
    }
}
test.expectStr(label(Outer.nothing), "none")
test.expectStr(label(Outer.wrap(Inner.plain)), "wrap-plain")
test.expectStr(label(Outer.wrap(Inner.tagged(7))), "wrap-tagged-7")
test.expectStr(label(Outer.pair(Inner.plain, "x")), "pair-plain-x")
test.expectStr(label(Outer.pair(Inner.tagged(3), "y")), "pair-3-y")

// Dynamic-array payload — the match arm iterates the heap array
// without leaking it.
enum Bag {
    empty
    items: (i64[])
}

fn total(b: Bag): i64 {
    match b {
        empty { 0 }
        items(xs) {
            let s = 0
            for x in xs { s += x }
            s
        }
    }
}
test.expect(total(Bag.empty), 0)
test.expect(total(Bag.items([1, 2, 3, 4])), 10)
test.expect(total(Bag.items([])), 0)

// Optional payload — the inner Optional is independently some/none.
enum Maybe {
    here: (i64?)
}

fn unwrap_or(m: Maybe, fallback: i64): i64 {
    match m {
        here(o) {
            if let some(v) = o { v } else { fallback }
        }
    }
}
test.expect(unwrap_or(Maybe.here(some(7)), -1), 7)
test.expect(unwrap_or(Maybe.here(none), -1), -1)
