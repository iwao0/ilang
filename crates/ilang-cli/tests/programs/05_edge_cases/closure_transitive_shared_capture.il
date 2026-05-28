// Three levels of nested closures all sharing ONE mutable cell: the
// innermost writes the outermost factory's captured `n`, and the write
// is visible up the whole chain and persists across calls. Extends
// closure_nested_shared_cell (two levels) to a deeper chain.
use std.test as test

fn make(): fn(): i64 {
    let n = 0
    fn(): i64 {
        let mid = fn() {
            let inner = fn() { n = n + 1 }
            inner()
            inner()
        }
        mid()
        n
    }
}

let h = make()
test.expect(h(), 2)             // inner bumped twice per call
test.expect(h(), 4)             // the shared cell persists across calls
test.expect(h(), 6)

// A second factory invocation has its own independent cell.
let h2 = make()
test.expect(h2(), 2)
test.expect(h(), 8)             // first chain unaffected
