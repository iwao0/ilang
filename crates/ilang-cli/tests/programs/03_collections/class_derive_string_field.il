// `@derive(Eq, Hash)` now handles string fields by routing through
// the built-in `string.hashCode()`. Two `Tag` instances with the
// same `name` should dedup in a Set.
use std.test as test

@derive(Eq, Hash)
class Tag {
    pub name: string
    pub weight: i64
    pub init(name: string, weight: i64) {
        this.name = name
        this.weight = weight
    }
}

let s = new Set<Tag>()
s.add(new Tag("alpha", 1))
s.add(new Tag("beta", 2))
s.add(new Tag("alpha", 1))   // structural dup → collapses
s.add(new Tag("alpha", 99))  // same name, different weight
test.expect(s.size(), 3)
test.expectBool(s.has(new Tag("alpha", 1)), true)
test.expectBool(s.has(new Tag("gamma", 0)), false)

// Map key — same protocol, same outcome.
let m = new Map<Tag, i64>()
m.set(new Tag("x", 0), 100)
m.set(new Tag("y", 0), 200)
m.set(new Tag("x", 0), 999)
test.expect(m.size(), 2)
test.expect(m[new Tag("x", 0)], 999)
