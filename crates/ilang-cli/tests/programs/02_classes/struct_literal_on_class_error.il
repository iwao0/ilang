// Struct-literal construction (`Name { f: v }`) is reserved for
// value types — top-level `struct` / `union`. Pointing it at a
// regular ARC class must be rejected, otherwise the class's `init`
// gets silently skipped and any invariants it enforces are lost.
//
// expect-error: struct-literal construction

class Counter {
    count: i64
    label: string
    init(c: i64, l: string) { this.count = c; this.label = l }
}

let _ = Counter { count: 10, label: "primary" }
