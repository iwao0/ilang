// `console.log(obj)` field order matches between interp and JIT.
// Pre-fix the JIT sorted alphabetically (`emit_print_object` used
// `as_str().cmp(...)`) while the interpreter sorted by `Symbol`
// intern id, so a class declared with fields `n, s, a` printed
// `Foo { n: 5, s: hi, a: [1, 2] }` from the interpreter and
// `Foo { a: [1, 2], n: 5, s: hi }` from the JIT — the harness's
// parity check caught this once a fixture existed.
//
// expect: Foo { n: 5, s: hi, a: [1, 2] }

class Foo {
    n: i64
    s: string
    a: i64[]
    init() {
        this.n = 5
        this.s = "hi"
        this.a = [1, 2]
    }
}

console.log(new Foo())
