// Nested `@repr(C) struct`: an outer struct can embed another
// `@repr(C) struct` inline (no pointer, no separate allocation).
// Field offsets follow C struct layout, so the bytes are exactly
// where a C compiler would place them.
use test

@extern(C) {
    @repr(C) struct point {
        x: i32
        y: i32
    }

    @repr(C) struct rect {
        origin: point
        width: i32
        height: i32
    }
}

let r = new rect()

// Nested writes via chain access.
r.origin.x = 10 as i32
r.origin.y = 20 as i32
r.width = 100 as i32
r.height = 50 as i32

// Nested reads via chain access.
test.expect(r.origin.x as i64, 10)
test.expect(r.origin.y as i64, 20)
test.expect(r.width as i64, 100)
test.expect(r.height as i64, 50)

// Aggregate literal: build a fresh struct in one shot.
let p = point { x: -3 as i32, y: 8 as i32 }
test.expect(p.x as i64, -3)
test.expect(p.y as i64, 8)

// Aggregate literal mixed with chain access.
let r2 = rect {
    origin: point { x: 1 as i32, y: 2 as i32 },
    width: 99 as i32,
    height: 11 as i32,
}
test.expect(r2.origin.x as i64, 1)
test.expect(r2.origin.y as i64, 2)
test.expect(r2.width as i64, 99)
test.expect(r2.height as i64, 11)
