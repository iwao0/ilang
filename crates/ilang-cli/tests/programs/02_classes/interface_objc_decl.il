// `@objc interface` declarations inside `@extern(ObjC) { ... }`
// register as ordinary ilang interfaces, with extra Objective-C
// selector metadata that downstream passes consume:
//
//   * The type-checker accepts the interface as a base in any
//     `class C: Iface { … }` declaration and runs the same
//     conformance check it does for plain interfaces; ``
//     methods may be skipped.
//
//   * The loader's auto-lift pass spots top-level classes naming
//     an `@objc interface` as a base and wraps them in a
//     synthesised `@extern(ObjC) { @objc class … }` block so the
//     resulting class is registered with the ObjC runtime, with
//     selectors inherited from the interface and `alloc` / `init`
//     auto-injected.
//
// This fixture only proves the parser / type-checker pieces of
// that story; the runtime dispatch path is exercised by the
// example-driven smoke tests under `bindings/cocoa/` since it
// needs Objective-C runtime symbols.
//
// jit: skip

@extern(ObjC) {
    @objc class NSObject {
        @objc("release") release?()
    }
    @objc pub interface Greeter {
        // Auto-derived selector (= `hello:`) — the parser appends
        // ':' × paramCount when @objc("...") is absent.
        hello(name: i64)
        // Explicit selector. Same shape but spelled out.
        @objc("hello:withSuffix:")
        helloWithSuffix?(name: i64, suffix: i64)
    }
}

// A satisfies both interface methods (optional included). The
// auto-lift turns it into an @objc class registered with the ObjC
// runtime; selectors come from `Greeter`.
class A: Greeter {
    pub hello(name: i64) {
        console.log("hello A")
    }
    pub helloWithSuffix(name: i64, suffix: i64) {
        console.log("hello A suffix")
    }
}

// B skips the helloWithSuffix — still legal because
// `@optional` excuses it from the conformance check.
class B: Greeter {
    pub hello?(name: i64) {
        console.log("hello B")
    }
}
