// `return` / `break` / `continue` transfer control and yield no value,
// so using one as a sub-expression in a value-combining position
// (here a function argument) is rejected with a clean checker
// diagnostic. The checker types `return X` as the enclosing fn's
// return type so legitimate positions (fn tail, if/match branch, loop
// break value) type-check without a Never type; that pretense let
// `g(return 5)` slip past the checker and fail only as an internal MIR
// lowering error ("no coercion from () to i64"). An explicit guard now
// rejects it up front.
//
// The same guard covers binary/unary operands (`1 + (return 2)`),
// array/tuple/map elements (`[1, return 2]`), index, `some(...)`, and
// template parts.
//
// expect-error: cannot be used as a value here

fn g(n: i64): i64 { n + 1 }

fn f(): i64 {
    g(return 5)
}

console.log(f().toString())
