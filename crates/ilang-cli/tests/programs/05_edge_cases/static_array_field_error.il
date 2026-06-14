// A `static` class field's `LoadStatic` / `StoreStatic` codegen only
// handles single-word slot values (numeric / bool / string pointer).
// The checker used to also accept a dynamic array of primitives, so a
// `static arr: i64[]` compiled — but reading or reassigning it hit
// "static slot type" at codegen, and an init-then-use path SIGSEGV'd.
// The checker now rejects it up front with a clean diagnostic.

// expect-error: type i64[] not yet supported

class C {
    static arr: i64[] = [1, 2, 3]
}

console.log(C.arr.length.toString())
