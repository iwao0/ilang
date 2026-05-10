// Local `const x = expr` inside a fn body — same shape as
// `let`, but reassignment is rejected.
// expect: 7

fn run(): i64 {
    const a = 3
    const b = 4
    a + b
}

console.log(run())
