// `console.log(weak)` parity between interp and JIT.
//
// Pre-fix the JIT printed `<weak C alive>` instead of the
// interpreter's `weak(<alive>)`, AND the JIT read the wrong
// offset for the strong rc (the weak_rc slot at -24 instead of
// the strong_rc slot at -32 / runtime::STRONG_OFFSET), so a
// dead weak still printed alive.
//
// expect: weak(<alive>)
// expect: weak(<dead>)

class C { init() {} }

let s1 = new C()
let w1: C.weak = s1
console.log(w1)

let s2 = new C()
let w2: C.weak = s2
s2 = new C()    // drop the original; w2 is now dead
console.log(w2)
