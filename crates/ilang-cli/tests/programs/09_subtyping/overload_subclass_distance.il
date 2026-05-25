// Overload resolution prefers the closer parent when an arg
// could go to multiple ancestors. Pre-fix `score_arg` didn't
// know about class subtype at all, so a `C: B: A` arg flowing
// into either `f(A)` or `f(B)` reported "no matching overload"
// — and even after a coarse "any subclass = score 5" tweak the
// two overloads tied. Inheritance-distance scoring picks `f(B)`.
use std.test as test
class A {
    init() {}
    label(): string { "A" }
}
class B: A {
    init() { super() }
    override label(): string { "B" }
}
class C: B {
    init() { super() }
    override label(): string { "C" }
}

fn show(x: A): string { "got A" }
fn show(x: B): string { "got B" }

test.expectStr(show(new A()), "got A")
test.expectStr(show(new B()), "got B")
test.expectStr(show(new C()), "got B")     // C's nearest match is B, not A
