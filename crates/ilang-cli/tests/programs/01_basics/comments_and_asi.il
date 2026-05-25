// Line comments are ignored.
use std.test as test
/*
 * Block comments span lines.
 */
test.expect(1 + 1, 2)

/* Nested block comments work too: /* inner */ outer is still a comment */
test.expect(2 * 3, 6)

// ASI: newline ends a statement; explicit `;` and `}` also do.
let a = 1
let b = 2
test.expect(a + b, 3)

let c = 10; let d = 20; test.expect(c + d, 30)   // semicolons OK on one line

// ASI: an expression spanning a newline is still a single expression
// when a binary operator continues onto the next line.
let big = 1 +
          2 +
          3
test.expect(big, 6)

// ASI: a let on its own line, then a block-tail expression — both
// terminated naturally.
let z = {
    let inner = 7
    inner * 6
}
test.expect(z, 42)
