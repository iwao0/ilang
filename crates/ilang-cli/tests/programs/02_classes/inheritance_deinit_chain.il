// `init` and `deinit` are per-class — the parent's `deinit` does
// NOT run automatically when a child instance dies. Children must
// call `super.deinit()` explicitly if they want the parent hook
// to fire. Per syntax.md §7 L987 ("`init` and `deinit` are
// per-class (not subject to override rules)").
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

// Without an explicit `super.deinit()`, only the child's hook fires.
class BNoSuper: A {
    init() { super() }
    deinit() { log.s.push("BNoSuper.deinit") }
}
{
    let _x = new BNoSuper()
}
test.expect(log.s.length, 1)
test.expectStr(log.s[0], "BNoSuper.deinit")

// Clear the log between sub-tests.
log.s = []

// With `super.deinit()`, the chain fires bottom-up.
class B: A {
    init() { super() }
    deinit() {
        log.s.push("B.deinit")
        super.deinit()
    }
}
class C: B {
    init() { super() }
    deinit() {
        log.s.push("C.deinit")
        super.deinit()
    }
}
{
    let _y = new C()
}
test.expect(log.s.length, 3)
test.expectStr(log.s[0], "C.deinit")
test.expectStr(log.s[1], "B.deinit")
test.expectStr(log.s[2], "A.deinit")
