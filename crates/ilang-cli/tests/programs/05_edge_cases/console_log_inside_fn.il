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
