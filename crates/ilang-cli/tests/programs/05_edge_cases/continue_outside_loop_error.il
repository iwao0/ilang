// `continue` is only meaningful inside a `while` / `loop` / `for`
// body. Using it outside any loop must be a compile-time error.
//
// expect-error: `continue` used outside of a loop

continue
