// `@optional` on an interface method marks it as not-required-
// to-implement. Implementing classes may omit such methods
// without failing the type-checker's conformance pass; the
// interface slot still exists for any class that *does* provide
// a body, so virtual dispatch on those classes works as usual.
//
// This is the mechanism that lets ilang model Objective-C
// `@optional` protocol methods (NSApplicationDelegate etc.) —
// the ObjC runtime probes `respondsToSelector:` for each
// optional method at the call site, so missing implementations
// are fine.
// expect: eager-hello
// expect: eager-bye
// expect: lazy-hello

interface Greet {
    hello(): string
    @optional goodbye(): string
}

class Eager: Greet {
    pub init() {}
    pub hello(): string { "eager-hello" }
    pub goodbye(): string { "eager-bye" }
}

// Lazy skips goodbye — still legal because of `@optional`.
class Lazy: Greet {
    pub init() {}
    pub hello(): string { "lazy-hello" }
}

fn greet(g: Greet) {
    console.log(g.hello())
}

let e = new Eager()
greet(e)
console.log(e.goodbye())

let l = new Lazy()
greet(l)
