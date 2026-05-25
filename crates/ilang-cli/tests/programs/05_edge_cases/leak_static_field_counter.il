// Each `new Counter()` increments a static counter from `init`,
// each scope-exit drop calls `deinit` which increments another.
// At end-of-loop both counters MUST equal the iteration count —
// any miss means an allocation didn't fire its lifecycle hook.
// Verifies init/deinit symmetry across many iterations and that
// static-field increments survive the discard-as-stmt pattern.
//
// `arc_scope_drop.il` covers a similar shape with explicit
// scopes; this fixture stresses the discard-as-stmt path
// (`let _ = new Counter()` -- well, `new Counter()` as expr-
// statement).

use std.test as test
class Counter {
    n: i64
    init() {
        this.n = 0
        Counter.created = Counter.created + 1
    }
    deinit() { Counter.dropped = Counter.dropped + 1 }
    static created: i64 = 0
    static dropped: i64 = 0
}

let i: i64 = 0
while i < 100 {
    new Counter()        // expression statement — instance discarded
    i = i + 1
}

test.expect(Counter.created, 100)
test.expect(Counter.dropped, 100)
