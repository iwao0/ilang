// `if` の各 arm から CRepr struct を返す fn を呼んで、 結果を
// 共通の binding に流す。 各 arm の sret 経路が phi node 越しに
// 整合し、 binding の Local として scope-exit で 1 回だけ free
// されるかを pin (二重 free / leak の検出)。
//
// mir-jit only — liveAllocCount は AOT 配線なし。
// aot: skip
use std.test as test

@extern(C) {
    pub struct Box {
        seq: i32
    }
}

fn make_box(n: i32): Box {
    let b = new Box()
    b.seq = n
    b
}

let base = test.liveAllocCount()
let i: i64 = 0
while i < 50 {
    let b = if i % 2 == 0 { make_box(i as i32) } else { make_box((i * 2) as i32) }
    let expected = if i % 2 == 0 { i } else { i * 2 }
    test.expect(b.seq as i64, expected)
    i = i + 1
}
test.expectTrue(test.liveAllocCount() - base < 8)
