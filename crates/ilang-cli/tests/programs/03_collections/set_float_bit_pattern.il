// `Set<f32/f64>` hashes and compares elements by their raw BIT PATTERN
// (docs: "Floats compare by bit pattern, so distinct NaN payloads stay
// as separate entries"). `set_element_types.il` covers ordinary float
// dedup; this pins the special-value consequences a value-equality
// implementation would get wrong:
//   - NaN is findable: `has(nan)` is true even though NaN != NaN;
//   - +0.0 and -0.0 are DISTINCT entries (different bit patterns);
//   - +Inf and -Inf are distinct, and each value dedups with itself;
//   - delete locates a NaN by its bits.
use std.test as test

let nan = 0.0 / 0.0
let inf = 1.0 / 0.0

let s = new Set<f64>()
// The same NaN bit pattern dedups...
s.add(nan)
s.add(nan)
test.expect(s.size(), 1)
// ...and a bit-pattern lookup finds it, despite NaN != NaN by value.
test.expectBool(s.has(nan), true)

// +0.0 and -0.0 carry distinct bit patterns -> distinct entries.
let z = new Set<f64>()
z.add(0.0)
z.add(-0.0)
test.expect(z.size(), 2)
test.expectBool(z.has(0.0), true)
test.expectBool(z.has(-0.0), true)

// Each infinity dedups with itself; +Inf and -Inf are distinct.
let i = new Set<f64>()
i.add(inf)
i.add(inf)
i.add(-1.0 / 0.0)
test.expect(i.size(), 2)
test.expectBool(i.has(inf), true)

// delete locates the NaN entry by its bits.
test.expectBool(s.delete(nan), true)
test.expect(s.size(), 0)
