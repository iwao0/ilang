// `make_tup()[0]` extracts element 0 and the rest of the tuple
// is unreferenced. Pre-fix the interp dropped the tuple via
// Rust's `Rc` and never released the unselected element's class
// instance — so the JIT printed `deinit 2` while the interp
// stayed silent. Mirror the array path: release the tuple after
// the cloned element is in hand.
//
// expect: deinit 2
// expect: after
// expect: 1
// expect: deinit 1

class T {
    n: i64
    init(n: i64) { this.n = n }
    deinit() { console.log("deinit " + this.n.toString()) }
}

fn make_tup(): (T, T) { (new T(1), new T(2)) }

let first = make_tup()[0]
console.log("after")
console.log(first.n)
