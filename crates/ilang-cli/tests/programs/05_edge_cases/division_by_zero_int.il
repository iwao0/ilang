// Integer division (and modulo) by zero is a runtime panic.
// expect-error: division by zero
let n = 10
let _ = n / 0
