// `@repr(C) class` basics: declare, allocate, read/write fields.
// Doesn't go through native extern — just exercises the layout
// and zero-init semantics so failures are easy to localize.
use test

@repr(C) class point {
    x: i32
    y: i32
}

@repr(C) class mixed {
    flag: bool
    count: i64
    ratio: f64
}

let p = new point()
test.expect(p.x as i64, 0)
test.expect(p.y as i64, 0)

p.x = 100 as i32
p.y = -50 as i32
test.expect(p.x as i64, 100)
test.expect(p.y as i64, -50)

let m = new mixed()
test.expectFalse(m.flag)
test.expect(m.count, 0)
test.expectF64(m.ratio, 0.0)

m.flag = true
m.count = 1234567890
m.ratio = 3.14
test.expectTrue(m.flag)
test.expect(m.count, 1234567890)
test.expectF64(m.ratio, 3.14)
