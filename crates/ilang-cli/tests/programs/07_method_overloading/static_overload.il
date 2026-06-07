// `static` methods can be overloaded — same arity/type rules as
// instance methods, mangled to `name__<param_types>` after type
// checking. Per syntax.md §7 `static` methods.
use std.test as test

class K {
    // Arity-based pair: `n` vs `()`.
    static foo(): i64 { 1 }
    static foo(n: i64): i64 { n }

    // Parameter-type pair: i64 vs string.
    static tag(n: i64): string { "i:" + n.toString() }
    static tag(s: string): string { "s:" + s }
}

// 0-arg overload.
test.expect(K.foo(), 1)
// 1-arg overload (i64).
test.expect(K.foo(7), 7)

// Same-arity overloads dispatched purely on argument type.
test.expectStr(K.tag(42), "i:42")
test.expectStr(K.tag("hi"), "s:hi")
