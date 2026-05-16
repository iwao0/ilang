// Generic enum constructor inside a generic fn body — previously
// failed at MIR ("unknown type: T") because monomorphize_enums
// only seeded EnumCtors from non-generic fn bodies, and
// monomorphize_fns didn't mangle EnumCtor.enum_name when
// specializing a generic body per call site.
//
// Now `monomorphize_fns` post-rewrites EnumCtor refs in
// specialized bodies (against the typechecker's
// enum_ctor_type_args side table), and the main pipeline
// re-attaches generic templates before the second class/enum
// passes so the freshly-surfaced concrete refs synthesize.
//
// expect: 42
// expect: hi
// expect: none

enum MyOpt<T> {
    some: (T)
    nope
}

fn wrap<T>(v: T): MyOpt<T> {
    MyOpt.some(v)
}

fn show_i64(o: MyOpt<i64>) {
    match o {
        some(v) { console.log(v.toString()) }
        nope { console.log("none") }
    }
}

fn show_str(o: MyOpt<string>) {
    match o {
        some(s) { console.log(s) }
        nope { console.log("none") }
    }
}

let a: MyOpt<i64> = wrap(42)
let b: MyOpt<string> = wrap("hi")
let c: MyOpt<i64> = MyOpt.nope
show_i64(a)
show_str(b)
show_i64(c)
let _ = 0
