// Class-level `const name: T = …` declares an immutable static
// slot. Reassigning it through `ClassName.name = …` must be a
// compile-time error — `static name: T = …` (without `const`)
// is the mutable counterpart already covered by other fixtures.
//
// expect-error: cannot assign to const static field "C"."max"

class C {
    const max: i64 = 10
}

C.max = 99
