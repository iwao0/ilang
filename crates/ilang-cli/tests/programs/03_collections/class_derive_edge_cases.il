// Corner cases for the `@derive(Eq, Hash)` synthesis:
//   - class with no fields (every instance is equal to every
//     other; hashCode is the seed)
//   - single-field class (the polynomial fold reduces to one
//     step)
//   - bool field (the `if cond { 1 } else { 0 }` cast path)
//   - mixed integer widths (each cast to i64)
use std.test as test

// ---- empty fields: all instances are equal, one bucket ----
@derive(Eq, Hash)
class Marker {
    init() {}
}
let s_marker = new Set<Marker>()
s_marker.add(new Marker())
s_marker.add(new Marker())
s_marker.add(new Marker())
test.expect(s_marker.size(), 1)
test.expectBool(s_marker.has(new Marker()), true)

// ---- single field ----
@derive(Eq, Hash)
class Id {
    pub n: i64
    pub init(n: i64) { this.n = n }
}
let s_id = new Set<Id>()
s_id.add(new Id(1))
s_id.add(new Id(2))
s_id.add(new Id(1))
test.expect(s_id.size(), 2)
test.expectBool(s_id.has(new Id(1)), true)
test.expectBool(s_id.has(new Id(42)), false)

// ---- bool field ----
@derive(Eq, Hash)
class Flag {
    pub on: bool
    pub init(on: bool) { this.on = on }
}
let s_flag = new Set<Flag>()
s_flag.add(new Flag(true))
s_flag.add(new Flag(false))
s_flag.add(new Flag(true))   // dup
test.expect(s_flag.size(), 2)

// ---- mixed integer widths ----
@derive(Eq, Hash)
class Mixed {
    pub a: i8
    pub b: u16
    pub c: i32
    pub d: u64
    pub init(a: i8, b: u16, c: i32, d: u64) {
        this.a = a
        this.b = b
        this.c = c
        this.d = d
    }
}
let s_mix = new Set<Mixed>()
s_mix.add(new Mixed(1i8, 2u16, 3i32, 4u64))
s_mix.add(new Mixed(1i8, 2u16, 3i32, 4u64))   // dup
s_mix.add(new Mixed(1i8, 2u16, 3i32, 99u64))   // differs on d
test.expect(s_mix.size(), 2)
