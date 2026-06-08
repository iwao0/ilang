// fresh `Optional<T[]>` を `match` で受けて `some(xs) { xs }` を
// arm tail とするパターン。 838d2dc4 の PatternBinding 経路で
// inner array の rc が cascade と balance することを
// `liveAllocCount` で確認 (Box が array の要素として乗る)。
//
// mir-jit only — liveAllocCount は AOT 配線なし。
// aot: skip
use std.test as test

class Box {
    n: i64
    init(n: i64) { this.n = n }
}

fn make_opt(): Box[]? { some([new Box(1), new Box(2)]) }

fn empty_arr(): Box[] { [new Box(0)] }

fn through_match(): Box[] {
    match make_opt() {
        some(xs) { xs }
        none    { empty_arr() }
    }
}

through_match()
let base = test.liveAllocCount()
let i: i64 = 0
while i < 50 {
    let _ = through_match()
    i = i + 1
}
test.expect(test.liveAllocCount() - base, 0)
