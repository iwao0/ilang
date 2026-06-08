// `arm_returns_own_binding` (body_cx.rs) は arm body tail の `Var(n)`
// が **arm pattern の binding 名** と一致するときだけ true を返す。
// 別 binding (例: enclosing scope の Local) と同名でも別 scope な
// 場合の挙動を pin する。
//
//   ```
//   let x = some_box()
//   match make_opt() {
//       some(v) { x }    // tail = Var(x); arm binding は `v`、 別物
//       none    { x }
//   }
//   ```
//
// scrut_is_fresh = true でも、 arm tail = Var(x) は pattern binding
// ではないので `arm_returns_own_binding = false` → `is_fresh_object_expr`
// は arm body 自体の fresh 判定に委ねる。 `Var(x)` は Local lookup
// で fresh = false → Match fresh = false → stmt.rs::Let で Retain
// が走り、 `x` の rc が +1 されて balance する。
//
// 怪しいのは「Var が pattern binding と同名 but 別 scope」のような
// shadowing。 ilang の構文で shadowing が許される場合に挙動が変わるか。

use std.test as test

class Box {
    n: i64
    init(n: i64) { this.n = n }
}

fn maybe(): Box? { some(new Box(42)) }

fn fallback(): Box { new Box(-1) }

let x = new Box(7)

fn choose(use_match: bool): Box {
    if use_match {
        match maybe() {
            some(v) { x }   // arm binding `v` は使わず、 外の `x` を返す
            none    { fallback() }
        }
    } else {
        x
    }
}

let r = choose(true)
test.expect(r.n, 7)

let r2 = choose(false)
test.expect(r2.n, 7)
