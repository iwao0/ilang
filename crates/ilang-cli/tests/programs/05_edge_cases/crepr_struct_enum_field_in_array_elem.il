// CRepr struct (inline enum field を持つ) を ilang の Array に入れて
// AssignIndex + LoadField の組み合わせを確認。 `arr[i].mode = ...`
// のような書き方が文法的に許される場合、 AssignField path に内側
// で落ちて inline enum slot として扱われるはず。 許されない場合は
// `let tmp = arr[i]; tmp.mode = ...` のパターンに置き換えて確認する。
//
// CRepr struct を ilang の Array に入れる shape 自体は珍しいが、
// 「条件分岐を網羅できる fixture」要件のため pin する。

use std.test as test

pub enum Mode: u16 {
    idle   = 0x0001
    active = 0x0002
}

@extern(C) {
    pub struct Slot {
        mode: Mode
        seq:  i32
    }
}

let s0 = new Slot()
s0.mode = Mode.idle
s0.seq = 1

let s1 = new Slot()
s1.mode = Mode.active
s1.seq = 2

// `Slot[]` (CRepr struct を要素とした配列) が許されるかをまず試す。
let arr: Slot[] = [s0, s1]
test.expect((arr[0].mode as u16) as i64, 0x01)
test.expect((arr[1].mode as u16) as i64, 0x02)
test.expect(arr[0].seq as i64, 1)
test.expect(arr[1].seq as i64, 2)
