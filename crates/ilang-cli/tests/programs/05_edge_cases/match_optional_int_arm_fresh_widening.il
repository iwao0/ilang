// 023a0ccb の Match fresh 判定は arm body が `Str` literal の場合
// にだけ「fresh-equivalent」として許容する。 int / bool literal も
// rc=-1 相当 (= release-safe) なので、 同じ扱いで良いはず。
// 現状の判定では int literal の arm が混じると Match fresh = false
// になる。 over-retain の方向で safe なので動作上は問題ないが、
// 確認しておく。

use std.test as test

fn make_opt(): i64? { some(7) }

// arm 1: int Var (binding)、 arm 2: int literal。
// is_fresh_object_expr では int literal も「fresh じゃない」と
// 判定される (`_ => false`)。 ただし i64 は heap kind ではない
// (is_arc_heap=false) ので、 stmt.rs::Let / caller-releases-fresh
// の release / retain 自体は skip される。 結果として安全。
fn through(): i64 {
    match make_opt() {
        some(n) { n }
        none    { 0 }
    }
}

test.expect(through(), 7)

// `let _ = through()` の `_` discard も問題ないか。
let _ = through()
test.expect(through(), 7)

// 直接 expression として使う path。
test.expect(match make_opt() {
    some(n) { n + 1 }
    none    { 99 }
}, 8)
