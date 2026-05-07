// Tiny LCG (linear congruential generator). Single shared state at
// the module level so any caller — main, particle spawners, the
// background star field — pulls from the same deterministic stream.

let state: i64 = 0x5fe7c1a3

fn randNext(): i64 {
    state = state * 6364136223846793005 + 1442695040888963407
    state
}

/// Returns a float in [0, 1).
fn randf(): f64 {
    let r = randNext()
    let bits = r & 0x7fffffff
    (bits as f64) / 2147483648.0
}
