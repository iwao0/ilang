// Sibling module for `struct_lit_in_method_via_dep.il`. Declares
// a `pub struct` + a `pub class` whose method constructs the
// struct via a literal. Standalone run is a no-op (no top-level
// expressions, no expect lines → empty stdout).
//
// Regression-test target: loader's `prefix_expr` used to skip
// qualifying `StructLit.class`, so once this module was loaded
// as a dep its bare `Builder.make()` body looked up `Pair`
// instead of `lib_struct_lit_in_method.Pair`, and the
// type-checker reported `undefined class "Pair"`.

@extern(C, "nonexistent") {
    pub struct Pair {
        a: i32
        b: i32
    }

    pub class Builder {
        pub init() {}
        pub make(): i32 {
            let p = Pair { a: 30, b: 12 }
            p.a + p.b
        }
    }
}
