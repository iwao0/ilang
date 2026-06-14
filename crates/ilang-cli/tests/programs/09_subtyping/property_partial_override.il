// A subclass that overrides only ONE accessor of an inherited property
// keeps the parent's accessor for the other direction. The checker built
// each class's property accessor set from its OWN declarations only, so
// overriding just the getter made the property read-only in the subclass
// (`has no setter`), and overriding just the setter made it write-only.
// The accessor presence now merges with the inherited entry.
//
// Combined with virtual dispatch, the overridden accessor and the
// inherited one both route by the runtime class through a base-typed
// reference.
use std.test as test

class Base {
    store: i64
    init() { this.store = 0 }
    get prop(): i64 { this.store }
    set prop(v: i64) { this.store = v }
}

class GetOnly: Base {
    init() { super() }
    get prop(): i64 { this.store + 1000 }   // override getter, inherit setter
}

class SetOnly: Base {
    init() { super() }
    set prop(v: i64) { this.store = v * 2 }  // override setter, inherit getter
}

// Overridden getter + inherited setter, reached through a base reference.
let g: Base = new GetOnly()
g.prop = 7                  // inherited Base setter stores 7
test.expect(g.prop, 1007)   // overridden getter returns 7 + 1000

// Inherited getter + overridden setter, reached through a base reference.
let s: Base = new SetOnly()
s.prop = 7                  // overridden setter stores 7 * 2
test.expect(s.prop, 14)     // inherited getter returns 14

// Direct (static-type) access agrees.
let g2 = new GetOnly()
g2.prop = 3
test.expect(g2.prop, 1003)
let s2 = new SetOnly()
s2.prop = 4
test.expect(s2.prop, 8)
