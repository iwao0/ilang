// `deinit` runs for top-level `let` bindings at program exit,
// matching the JIT's `__main` exit-drop sweep.
//
// Pre-fix the interpreter dropped `Rc`s when the Interpreter
// struct went out of scope, but never called user `deinit` on
// surviving top-level globals — so `let g = new G()` at file
// scope leaked silently from the user's point of view, while the
// JIT happily ran `deinit`. The split between `Interpreter::run`
// and the new `release_globals_at_exit` keeps REPL semantics
// (globals persist across input chunks) while the script-mode
// CLI invokes the cleanup explicitly after `run` returns.
//
// expect: init A
// expect: init B
// expect: --- end
// expect: deinit B
// expect: deinit A

class A {
    init() { console.log("init A") }
    deinit() { console.log("deinit A") }
}
class B {
    init() { console.log("init B") }
    deinit() { console.log("deinit B") }
}

let a = new A()
let b = new B()
console.log("--- end")
