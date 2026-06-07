// A child class can overload methods the *parent doesn't* declare
// (covered by `subclass_overload.il`), but it can't introduce a
// new overload sharing a name with a parent method — `override`
// is single-signature, and a bare same-name method without
// `override` is rejected as "hides a parent method". The pair
// (override variant + a sibling non-override overload) therefore
// always trips the hide-without-override rule.
//
// expect-error: hides a parent method

class Parent {
    init() {}
    greet(): string { "parent" }
}
class Child: Parent {
    init() { super() }
    override greet(): string { "child0" }
    greet(n: i64): string { "child" + n.toString() }
}
