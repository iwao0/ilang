// A class instance that's produced as a temporary and never
// bound — `make().field`, `f(make())`, `make()` as an expr-stmt —
// should fire `deinit` as soon as it leaves the surrounding
// expression. Pre-fix the interpreter dropped these via Rust's
// `Rc` and never invoked user `deinit`, so the JIT (which always
// emits a release for the fresh receiver / the discarded fn arg)
// reported `deinit` while the interp stayed silent.
//
// Three sites are exercised below:
//   1. expr-stmt: `make()` discarded immediately.
//   2. field access: `make().n`.
//   3. fn arg: `use_t(make())`.
//
// expect: deinit 1
// expect: after stmt
// expect: deinit 1
// expect: n=1
// expect: after field
// expect: deinit 1
// expect: r=1
// expect: after arg

class T {
    n: i64
    init(n: i64) { this.n = n }
    deinit() { console.log("deinit " + this.n.toString()) }
}

fn make(): T { new T(1) }
fn use_t(t: T): i64 { t.n }

make()
console.log("after stmt")

let n = make().n
console.log("n=" + n.toString())
console.log("after field")

let r = use_t(make())
console.log("r=" + r.toString())
console.log("after arg")
