// `none.unwrap()` is an explicit "I checked this" assertion. If the
// caller is wrong, the runtime panics rather than silently using
// garbage.
// expect-error: unwrap
let x: i64? = none
let _ = x.unwrap()
