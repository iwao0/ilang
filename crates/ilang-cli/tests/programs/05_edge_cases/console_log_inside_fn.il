// `console.log` inside a fn body / method body — visible everywhere,
// not just at top-level. Pre-fix the interpreter installed
// `console` into `self.vars`, but a fn call does
// `mem::take(&mut self.vars)` to scope the call frame, so the
// builtin disappeared inside fn bodies and the interp errored
// `undefined variable "console"` while the JIT happily printed.
// Now `console` lives in `self.globals` (which fn calls don't
// touch), restoring parity.
//
// expect: hi
// expect: from method
// expect: from closure

fn say(s: string) {
    console.log(s)
}
say("hi")

class C {
    init() {}
    greet() {
        console.log("from method")
    }
}
new C().greet()

// `console` was getting added to closure capture lists by the type
// checker (since it lives in the top-level env), and the JIT would
// then error `unknown class "Console"` trying to bind it as a
// capture. Filter it out — both sides treat `console.log(...)` as
// a globally available primitive, not a user value.
let f = fn() {
    console.log("from closure")
}
f()
