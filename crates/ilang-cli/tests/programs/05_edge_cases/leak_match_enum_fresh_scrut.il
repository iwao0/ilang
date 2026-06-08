// fresh な `Enum` を `match` で受けて payload binding を arm tail
// で返すパターン。 47d4f979 の既存 fixture は scrutinee が Local
// (= 借用) のケースしか pin していない。 ここでは scrutinee が
// fresh (関数戻り値) で、 838d2dc4 の `lower_match_enum` に渡る
// `scrut_is_fresh = true` がきちんと PatternBinding に焼き付き、
// tail-Var Retain で cascade と balance するかを確認。
//
// 注意: 838d2dc4 時点では `lower_match_enum` は scrutinee の
// fresh-time Release を発行していない。 そのため caller が必ず
// 1 回 release してくれる前提で動く。 ここでは `let _ =
// through_match()` が `is_fresh_object_expr(Match)` 経由で
// release を入れてくれる (some arm tail = Var(inner) → arm
// fresh = false → Match fresh = false → stmt.rs::Let は retain
// 側に倒れる)。 結果として「inner Box の rc が caller release
// で 0 に落ちる」シナリオで leak を検証する。
//
// mir-jit only — liveAllocCount は AOT 配線なし。
// aot: skip
use std.test as test

class Box {
    n: i64
    init(n: i64) { this.n = n }
}

enum Container {
    empty
    has: (Box)
}

fn make_container(): Container {
    Container.has(new Box(7))
}

fn through_match(): Box {
    match make_container() {
        empty       { new Box(0) }
        has(inner)  { inner }
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
