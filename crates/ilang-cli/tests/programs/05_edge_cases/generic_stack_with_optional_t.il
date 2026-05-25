// Generic class whose methods both consume and return `T?`. The
// monomorphized `Stack<i64>` and `Stack<string>` each get distinct
// concrete code, and the `T?` return type has to round-trip the
// type parameter into the Optional layout per instantiation. Both
// the primitive and heap-typed instantiations are covered.
use std.test as test
class Stack<T> {
    xs: T[]
    init() { this.xs = [] }
    push(v: T) { this.xs.push(v) }
    pop(): T? { this.xs.pop() }
    size(): i64 { this.xs.length }
}

let s = new Stack<i64>()
test.expect(s.size(), 0)
test.expectBool(s.pop().isNone, true)

s.push(1)
s.push(2)
s.push(3)
test.expect(s.size(), 3)
test.expect(s.pop().unwrap(), 3)
test.expect(s.pop().unwrap(), 2)
test.expect(s.size(), 1)
test.expect(s.pop().unwrap(), 1)
test.expectBool(s.pop().isNone, true)
test.expect(s.size(), 0)

// Heap T — distinct monomorphization.
let strs = new Stack<string>()
strs.push("hi")
strs.push("yo")
test.expect(strs.size(), 2)
test.expectStr(strs.pop().unwrap(), "yo")
test.expectStr(strs.pop().unwrap(), "hi")
test.expectBool(strs.pop().isNone, true)
