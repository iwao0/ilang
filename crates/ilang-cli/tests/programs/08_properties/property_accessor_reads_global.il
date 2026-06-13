// A property getter / setter body may reference a top-level `let` (a
// global) the same way a regular method does. The slot-promotion pass
// (`build_slot_table` -> `collect_fn_free_var_refs`) walked class methods
// and static methods but NOT property accessor bodies, so a top-level let
// referenced only by a getter / setter was never promoted to a host slot
// and hit "mir lower: unbound variable" at lowering. Methods worked;
// accessors didn't.
//
// Now the accessor bodies are walked too. This also confirms a getter is
// evaluated exactly once per access (the side-effect counter).

use std.test as test

let getCalls: i64[] = [0]
let setCalls: i64[] = [0]
let shared: i64[] = [100]

class Probe {
    _v: i64
    init() { this._v = 0 }

    // Getter reads + mutates a global.
    get next(): i64 {
        getCalls[0] = getCalls[0] + 1
        shared[0] = shared[0] + 1
        shared[0]
    }

    // Setter reads + mutates a global.
    set value(x: i64) {
        setCalls[0] = setCalls[0] + 1
        this._v = x + shared[0]
    }

    get stored(): i64 { this._v }
}

let p = new Probe()

// Three getter accesses -> shared goes 100 -> 103, getCalls = 3.
let a = p.next            // 101
let b = p.next            // 102
let c = p.next            // 103
test.expect(a + b + c, 101 + 102 + 103)
test.expect(getCalls[0], 3)          // evaluated once per access
test.expect(shared[0], 103)

// Two setter calls -> setCalls = 2; the setter reads the global.
p.value = 1               // _v = 1 + 103 = 104
p.value = 2               // _v = 2 + 103 = 105
test.expect(setCalls[0], 2)
test.expect(p.stored, 105)
