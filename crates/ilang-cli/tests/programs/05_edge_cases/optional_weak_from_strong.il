// A strong `Object` flowing into a `Node.weak?` slot is the bare
// strong→weak auto-downgrade composed with the `T → T?` wrap. An
// explicit `some(strongRef)` worked (it coerces the inner to `Weak`
// then wraps), but the implicit/bare form was broken in several
// placements:
//
//   * `let w: Node.weak? = strongRef` and a `Node.weak?` argument
//     hit "no coercion from obj to weak?" at MIR lowering — the
//     `Object → Optional<Weak>` step matched no rule (the wrap arm and
//     the Optional→Optional widen both omitted `Weak`).
//   * `obj.f = strongRef` against a `Node.weak?` field compiled but
//     stored a RAW STRONG pointer into the weak Optional slot, so the
//     later upgrade / release SIGSEGV'd.
//   * returning a borrowed strong as `Node.weak` / `Node.weak?` leaked
//     the object (deinit never ran): the borrowed-tail retain bumped
//     the STRONG rc and then StrongToWeak-coerced the value, orphaning
//     that +1. (Pre-existing — plain `Node.weak` returns leaked too.)
//
// Fixes: `coerce` decomposes `Object → Optional<Weak>` like
// `lower_some_with_hint` (downgrade the inner to `Weak`, take the
// Optional's weak-rc share, box); `store_value_to_field` routes the
// same shape through `coerce`; and `emit_callee_retain` skips the
// strong borrow-retain when the return type is weak / optional-weak
// (the weak side balances its own rc, like a bare `let w: Node.weak`).
//
// Each `round*` makes one `Node`; the weak ref never keeps it alive,
// so it dies exactly once per call and the churn delta stays flat.

// expect: optional weak from strong ok

use std.test as test

let deinits: i64[] = [0]

class Node {
    pub p: Node.weak?
    pub id: i64
    init(id: i64) { this.id = id; this.p = none }
    deinit() { deinits[0] = deinits[0] + 1 }
}

fn rd(ow: Node.weak?): i64 {
    match ow {
        some(w) {
            match w.get() {
                some(n) { n.id }
                none { -1 }
            }
        }
        none { -2 }
    }
}

// let binding: bare strong -> Node.weak?
fn viaLet(): i64 {
    let s = new Node(11)
    let ow: Node.weak? = s
    rd(ow)
}

// argument position: bare strong -> Node.weak? param
fn argTake(ow: Node.weak?): i64 { rd(ow) }
fn viaArg(): i64 {
    let s = new Node(12)
    argTake(s)
}

// field store: bare strong -> Node.weak? field, read back while alive
fn viaFieldAlive(): i64 {
    let target = new Node(13)
    let holder = new Node(0)
    holder.p = target
    rd(holder.p)
}

// field store: target dies before the field is read -> none
fn viaFieldDead(): i64 {
    let holder = new Node(0)
    {
        let target = new Node(14)
        holder.p = target
    }
    rd(holder.p)
}

// return position: borrowed strong returned as Node.weak? (no leak)
fn asOptWeak(keep: Node): Node.weak? { keep }
fn viaReturn(): i64 {
    let s = new Node(15)
    let ow = asOptWeak(s)
    rd(ow)
}

let bd = deinits[0]
test.expect(viaLet(), 11)
test.expect(viaArg(), 12)
test.expect(viaFieldAlive(), 13)
test.expect(viaFieldDead(), -1)        // target died -> none
test.expect(viaReturn(), 15)
// viaLet/arg/return: 1 Node each. viaFieldAlive/Dead: 2 each (target +
// holder). 3*1 + 2*2 = 7 deinits.
test.expect(deinits[0] - bd, 7)

// churn: every Node dies, weak refs never pin them, nothing leaks.
let acc: i64[] = [0]
let d0 = test.liveAllocBytes()
let bd1 = deinits[0]
let i = 0
while i < 100 {
    acc[0] = acc[0]
        + viaLet() + viaArg() + viaFieldAlive() + viaFieldDead() + viaReturn()
    i = i + 1
}
test.expect(test.liveAllocBytes() - d0, 0)
test.expect(deinits[0] - bd1, 700)     // 7 per round, exactly

console.log("optional weak from strong ok")
