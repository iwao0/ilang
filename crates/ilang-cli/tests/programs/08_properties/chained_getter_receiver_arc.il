// A property getter read whose receiver is itself a field / property
// access — `o.inner.boxed`, the everyday `a.b.getter` chain — leaked
// two ways:
//   (1) the leaf getter's owned +1 was dropped only when the receiver
//       was a plain Var / `this` / class-name; a chained receiver fell
//       to the borrow default, so the fresh value the getter returns
//       was never released (field_is_property_access didn't resolve
//       chained receivers), and
//   (2) the property-getter dispatch never released a FRESH receiver,
//       so the intermediate getter result (`o.inner`) leaked once per
//       read — even when the leaf was a plain field or scalar.
// Both are fixed: the leaf is classified owned via the receiver's
// resolved static class, and the getter call drops a fresh receiver
// like the `.length` path does.
//
// expect: 42
// expect: churn inner=100 box=100

let di: i64[] = [0]
let db: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { db[0] = db[0] + 1 }
}
class Inner {
    tag: i64
    init() { this.tag = 42 }
    deinit() { di[0] = di[0] + 1 }
    get boxed(): Box { new Box(this.tag) }   // fresh leaf each read
}
class Outer {
    _inner: Inner
    init() { this._inner = new Inner() }
    get inner(): Inner { this._inner }        // borrowed intermediate
}

// `o.inner` is a getter result (fresh, +1); `.boxed` is a getter on it
// returning a fresh Box. Both transients must be reclaimed.
fn run(): i64 {
    let o = new Outer()
    o.inner.boxed.n
}

console.log(run().toString())                  // 42

// Each run mints exactly one Inner and one Box (the boxed leaf) that
// must die — deinit exactly 100 over 100 runs, no leak / over-release.
let bi = di[0]
let bb = db[0]
let i = 0
while i < 100 { let _ = run(); i = i + 1 }
console.log("churn inner=" + (di[0] - bi).toString()
    + " box=" + (db[0] - bb).toString())
