// A `break <diverging-expr>` whose VALUE transfers control — `break
// (return v)`, or `break (if ... else ...)` where every path returns —
// must not pin the loop's exit-block param to the diverging value's
// placeholder `()` type. A genuine `break v` then mismatched it and
// MIR codegen aborted with a Cranelift verifier error ("mismatched
// argument count for jump") instead of compiling.
//
// The checker already skips diverging breaks from the break-value join
// (see loop_break_todo_diverges.il). This pins the MIR lowering side:
// a diverging break lowers its value for the side effect, then closes
// the now-dead block as Unreachable — attaching no exit param and
// jumping with no value. The exit param type is set solely by the
// genuine `break v`.
//
// expect: 8
// expect: 20
// expect: early=1

// `break (return v)` mixed with a real break
fn via_return(c: bool): i64 {
    let r = loop {
        if c { break (return 1) }
        break 8
    }
    r
}

// `break (if all-paths-return)` mixed with a real break
fn via_if(c: bool): i64 {
    let r = loop {
        if c { break (if c { return 1 } else { return 2 }) }
        break 20
    }
    r
}

// taking the diverging path actually returns from the fn
fn early(c: bool): i64 {
    let r = loop {
        if c { break (return 1) }
        break 99
    }
    r
}

console.log(via_return(false).toString())          // 8
console.log(via_if(false).toString())              // 20
console.log("early=" + early(true).toString())     // diverging path → return 1
