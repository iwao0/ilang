// fn の戻り値で CRepr struct を渡し、 caller 側で inline enum field
// を read する path。 SDL2 で AudioSpec を作って caller に返す
// パターン (sdl_audio.il の `init` 内 `new AudioSpec()` の延長) を
// 最小化したもの。
//
// caller-releases-fresh が走るのは SSA 値の MirTy = `Object(CRepr)`
// 経由で、 CRepr Object は `is_arc_slot` で false なので
// `__release_object` は呼ばれない。 戻り値の `b` で inline enum
// field を再度読むときに、 `LoadField` codegen の static probe が
// 正しく動くことを確認する。

use std.test as test

pub enum Mode: u16 {
    idle   = 0x0001
    active = 0x0002
}

@extern(C) {
    pub struct Box {
        kind: Mode
        n:    i32
    }
}

fn make_active(): Box {
    let b = new Box()
    b.kind = Mode.active
    b.n = 7
    b
}

let b = make_active()
test.expect((b.kind as u16) as i64, 0x02)
test.expect(b.n as i64, 7)
