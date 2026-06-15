// Capability bypass guard: taking the *address* of a C function and
// calling it indirectly must not slip past the ffi gate. `let f = abs`
// lowers to a closure over the extern; without gating the address-of,
// the later `f(...)` is an indirect call the call-site gate never sees.
// A self-contained @extern(C) declaration keeps `abs` the only FFI sink,
// so the diagnostic can only come from this indirect use.
//
// expect-error: capability 'ffi'
@extern(C) {
    @lib("c") fn abs(x: i32): i32

    pub fn run() {
        let f = abs
        console.log(f(-7 as i32).toString())
    }
}

run()
