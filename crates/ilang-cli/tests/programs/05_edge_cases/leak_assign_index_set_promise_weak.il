// `arr[i] = new_value` for a heap-typed Array element has to
// release the slot's prior occupant and (for borrowed rhs)
// retain the new one — pre-fix the `elem_is_heap` match in
// `ExprKind::AssignIndex` (Array branch) listed Object / Array
// / Tuple / Map / Optional / Fn / Str / Enum, but omitted Set /
// Promise / Weak. So an `arr: Set<T>[]` overwrite leaked the
// prior Set on every store.
//
// Probes a Tag element held by each kind so liveAllocCount
// notices.
//
// mir-jit only — liveAllocCount isn't wired into AOT.
// aot: skip
use std.test as test

@derive(Eq, Hash)
class Tag {
    pub n: i64
    init(n: i64) { this.n = n }
}

class N {
    v: i64
    init(x: i64) { this.v = x }
}

fn round_set_elem() {
    let s0 = new Set<Tag>()
    s0.add(new Tag(0))
    let arr: Set<Tag>[] = [s0]
    let i: i64 = 0
    while i < 20 {
        let s = new Set<Tag>()
        s.add(new Tag(i))
        arr[0] = s
        i = i + 1
    }
}

fn round_promise_elem() {
    let arr: Promise<N>[] = [Promise.resolve(new N(0))]
    let i: i64 = 0
    while i < 20 {
        arr[0] = Promise.resolve(new N(i))
        i = i + 1
    }
}

fn round_weak_elem() {
    let n = new N(0)
    let arr: N.weak[] = [n]
    let i: i64 = 0
    while i < 20 {
        arr[0] = n
        i = i + 1
    }
}

round_set_elem()
round_promise_elem()
round_weak_elem()

let base = test.liveAllocCount()
for _ in 0..50 {
    round_set_elem()
    round_promise_elem()
    round_weak_elem()
}
test.expect(test.liveAllocCount() - base, 0)
