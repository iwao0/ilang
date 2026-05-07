// Map methods on a fresh receiver release the map after dispatch
// (126f84c + 33e12d5 for the string-key release).
use test

fn make_map(): Map<i64, i64> {
    let m = new Map<i64, i64>()
    m.set(1, 100)
    m.set(2, 200)
    m
}

let total: i64 = 0
for _ in 0..500 {
    if let some(v) = make_map().get(1) {
        total = total + v
    }
}
test.expect(total, 100 * 500)

// Fresh string key flowing into Map<string, V> — runtime clones
// the content, the JIT now releases the input rc.
let prefix: string = "k_"
let m = new Map<string, i64>()
m.set("k_a", 1)
let hits: i64 = 0
for _ in 0..500 {
    if m.has(prefix + "a") { hits = hits + 1 }
}
test.expect(hits, 500)
