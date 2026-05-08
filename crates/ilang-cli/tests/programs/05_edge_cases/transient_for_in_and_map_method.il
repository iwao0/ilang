// Two more transient-temporary release sites the interpreter
// previously missed:
//
//   1. `for t in make_arr() { ... }` — the iterator is a fresh
//      array; once the loop exits, no binding survives. The JIT
//      releases the array on loop exit so each element's
//      `deinit` fires.
//
//   2. `make_map().keys()` (or any built-in Map method on a
//      fresh receiver) — the receiver has no surviving binding
//      after the call. Match the JIT's fresh-receiver release.
//
// expect: 1
// expect: 2
// expect: 3
// expect: deinit 1
// expect: deinit 2
// expect: deinit 3
// expect: after for
// expect: deinit 10
// expect: a

class T {
    n: i64
    init(n: i64) { this.n = n }
    deinit() { console.log("deinit " + this.n.toString()) }
}

fn make_arr(): T[] { [new T(1), new T(2), new T(3)] }
for t in make_arr() {
    console.log(t.n)
}
console.log("after for")

fn make_map(): Map<string, T> {
    let m = new Map<string, T>()
    m.set("a", new T(10))
    m
}
for k in make_map().keys() {
    console.log(k)
}
