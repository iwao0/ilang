// `async fn` inside a `class`. The desugar lifts the state class
// and the poll fn out next to the containing class as top-level
// items; the class keeps the original-named method as a thin
// wrapper that builds the state (with `this` stored as
// `state.__this`) and kicks the first poll. References to `this`
// inside the user's method body get rewritten to `state.__this`
// in the lifted poll fn.
//
// Limitation: state-class fields with heap types (Object, Array,
// Map, etc.) have no safe init-time default. Async fns can hold
// primitive `let` bindings but heap-typed bindings need the
// Optional-wrapping refactor that's a follow-up.
//
// expect: 17

class Counter {
    pub n: i64

    pub init(n: i64) {
        this.n = n
    }

    pub async incr(p: Promise<i64>): i64 {
        let v = await p
        this.n + v
    }
}

let c = new Counter(10)

let _ = c.incr(Promise.resolve(7)).then(fn(r: i64) {
    console.log(r.toString())
})

let _ = 0
