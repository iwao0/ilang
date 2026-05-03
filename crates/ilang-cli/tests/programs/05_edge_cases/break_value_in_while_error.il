// `break v` is only allowed in `loop`, not `while` / `for`.
// expect-error: break value
let i = 0
let n = while i < 10 {
    break i
}
