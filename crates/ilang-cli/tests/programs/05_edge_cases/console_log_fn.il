// `console.log(<fn>)` parity between interp and JIT.
//
// Pre-fix the JIT printed the raw closure pointer (a meaningless
// integer) for first-class fn values, while the interpreter prints
// `<fn NAME>` for top-level fn references and `<fn>` for anonymous
// closures. Now the JIT registers each closure-trampoline's
// resolved address with a runtime fn-name map after
// `finalize_definitions` and dispatches through a print helper
// that matches.
//
// expect: <fn add>
// expect: <fn mul>
// expect: <fn add>
// expect: <fn>

fn add(x: i64): i64 { x + 1 }
fn mul(x: i64, y: i64): i64 { x * y }

console.log(add)
console.log(mul)
let f = add
console.log(f)

let c = fn(n: i64): i64 { n * 2 }
console.log(c)
