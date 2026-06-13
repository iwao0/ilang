// A generic enum ctor / generic fn call whose type parameter can't be
// pinned from its args AND has no expected type from context leaves the
// param `Any`. That used to abort lowering with the opaque "Type::Any
// (variadic builtins)". The checker now reports it up front with a source
// span and an annotation hint.
//
// Here `Result.ok(5)` fixes T = i64 but leaves E unresolved (an `ok` value
// says nothing about the error type), and the binding has no annotation to
// supply it. The fix is to annotate: `let r: Result<i64, string> =
// Result.ok(5)`.

// expect-error: cannot infer the type parameter(s) of `Result`

fn main(): i64 {
    let r = Result.ok(5)
    match r {
        ok(v) { v }
        err(s) { 0 - 1 }
    }
}
console.log(main().toString())
