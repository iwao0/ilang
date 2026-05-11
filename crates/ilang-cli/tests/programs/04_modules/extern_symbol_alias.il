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
}

test.expect(myAbs(-7 as i32) as i64, 7)
test.expect(myAbs(42 as i32) as i64, 42)

// Both wrappers reach the same underlying C fn.
test.expect(intAbs(-100 as i32) as i64, 100)
test.expect(intAbs(0 as i32) as i64, 0)
