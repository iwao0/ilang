// A closure that captures a class instance must call the
// instance's `deinit` when the closure itself is dropped.
//
// Pre-fix the interpreter's `release` recursed through Object /
// Array / Enum / Some / Map / Tuple but skipped `Value::Fn`, so a
// closure holding the only reference to a class value silently
// leaked the `deinit`. The JIT's per-wrapper closure_drop_fn
// always ran it. The harness caught the gap once a fixture
// compared deinit traces across backends.
//
// expect: init 42
// expect: 42
// expect: --- end
// expect: deinit 42

class T {
    n: i64
    init(n: i64) { this.n = n; console.log("init " + n.toString()) }
    deinit() { console.log("deinit " + this.n.toString()) }
}

fn make_closure(): fn(): i64 {
    let t = new T(42)
    fn(): i64 { t.n }
}

let f = make_closure()
console.log(f())
console.log("--- end")
