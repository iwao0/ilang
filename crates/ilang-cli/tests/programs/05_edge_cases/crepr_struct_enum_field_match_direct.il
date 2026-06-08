// `match crepr_struct.enum_field { ... }` — local に bind せず直接
// match の scrutinee として渡す path。 `lower_match` の入口で
// `is_fresh_object_expr(scrutinee)` 判定が走り、 838d2dc4 の
// PatternBinding `needs_retain_on_tail` が `scrut_is_fresh` を見て
// 決まる。 CReprEnum field は LoadField で `Enum` に昇格しているので
// match の lower 経路は通常の heap enum cell として扱える。

use std.test as test

pub enum Mode: u16 {
    idle   = 0x0001
    active = 0x0002
    paused = 0x0004
}

@extern(C) {
    pub struct State {
        mode: Mode
    }
}

let s = new State()
s.mode = Mode.paused

let label = match s.mode {
    idle   { "idle" }
    active { "active" }
    paused { "paused" }
}
test.expectStr(label, "paused")

// `if let` 形式でも同じ shape を確認したい — `if let active = s.mode`
// は Optional ではないので使えない。 代わりに switch-style match
// の一部だけ書き直して flow を再度通す。
s.mode = Mode.idle
test.expect(match s.mode {
    idle   { 0 as i64 }
    active { 1 as i64 }
    paused { 2 as i64 }
}, 0)
