// Built-in `math` module — shipped inside the compiler, no on-disk
// `math.il` needed.
use test
use math

test.expectF64(math.sqrt(16.0), 4.0)
test.expectF64(math.pow(2.0, 10.0), 1024.0)
test.expectF64(math.floor(3.9), 3.0)
test.expectF64(math.ceil(3.1), 4.0)
test.expectF64(math.abs(-7.5), 7.5)

// Constants — `math.pi` / `math.e` are top-level consts in the stdlib.
test.expectF64(math.sin(math.pi / 2.0), 1.0)
test.expectF64(math.cos(0.0), 1.0)

// Math accepts integer args via implicit promotion.
test.expectF64(math.sqrt(25), 5.0)
