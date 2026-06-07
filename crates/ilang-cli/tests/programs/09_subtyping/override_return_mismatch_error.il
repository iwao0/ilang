// `override` must reproduce the parent's signature exactly — a
// return-type drift is rejected at type check, alongside the
// parameter-type drift case in `override_signature_mismatch_error`.
// Per syntax.md §7 "The override's signature must match the
// parent's exactly."
// expect-error: signature
class A {
    init() {}
    speak(): string { "A" }
}
class B: A {
    init() { super() }
    // Wrong: parent returns string, child returns i64.
    override speak(): i64 { 0 }
}
let _ = new B()
