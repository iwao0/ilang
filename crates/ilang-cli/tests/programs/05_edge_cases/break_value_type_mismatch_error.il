// All `break v` in the same `loop` must have compatible types.
// expect-error: expected
let _ = loop {
    if true {
        break 1
    } else {
        break "two"
    }
}
