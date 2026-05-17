// `@objc interface` declarations inside `@extern(ObjC) { ... }`
// register as ordinary ilang interfaces, with extra Objective-C
// selector metadata that downstream passes can consume.
//
// At the type-checker level the interface behaves exactly like a
// regular interface: classes implement it via `: Iface` in the
// base list, virtual dispatch on the interface-typed variable
// reaches the concrete method body, `@optional` methods may be
// omitted by the implementer.
//
// (Wiring the implementing class through `objc_msgSend` instead
// of ilang's normal virtual table is a separate downstream pass
// and not exercised here; this fixture only proves the parser /
// type-checker pieces of the @objc-interface story.)
// expect: hello A
// expect: hello B
// expect: lazy-A skips the optional

@extern(ObjC) {
    @objc pub interface Greeter {
        // Auto-derived selector (= `hello:`) — the parser appends
        // ':' × paramCount when @objc("...") is absent.
        hello(name: i64)
        // Explicit selector. Same shape but spelled out.
        @objc("hello:withSuffix:")
        @optional helloWithSuffix(name: i64, suffix: i64)
    }
}

class A: Greeter {
    pub init() {}
    pub hello(name: i64) {
        console.log("hello A")
    }
    pub helloWithSuffix(name: i64, suffix: i64) {
        console.log("hello A suffix")
    }
}

// B skips the @optional helloWithSuffix — still implements the
// interface because the conformance check honours @optional.
class B: Greeter {
    pub init() {}
    pub hello(name: i64) {
        console.log("hello B")
    }
}

let a: Greeter = new A()
let b: Greeter = new B()
a.hello(0)
b.hello(0)
console.log("lazy-A skips the optional")
