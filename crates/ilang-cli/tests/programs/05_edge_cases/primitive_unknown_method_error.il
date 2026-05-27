// Numeric / bool primitives have a tiny built-in method set
// (`toString`, plus `isFinite` / `isNaN` on floats). Typos like
// `isNan` (JS convention is `isNaN` — both Ns capitalised) used
// to fall through the type checker's primitive arms and hit
// `expect_object`, surfacing a misleading "expected <object>, got
// f64" instead of naming the actual missing method.
//
// expect-error: class "f64" has no method "isNan"

fn main(): i64 {
    let x = 1.0
    if x.isNan() { 1 } else { 0 }
}
