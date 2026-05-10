// `break` is only meaningful inside a `while` / `loop` / `for`
// body. Top-level (or a fn body that isn't inside a loop) must
// reject it so users don't get a silent fall-through.
//
// expect-error: `break` used outside of a loop

break
