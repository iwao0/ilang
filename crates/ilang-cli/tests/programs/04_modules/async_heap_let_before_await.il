// Heap-typed let binding introduced BEFORE the first await. Under
// the legacy class-based lowering this failed because the state
// class's init has to assign every field, and there's no zero-value
// for a class-typed slot. The enum-variant lowering (state_machine)
// fixes it: V_0's payload doesn't include the post-await binding,
// and `c` is built fresh inside the V_0 arm.
//
// expect: 17

class Counter {
    pub n: i64
    pub init(n: i64) { this.n = n }
}

async fn run(p: Promise<i64>): i64 {
    let c = new Counter(10)
    let v = await p
    c.n + v
}

let _ = run(Promise.resolve(7)).then(fn(n: i64) {
    console.log(n.toString())
})
let _ = 0
