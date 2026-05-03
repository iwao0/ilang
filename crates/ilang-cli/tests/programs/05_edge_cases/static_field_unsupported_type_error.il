// Static field types are limited to i64 / f64 / bool for now.
// expect-error: not yet supported
class C {
    init() {}
    static name: string = "alice"
}
let _ = new C()
