// `override` must reproduce the parent's signature exactly — a
// parameter-type drift is rejected at type check. Per syntax.md
// §7 "The override's signature must match the parent's exactly."
// expect-error: signature
class A {
    init() {}
    speak(n: i64): string { "A:" + n.toString() }
}
class B: A {
    init() { super() }
    // Wrong: parent takes i64, child takes string.
    override speak(s: string): string { "B:" + s }
}
let _ = new B()
