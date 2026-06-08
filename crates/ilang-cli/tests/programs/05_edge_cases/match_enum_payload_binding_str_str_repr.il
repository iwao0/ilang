// `: string` repr の enum を CRepr 親じゃない (= 通常 ARC heap)
// として持つ class field を match で分解する。 string repr enum
// は CReprEnum 化対象外 (`is_inline_repr_enum` で int_repr = false)
// なので、 通常の Enum heap cell path を踏むはず。
//
// 副次的に: arm `boxed { b: x } { x }` で string-repr enum の
// payload を tail で返す形 (47d4f979 の borrowed scrut 版に対する
// fresh scrut 版)。

use std.test as test

pub enum Hint: string {
    fast = "fast"
    slow = "slow"
    bal  = "balanced"
}

class Spec {
    hint: Hint
    init(h: Hint) { this.hint = h }
}

fn make_spec(): Spec { new Spec(Hint.fast) }

let s = make_spec()
let label = match s.hint {
    fast { "fast" }
    slow { "slow" }
    bal  { "balanced" }
}
test.expectStr(label, "fast")

// 別 variant でも分岐できることを確認。
s.hint = Hint.bal
test.expectStr(match s.hint {
    fast { "fast" }
    slow { "slow" }
    bal  { "balanced" }
}, "balanced")
