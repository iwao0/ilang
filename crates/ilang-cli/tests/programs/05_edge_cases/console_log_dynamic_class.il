// `console.log(<obj>)` prints the dynamic class name + dynamic
// field set, matching the interpreter's behaviour for upcasts.
//
// Pre-fix the JIT printed the static class — `let a: A = new B()`
// came out as `A {}` from the JIT and `B {}` from the interp; with
// fields it kept A's layout even when the receiver was a B with
// extra fields. Fixed by emitting one print fn per class and
// dispatching through a runtime helper that looks the obj's vtable
// up in a process-global registry (rust's `dyn Debug` style).
//
// expect: B { x: 1, y: 2 }
// expect: [P {}, Q {}]
// expect: some(Q {})
// expect: (P {}, Q {})
// expect: [[Q {}]]

class A {
    x: i64
    init() { this.x = 1 }
}
class B extends A {
    y: i64
    init() { super(); this.y = 2 }
}

let a: A = new B()
console.log(a)

class P {
    init() {}
}
class Q extends P {
    init() { super() }
}

let xs: P[] = [new P(), new Q()]
console.log(xs)

let o: P? = some(new Q())
console.log(o)

let pair: (P, P) = (new P(), new Q())
console.log(pair)

let nested: P[][] = [[new Q()]]
console.log(nested)
