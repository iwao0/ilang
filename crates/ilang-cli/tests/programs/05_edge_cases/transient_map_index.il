// `make_map()[k]` extracts one entry's value; the unselected
// entries had no other owner. Pre-fix the interp dropped the
// map via Rust's `Rc` and never released the leftover values'
// class instances, so the JIT printed `deinit 22` while the
// interp stayed silent. Mirror the array / tuple paths and
// release the map after extraction when `obj.kind` is not
// aliased.
//
// expect: deinit 22
// expect: 11
// expect: deinit 11

class T {
    n: i64
    init(n: i64) { this.n = n }
    deinit() { console.log("deinit " + this.n.toString()) }
}

fn make_map(): Map<string, T> {
    let m = new Map<string, T>()
    m.set("a", new T(11))
    m.set("b", new T(22))
    m
}

let v = make_map()["a"]
console.log(v.n)
