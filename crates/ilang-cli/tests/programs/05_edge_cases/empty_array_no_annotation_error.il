// `let x = []` without a type annotation can't pick an element type
// — the checker requires either `let x: T[] = []` or a non-empty
// literal. Per syntax.md §3 "Variables".
//
// expect-error: cannot infer element type for empty array literal

let b = []
