// fresh `Optional<string>` を `match` で受けて `some(s) { s }` を
// arm tail とするパターン。 838d2dc4 で `Binding::PatternBinding`
// の `needs_retain_on_tail = scrut_is_fresh` が有効になり、
// `lower_block_hinted` の tail-Var Retain が cascade と balance
// する。 inner の文字列が leak しないことを `liveStringCount` で
// 検証。
//
// mir-jit only — liveStringCount は AOT 配線なし。
// aot: skip
use std.test as test

fn make_opt(n: i64): string? { some("v" + n.toString()) }

fn through_match(n: i64): string {
    match make_opt(n) {
        some(s) { s }
        none    { "" }
    }
}

through_match(0)
let base = test.liveStringCount()
let i: i64 = 0
while i < 50 {
    let _ = through_match(i)
    i = i + 1
}
test.expectTrue(test.liveStringCount() - base < 32)
