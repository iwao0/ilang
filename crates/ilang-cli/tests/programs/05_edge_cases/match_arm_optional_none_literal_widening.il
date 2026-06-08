// 023a0ccb で `is_fresh_object_expr` の Match / IfLet 判定に
// `Str literal` だけ「fresh-equivalent」 として許容したが、 同じ
// 性質を持つ literal は他にもあるはず:
//
//   - `none` (`MirTy::Optional<Unit>`、 NewOptional inst で fresh 化
//     されるが内部状態は rc=1 で release 可能)
//   - integer / float literal (heap kind ではないので fresh の話に
//     ならない、 ただし stmt.rs::Let の Retain で fty.is_arc_heap
//     が false なら skip — これは別 path)
//   - bool literal — 同上
//
// この fixture は `match opt_int { some(n) { n } none { 0 } }` で
// 「none arm が int literal、 some arm が `Var(n)` で int 型」
// という形を確認する。 int 型は heap でないので stmt.rs::Let も
// caller-releases-fresh も発火しない、 ただし「Match fresh 判定」
// で arm_returns_own_binding が integer binding を返すケースに
// `arm_returns_own_binding` (heap return type 限定) が誤判定して
// over-retain しないか pin する。

use std.test as test

fn make_opt(n: i64): i64? { some(n) }

fn unwrap(o: i64?, default: i64): i64 {
    match o {
        some(n) { n }
        none    { default }
    }
}

test.expect(unwrap(make_opt(42), -1), 42)
test.expect(unwrap(none, -1), -1)

// heap 化される `string?` でも同じ shape。
fn make_str_opt(): string? { some("hi") }

fn unwrap_str(o: string?): string {
    match o {
        some(s) { s }
        none    { "" }
    }
}

test.expectStr(unwrap_str(make_str_opt()), "hi")
test.expectStr(unwrap_str(none), "")
