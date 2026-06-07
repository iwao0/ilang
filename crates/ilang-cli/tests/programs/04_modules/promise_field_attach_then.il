// Attach a `.then` to a class field of type `Promise<T>` through
// the instance. The field carries the upstream alive across the
// access; the `.then` fires once the executor settles on the pool.
//
// expect: field:21

class Holder {
    p: Promise<i64>
    init(v: i64) {
        this.p = new Promise<i64>(fn(resolve: fn(i64), _reject: fn(string)) {
            resolve(v)
        })
    }
}

let h = new Holder(21)
h.p.then(fn(n: i64) {
    console.log("field:" + n.toString())
})

let _ = 0
