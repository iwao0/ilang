// Round 159 rejected diverging exprs (`return` / `break` / `continue`)
// in value-combining positions (call args, operands, elements), but
// missed the ASSIGNMENT right-hand sides — `x = return 5`,
// `c.field = return 5`, `a[0] = return 5` — and the cast operand
// `(return 5) as T`. Those slipped past the checker and **panicked the
// MIR codegen** (a Rust panic in locals.rs / objects.rs / array.rs),
// which is worse than the value-join verifier errors. The same guard
// now covers assignment RHS and cast operands, so they fail with a
// clean checker diagnostic instead.
//
// expect-error: cannot be used as a value here

fn f(): i64 {
    let x = 0
    x = return 5
    x
}

console.log(f().toString())
