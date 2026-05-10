// `const` is a one-time-assign binding — any later assignment
// is a type error.
//
// expect-error: cannot assign to `z` — it is bound by `const`

fn f() {
    const z = 1
    z = 2
}
f()
