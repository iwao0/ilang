// `arr[i].kind = Mode.active` — AssignIndex の対象が CRepr struct で、
// その内側 inline enum field を更新する path。 lower 経路は
// `let tmp = arr[i]; tmp.kind = ...` に等価か、 構文として直接の
// 連鎖を許すかは試して確認する。

use std.test as test

pub enum Mode: u16 {
    idle   = 0x0001
    active = 0x0002
}

@extern(C) {
    pub struct Slot {
        kind: Mode
        seq:  i32
    }
}

let s0 = new Slot()
s0.kind = Mode.idle
s0.seq = 0

let s1 = new Slot()
s1.kind = Mode.idle
s1.seq = 1

let arr: Slot[] = [s0, s1]

// 直接連鎖。 ilang が許せばこれで終了。
arr[0].kind = Mode.active
arr[1].seq = 99

test.expect((arr[0].kind as u16) as i64, 0x02)
test.expect((arr[1].kind as u16) as i64, 0x01)
test.expect(arr[1].seq as i64, 99)
