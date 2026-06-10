// Deinits chain automatically (derived first, then each ancestor),
// so an explicit `super.deinit()` would run the parent's hook a
// second time — rejected at type-check like every other explicit
// deinit call.
//
// expect-error: `deinit` cannot be called explicitly
class A {
    init() {}
    deinit() {}
}
class B: A {
    init() { super() }
    deinit() {
        super.deinit()
    }
}
let b = new B()
