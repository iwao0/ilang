// Raw C pointer types (`*T` / `*const T`) are nameable only inside
// an `@extern(C) {}` block — using one in user code outside the
// block must surface as a type error so raw pointers stay
// encapsulated.
//
// expect-error: raw C pointer types are only nameable inside

let _x: *const i32 = 0 as *const i32
