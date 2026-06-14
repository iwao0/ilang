// An `@extern(C) union` shares offset-0 storage across all fields, so a
// heap field (object / string / array-of-heap) dangles the moment a
// sibling field overwrites it — `u.b = new Box(); u.a = 999; u.b.n`
// dereferences `999` and SIGSEGVs. `check_class` rejects such fields,
// but `@extern(C) { union ... }` (an `ExternCItem::Union`) skipped the
// body validation entirely, so this compiled and crashed at runtime.
// The union body check now runs the shared `validate_union`.

// expect-error: Heap types and nested aggregates aren't safe under shared storage

class Box {
    pub n: i64
    init(n: i64) { this.n = n }
}

@extern(C) {
    union U {
        a: u64
        b: Box
    }
}

let u = new U()
console.log("unreachable")
