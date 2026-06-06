// A child class with no `init` of its own inherits the parent's
// init signature. The child's own fields fall back to the
// type-driven defaults (`0` / `""` / `false` / `none` / `[]` /
// dead weak) listed in syntax.md §7 "Field defaults / required
// init assignment".
use std.test as test

class Parent {
    label: string
    n: i64
    init(l: string, k: i64) {
        this.label = l
        this.n = k
    }
    show(): string { this.label }
}

class Child: Parent {
    // No `init` here — calls Parent's signature.
    extra_int: i64           // default 0
    extra_str: string        // default ""
    extra_flag: bool         // default false
    extra_opt: i64?          // default none
    extra_arr: i64[]         // default []
}

let c = new Child("kid", 7)
// Inherited fields come from Parent.init.
test.expectStr(c.label, "kid")
test.expect(c.n, 7)
test.expectStr(c.show(), "kid")

// Child's own fields take the type-driven defaults.
test.expect(c.extra_int, 0)
test.expectStr(c.extra_str, "")
test.expectBool(c.extra_flag, false)
test.expectBool(c.extra_opt.isNone, true)
test.expect(c.extra_arr.length, 0)

// Push goes through the synthesised empty placeholder.
c.extra_arr.push(7)
c.extra_arr.push(8)
test.expect(c.extra_arr.length, 2)
test.expect(c.extra_arr[0], 7)
test.expect(c.extra_arr[1], 8)

// Other fields stay writable post-construction.
c.extra_int = 99
c.extra_str = "hi"
test.expect(c.extra_int, 99)
test.expectStr(c.extra_str, "hi")
