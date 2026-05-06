// `@extern(C) struct` layout no longer depends on declaration order.
// Here `rect` is declared **before** the `point` it embeds; the JIT
// topologically sorts struct layouts so this still resolves cleanly.
use test

@extern(C) {
    struct rect {
        origin: point   // forward reference — point declared later
        extent: point
        flags: u32
    }

    struct point {
        x: i32
        y: i32
    }
}

let r = new rect()
r.origin.x = 10 as i32
r.origin.y = 20 as i32
r.extent.x = 100 as i32
r.extent.y = 50 as i32
r.flags = 7 as u32
test.expect(r.origin.x as i64, 10)
test.expect(r.origin.y as i64, 20)
test.expect(r.extent.x as i64, 100)
test.expect(r.extent.y as i64, 50)
test.expect(r.flags as i64, 7)
