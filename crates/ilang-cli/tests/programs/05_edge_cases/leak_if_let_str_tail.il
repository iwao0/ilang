// `if let some(s) = <fresh>` の some 分岐で binding を tail として
// 返す Str バリエーション。 838d2dc4 で `lower_if_let` も
// PatternBinding 経路に揃ったので、 string の rc が cascade と
// balance するかを `liveStringCount` で確認。
//
// mir-jit only — liveStringCount は AOT 配線なし。
// aot: skip
use std.test as test

fn make_opt(n: i64): string? { some("v" + n.toString()) }

fn through_if_let(n: i64): string {
    if let some(s) = make_opt(n) { s } else { "fallback" }
}

through_if_let(0)
let base = test.liveStringCount()
let i: i64 = 0
while i < 50 {
    let _ = through_if_let(i)
    i = i + 1
}
test.expectTrue(test.liveStringCount() - base < 32)
