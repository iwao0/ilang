// `obj_a == obj_b` should work when one operand is a subclass of
// the other — both runtimes compare by reference identity, the
// upcast doesn't change the underlying pointer.
//
// Pre-fix the type checker rejected `p == q` (P, Q where Q
// extends P) as `cannot apply binary op between P and Q`,
// requiring the user to upcast manually with `q as P`. The JIT
// path also bailed on cross-class Object operands at lowering
// time. Now the TC accepts subclass-direction Eq/Ne and the JIT
// emits a plain i64 icmp on the user pointers.
//
// expect: true
// expect: true
// expect: false
// expect: true
// expect: true

class P { init() {} }
class Q: P { init() { super() } }

let p1 = new P()
let q1 = new Q()
let p_alias: P = q1

console.log(p_alias == q1)        // true — same instance, different static types
console.log(q1 == p_alias)        // true — symmetric
console.log(p_alias == p1)        // false — different instances
console.log(q1 != p1)             // true — `!=` parity

// And same-class still works.
let q2 = q1
console.log(q1 == q2)
