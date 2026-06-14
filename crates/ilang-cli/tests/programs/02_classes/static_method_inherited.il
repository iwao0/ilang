// A static method is reachable by a subclass's own name (it resolves to
// the nearest declaration up the chain), and a subclass may shadow it
// with its own. A static method can also call another static method and
// read a static field — and that works when reached through a subclass
// (which ties into static-field inheritance). `static_methods.il` only
// covers factory statics on a single class; this pins the inheritance.
//
// expect: ok
use std.test as test

class Base {
    static base: i64 = 100
    init() {}
    static who(): string { "base" }
    static helper(): i64 { 42 }
    static useHelper(): i64 { Base.helper() + Base.base }
}
class Mid: Base { init() { super() } }
class Leaf: Mid { init() { super() } static who(): string { "leaf" } }

// Static method inherited by the subclass name.
test.expectStr(Mid.who(), "base")
test.expectStr(Base.who(), "base")
// Subclass shadow resolves to its own.
test.expectStr(Leaf.who(), "leaf")
// Static method calling a static method and reading a static field.
test.expect(Base.useHelper(), 142)
// Reachable through a subclass name (with the inherited static field).
test.expect(Mid.useHelper(), 142)

console.log("ok")
