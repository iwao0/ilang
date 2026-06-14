// An `@extern(C)` struct lays its fields out inline (C layout, no ARC),
// so a heap field — a non-repr-C object, a non-last dynamic array, a
// tuple, an optional — has no owner and either leaks or crashes. The
// repr-C CLASS path rejects such fields, but `@extern(C) { struct ... }`
// (an `ExternCItem::Struct`) skipped field-type validation entirely:
// `struct S { b: Box }` compiled, and the `Box` was never released
// (silent leak; a dynamic non-last array field panicked). The struct
// body check now runs the same `check_repr_c_struct_field` as the class
// path (which additionally allows the inline-safe `repr` enum that
// `@extern(C)` structs use).

// expect-error: type Box not supported

class Box {
    pub n: i64
    init(n: i64) { this.n = n }
    deinit() {}
}

@extern(C) {
    struct S {
        x: u32
        b: Box
    }
}

let s = new S()
console.log("unreachable")
