// ObjCBlock — create + invoke round-trip for every currently
// supported signature shape. Pure ilang round-trip; doesn't go
// out to any Cocoa selector.

use test

// void(^)(void).
let nilCount = 0
let voidBlock = new ObjCBlock(fn() { })
voidBlock.invoke()
test.expect(nilCount, 0)

// void(^)(id) — pass an integer through as a raw handle.
let three = new ObjCBlock(fn(x: i64) {
    test.expect(x, 7)
})
three.invoke(7)

// id(^)(id) — square the argument.
let sq = new ObjCBlock(fn(x: i64): i64 { x * x })
test.expect(sq.invoke(6), 36)
test.expect(sq.invoke(11), 121)

// void(^)(void *, size_t) — two i64s.
let pair = new ObjCBlock(fn(a: i64, b: i64) {
    test.expect(a + b, 30)
})
pair.invoke(10, 20)

// void(^)(id, id, id) — three i64s.
let triple = new ObjCBlock(fn(a: i64, b: i64, c: i64) {
    test.expect(a * b * c, 24)
})
triple.invoke(2, 3, 4)

// void(^)(BOOL).
let flagBlock = new ObjCBlock(fn(f: bool) {
    test.expectTrue(f)
})
flagBlock.invoke(true)

0
