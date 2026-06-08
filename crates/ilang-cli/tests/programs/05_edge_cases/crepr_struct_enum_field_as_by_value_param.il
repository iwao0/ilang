// CRepr struct を fn の by-value 引数として受け、 fn 内部で
// inline enum field を read する。 caller 側が新規に作って渡す
// (fresh) のと、 既存 Local を渡す (borrowed) の両方を試す。
//
// extern C ABI で CRepr struct を by-value 渡しする path は SDL2
// のいくつかの API (e.g. SDL_RenderDrawRectF が SDL_FRect を取る)
// で実際に使われる。 ilang 側で fresh CRepr Object を渡しても
// caller-releases-fresh が `__release_object` を呼ばないこと
// (CRepr は is_arc_slot=false) を pin。

use std.test as test

pub enum Mode: u16 {
    idle   = 0x0001
    active = 0x0002
}

@extern(C) {
    pub struct Box {
        kind: Mode
    }
}

fn read_kind(b: Box): u16 {
    b.kind as u16
}

let local_box = new Box()
local_box.kind = Mode.active

// borrowed: 既存 Local をそのまま渡す。
test.expect(read_kind(local_box) as i64, 0x02)

// fresh: 関数呼び出し内で new した CRepr Object を渡す。
fn make_idle(): Box {
    let b = new Box()
    b.kind = Mode.idle
    b
}
test.expect(read_kind(make_idle()) as i64, 0x01)
