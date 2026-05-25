// Multiple ilang `@symbol("name")` declarations pointing at the
// same C symbol but with *different* ilang signatures — the
// shape Cocoa bindings need to express `objc_msgSend(obj, sel,
// ...)` per call shape. The first declaration owns the
// canonical Cranelift signature; subsequent declarations are
// aliased to the same FuncId and dispatched via `func_addr +
// call_indirect` at each call site so each call marshals
// arguments under its own ilang signature.
//
// We exercise three scenarios:
//   1. Two aliases with identical signatures — the second
//      declaration must not error at codegen, and both must
//      route to the same underlying C function.
//   2. Two aliases with *different* parameter counts — the
//      alias dispatch path must use the per-callee signature
//      so each call marshals correctly. The C side reads only
//      the first arg (`x`); any extra args ride in unused
//      registers and don't affect the result.
//   3. The canonical and the aliased ilang names must both
//      produce the same C-side result.
//
// jit: skip
use std.test as test
@extern(C) {
    // Canonical: matches the C function's real signature.
    fn test_alias_inc(x: i64): i64

    // Same-sig alias of the same C symbol. Codegen routes this
    // through `func_addr + call_indirect`, NOT a fresh
    // declare_function (Cranelift only allows one signature per
    // import symbol).
    @symbol("test_alias_inc") fn inc_alias_same(x: i64): i64

    // Different-sig alias: declares an extra `ignored` param.
    // ilang places it in x1 at the call site; the C side reads
    // only x0 and never touches x1, so the result is `x + 1`
    // regardless of `ignored`.
    @symbol("test_alias_inc") fn inc_alias_extra(x: i64, ignored: i64): i64

    // Canonical for the pair variant.
    fn test_alias_pair(a: i64, b: i64): i64

    // Same-sig alias.
    @symbol("test_alias_pair") fn pair_alias_same(a: i64, b: i64): i64
}

test.expect(test_alias_inc(41), 42)
test.expect(inc_alias_same(41), 42)
// Extra arg is irrelevant; the C function ignores x1.
test.expect(inc_alias_extra(41, 999), 42)
test.expect(inc_alias_extra(41, -7), 42)

test.expect(test_alias_pair(3, 5), 305)
test.expect(pair_alias_same(3, 5), 305)
