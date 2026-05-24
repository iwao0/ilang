// `@symbol("c_name")` lets the ilang-side fn name differ from the
// C symbol resolved at dlsym time. Useful when:
//   - The C name conflicts with an ilang keyword / module fn
//   - You want a more descriptive ilang name without renaming
//     the C source
//   - Two fns wrap the same C symbol with different ilang shapes
use test

@extern(C) {
    // The C symbol is `abs` but the ilang-callable name is `myAbs`.
    @lib("c") @symbol("abs") fn myAbs(x: i32): i32

    // Different ilang name, same C symbol — both resolve to libc::abs.
    @lib("c") @symbol("abs") fn intAbs(x: i32): i32

    // `@lib` declarations only resolve through the extern-C codegen
    // path, so they must be reached from inside another @extern(C)
    // block. Expose them via thin wrappers ordinary script code can
    // call.
    pub fn callMyAbs(x: i32): i32 { myAbs(x) }
    pub fn callIntAbs(x: i32): i32 { intAbs(x) }
}

test.expect(callMyAbs(-7 as i32) as i64, 7)
test.expect(callMyAbs(42 as i32) as i64, 42)

// Both wrappers reach the same underlying C fn.
test.expect(callIntAbs(-100 as i32) as i64, 100)
test.expect(callIntAbs(0 as i32) as i64, 0)
