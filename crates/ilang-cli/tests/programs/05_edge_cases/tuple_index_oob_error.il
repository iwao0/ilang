// Tuple indexing must stay within the declared arity. The check
// happens at type-check time because the index is a const literal,
// so the failure is reported before any execution.
// expect-error: tuple index 3 out of bounds
let p = (1, 2, 3)
let _ = p[3]
