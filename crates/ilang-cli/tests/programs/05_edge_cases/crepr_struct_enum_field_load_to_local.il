// CRepr struct (`@extern(C)` block 内) の unit-only enum field を
// load して local 変数に bind し、 match で分解できることを確認。
//
// `decl/extern_c.rs` は inline 用に `MirTy::CReprEnum(eid)` を field
// metadata に焼き付ける。 `lower_field` (literals.rs) は LoadField の
// dst SSA 型を `BodyCx::loaded_field_ty` 経由で `MirTy::Enum(eid)` に
// 昇格させ、 codegen は `__enum_unit_get_checked` を経由して heap-box
// pointer を返す。 結果として、 上位の `let mode = pkt.kind` および
// `match mode { ... }` は通常の heap enum cell として扱える。
//
// pre-CReprEnum 時代 (df51064a 後) の point fix (`parent_is_crepr &&
// matches!(fty, Enum(_))` を `is_arc_slot` から除外) では LoadField の
// 結果が一貫しなかった (codegen が動的判定で repr 幅 load → heap-box
// 化していたが、 lower 側の SSA 型は CRepr inline のまま `MirTy::Enum`)。
// この fixture は両層が `CReprEnum` ↔ `Enum` の昇格契約で揃って動く
// ことを pin する。

use std.test as test

pub enum Mode: u16 {
    idle   = 0x0001
    active = 0x0002
    paused = 0x0004
}

@extern(C) {
    pub struct Packet {
        kind: Mode
        seq:  i32
    }
}

let pkt = new Packet()
pkt.kind = Mode.active
pkt.seq = 42

// LoadField 経由で local に bind。 SSA 型は Enum に昇格しているはず
// (= match の scrutinee として heap enum cell の扱い)。
let mode = pkt.kind
let label = match mode {
    idle   { "idle" }
    active { "active" }
    paused { "paused" }
}
test.expectStr(label, "active")
test.expect(pkt.seq as i64, 42)

// 同じ field を別 variant で書き直して、 再読み込みでも正しく分岐
// すること。
pkt.kind = Mode.paused
let label2 = match pkt.kind {
    idle   { "idle" }
    active { "active" }
    paused { "paused" }
}
test.expectStr(label2, "paused")
