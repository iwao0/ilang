// `console.log(map)` parity between interp and JIT.
//
// Pre-fix the JIT printed the raw header pointer; now it walks
// the map's entries via `ilang_jit_map_sorted_keys` (sorted by
// stringified key form, matching the interpreter's `Display`)
// and prints `{k: v, ...}` with recursive value formatting.
//
// expect: {a: 1, b: 2}
// expect: {a: [1, 2], b: [3]}
// expect: {1: one, 10: ten, 2: two}
// expect: {false: 2.5, true: 1.5}

let m1 = {"a": 1, "b": 2}
console.log(m1)

let m2 = new Map<string, i64[]>()
m2.set("a", [1, 2])
m2.set("b", [3])
console.log(m2)

let m3 = new Map<i64, string>()
m3.set(1, "one")
m3.set(2, "two")
m3.set(10, "ten")
console.log(m3)

let m4 = new Map<bool, f64>()
m4.set(true, 1.5)
m4.set(false, 2.5)
console.log(m4)
