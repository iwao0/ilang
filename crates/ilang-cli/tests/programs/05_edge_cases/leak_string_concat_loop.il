// Per-iteration `"prefix" + n.toString()` — the intermediate
// `toString()` result and the concat output must both be released
// by the end of each iteration so a long loop doesn't grow the
// rc-tracked string registry without bound.
//
// Pre-fix (b94e6da) the lower_binary StrConcat path didn't
// release fresh-Str operands after the BinOp, so each loop body
// leaked the `toString()` temp into `STRING_REGISTRY`. breakout's
// per-frame HUD strings (`"FPS: " + intToStr(fps)` etc.) drove
// ~30 leaks/sec even with the rest of the pipeline correct.
//

use test

let baseline = test.liveStringCount()

let i: i64 = 0
while i < 1000 {
    let s = "x" + i.toString()
    // Discard `s` immediately — block-local Local with scope-exit
    // release. The interesting question is whether the toString
    // temp also gets released.
    let _ = s
    i = i + 1
}

let after = test.liveStringCount()
let grew = after - baseline
// Allow slack for any registry entry that genuinely outlives the
// loop (toString / format internals) but reject runaway growth.
test.expectTrue(grew < 50)
