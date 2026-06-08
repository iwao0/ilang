// `enum E { v: { b: Box } }` の struct-shaped payload binding を
// arm tail で返すパターン (47d4f979 fixture の borrowed scrut
// 版に対して、 ここでは fresh scrut)。 838d2dc4 の
// `lower_match_enum` の struct branch で
// `env.bind_pattern(..., scrut_is_fresh)` が正しく動くか確認。
//
// mir-jit only — liveAllocCount は AOT 配線なし。
// aot: skip
use std.test as test

class Box {
    n: i64
    init(n: i64) { this.n = n }
}

enum Field {
    bare
    boxed: { b: Box }
}

fn make_field(): Field { Field.boxed { b: new Box(99) } }

fn through(): Box {
    match make_field() {
        bare                { new Box(0) }
        boxed { b: inner }  { inner }
    }
}

through()
let base = test.liveAllocCount()
let i: i64 = 0
while i < 50 {
    let _ = through()
    i = i + 1
}
test.expect(test.liveAllocCount() - base, 0)
