// Optional<T> with heap T (string, Object).
use test

let s_opt: string? = some("hi")
test.expectBool(s_opt.isSome(), true)
test.expectBool(s_opt.isNone(), false)
test.expectStr(s_opt.unwrap(), "hi")

let s_none: string? = none
test.expectBool(s_none.isSome(), false)
test.expectBool(s_none.isNone(), true)

// T → T? auto-wrap on annotated let.
let auto_wrap: string? = "hello"
test.expectBool(auto_wrap.isSome(), true)
test.expectStr(auto_wrap.unwrap(), "hello")

// Optional<Object>.
class Box {
    n: i64
    init(v: i64) { this.n = v }
}
let bo: Box? = some(new Box(42))
test.expectBool(bo.isSome(), true)
test.expect(bo.unwrap().n, 42)

let bn: Box? = none
test.expectBool(bn.isNone(), true)
