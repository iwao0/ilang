// `this.field = []` where field is `T[]` works under both
// interpreter and JIT. Pre-fix the JIT errored with
// "JIT array literal must have at least one element" because
// AssignField fed the empty literal into the no-hint path; the
// `let` path already routed empties through
// `lower_array_literal` with the field's elem type.
use std.test

class Tree {
    children: Tree[]
    label: i64
    init() {
        this.children = []      // empty array — field type is the hint
        this.label = 0
    }
    add(c: Tree) {
        this.children.push(c)
    }
}

let t = new Tree()
test.expect(t.children.length, 0)
t.add(new Tree())
t.add(new Tree())
test.expect(t.children.length, 2)
