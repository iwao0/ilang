// Deinits chain automatically, Swift-style: when an instance dies,
// the most derived class's `deinit` fires first, then each
// ancestor's in turn. There is no `super.deinit()` — explicit deinit
// calls are rejected (see inheritance_super_deinit_error.il) — and
// classes without their own `deinit` are skipped without breaking
// the chain.
use std.test as test

class Log {
    s: string[]
    init() { this.s = [] }
}
let log = new Log()

class A {
    init() {}
    deinit() { log.s.push("A.deinit") }
}

// A two-class chain: child first, then parent — automatically.
class B: A {
    init() { super() }
    deinit() { log.s.push("B.deinit") }
}
{
    let _x = new B()
}
test.expect(log.s.length, 2)
test.expectStr(log.s[0], "B.deinit")
test.expectStr(log.s[1], "A.deinit")

// Clear the log between sub-tests.
log.s = []

// Three levels with a deinit-less middle class: the chain passes
// through Mid untouched.
class Mid: A {
    init() { super() }
}
class C: Mid {
    init() { super() }
    deinit() { log.s.push("C.deinit") }
}
{
    let _y = new C()
}
test.expect(log.s.length, 2)
test.expectStr(log.s[0], "C.deinit")
test.expectStr(log.s[1], "A.deinit")
