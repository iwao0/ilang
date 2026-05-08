// `break v` from a loop body skips `lower_block_value`'s normal
// release pass — anything declared inside the loop body has to
// be released explicitly before the jump, or its `deinit`
// silently leaks. Pre-fix the JIT printed `10` without ever
// running `deinit 10`; the interp got it right because its
// block-exit unwinding picks up locals on the early break path
// too.
//
// Snapshot `lc.env.bindings` when each loop frame is pushed;
// when `break` lowers, walk the bindings introduced (or
// shadowed) since the frame's entry and emit the matching
// `release` calls before jumping to the after-block.
//
// expect: deinit 10
// expect: 10

class T {
    n: i64
    init(n: i64) { this.n = n }
    deinit() { console.log("deinit " + this.n.toString()) }
}

let v = loop {
    let temp = new T(10)
    break temp.n
}
console.log(v)
