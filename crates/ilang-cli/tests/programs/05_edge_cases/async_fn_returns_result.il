// An `async fn` whose declared return type is a generic enum — e.g.
// `async fn getR(): Result<Box, string>` — desugars to a poll fn that
// settles the result promise via `Promise.$promise.settleResolve(p,
// Result.ok(..))`, a GENERIC static method (`settleResolve<T>(p:
// Promise<T>, v: T)`). The value ctor `Result.ok(..)` thus lands as a
// generic-method argument. `resolve_method_call` validated such args but
// never refined their enum-ctor type params (unlike the generic-fn and
// `check_args` paths), so `Result.ok(new Box(5))` kept E = Any and the
// monomorphizer aborted (now the 第63弾 diagnostic would flag it). Result:
// an async fn returning a `Result` needed an explicit inner annotation.
//
// Fix: `resolve_method_call` now refines each generic-method arg against
// its substituted param type. The promise's element type
// (`Result<Box, string>`) flows into `settleResolve`'s `v: T`, refining
// the ctor — so the async fn just works. Heap payloads stay ARC-exact
// across the await boundary.

// expect: total=300 deinits=50

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

// Returns a Result — no inner annotation; refined via settleResolve.
async fn getR(ok: bool): Result<Box, string> {
    if ok {
        Result.ok(new Box(5))
    } else {
        Result.err("no")
    }
}

// Drive 50 ok + 50 err awaits sequentially. Each ok makes a Box (freed
// after its match); each err makes none. By the time the driver settles,
// all 50 boxes are freed.
async fn driver(): i64 {
    let total = 0
    let i = 0
    while i < 50 {
        let r = await getR(true)
        total = total + match r {
            ok(b) { b.n }                    // 5
            err(s) { 0 - 1 }
        }
        let r2 = await getR(false)
        total = total + match r2 {
            ok(b) { b.n }
            err(s) { 1 }                     // err path: +1
        }
        i = i + 1
    }
    total                                    // (5 + 1) * 50 = 300
}

let p = driver()
let done = p.then(fn(v: i64) {
    console.log("total=" + v.toString() + " deinits=" + deinits[0].toString())
})
