// Tiny xorshift64 PRNG. Single shared state at the module level so
// any caller — main, particle spawners, the background star field —
// pulls from the same deterministic stream. We use xorshift instead
// of an LCG so the arithmetic stays inside i64 without overflow.

pub let state: i64 = 0x5fe7c1a3

pub fn randNext(): i64 {
    state = state ^ (state << 13)
    state = state ^ (state >> 7)
    state = state ^ (state << 17)
    state
}

/// Returns a float in [0, 1).
pub fn randf(): f64 {
    let r = randNext()
    let bits = r & 0x7fffffff
    (bits as f64) / 2147483648.0
}
