// Reading a missing key with the index form `m[k]` is a runtime panic —
// the unchecked read, documented as "missing key panics at runtime", with
// `m.get(k)` as the safe Optional-returning variant. The runtime helper
// `__map_get` used to `unwrap_or(0)`, silently handing back a zero / empty
// string (and, for object values, a null pointer that misbehaves
// downstream) instead of panicking. Now it panics like an out-of-bounds
// array index.

// expect-error: key not found in map

fn lookup(): i64 {
    let m: Map<string, i64> = {"a": 1, "b": 2}
    m["nope"]
}
console.log(lookup().toString())
