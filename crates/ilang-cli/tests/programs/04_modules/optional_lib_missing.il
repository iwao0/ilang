// `@lib(...) @optional` allows the JIT to keep going when the
// library can't be found — calls to its fns turn into a runtime
// stub that aborts if invoked. `os.libLoaded(...)` lets the program
// detect the situation and fall back.
//
// This fixture targets a definitely-missing library; the stub must
// NOT execute, so we pick the else branch.
use os
use test

@extern(C) {
    @lib("definitely_not_a_real_lib_xyz123.so") @optional fn never_called(): i64
}

test.expectFalse(os.libLoaded("definitely_not_a_real_lib_xyz123.so"))

let result = if os.libLoaded("definitely_not_a_real_lib_xyz123.so") {
    never_called()
} else {
    -1
}
test.expect(result, -1)
