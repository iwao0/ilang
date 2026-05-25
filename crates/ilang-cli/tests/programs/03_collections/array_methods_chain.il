// Every array method documented in §8 — exercised once each, then
// chained where possible.
use std.test as test
fn double(n: i64): i64 { n * 2 }
fn even(n: i64): bool { n % 2 == 0 }

let xs: i64[] = [1, 2, 3]

// push (mutates) — returns Unit.
xs.push(4)
xs.push(5)
test.expect(xs.length, 5)
test.expect(xs[4], 5)

// pop (mutates) — returns T?.
let p = xs.pop()
test.expectBool(p.isSome, true)
test.expect(p.unwrap(), 5)
test.expect(xs.length, 4)

// indexOf / includes.
test.expect(xs.indexOf(3), 2)
test.expect(xs.indexOf(99), -1)
test.expectBool(xs.includes(2), true)
test.expectBool(xs.includes(99), false)

// slice (non-mutating).
let mid = xs.slice(1, 3)
test.expect(mid.length, 2)
test.expect(mid[0], 2)
test.expect(mid[1], 3)
test.expect(xs.length, 4)        // source unchanged

// map (non-mutating, T → U).
let doubled = xs.map(double)
test.expect(doubled.length, 4)
test.expect(doubled[0], 2)
test.expect(doubled[3], 8)

// filter (non-mutating).
let evens = xs.filter(even)
test.expect(evens.length, 2)
test.expect(evens[0], 2)
test.expect(evens[1], 4)

// Chaining: map then filter then map.
let chained = xs.map(double).filter(even).map(double)
test.expect(chained.length, 4)   // every doubled value is already even
test.expect(chained[0], 4)       // 1*2*2 = 4
test.expect(chained[3], 16)      // 4*2*2 = 16

// forEach (side-effect via per-element mutable receiver).
class Counter {
    n: i64
    init() { this.n = 0 }
    add(k: i64) { this.n = this.n + k }
    get(): i64 { this.n }
}
let acc = new Counter()
class Bumper {
    c: Counter
    init(c: Counter) { this.c = c }
    bump(k: i64) { this.c.add(k) }
}
// We need a fn(i64) closure; ilang has no closures, so route through
// a class instance whose method we call from the top-level fn arg.
fn add_to_acc(k: i64) { /* can't access acc — use a wrapper */ }
// Easier: just use a per-element Counter array.
let bumpers: Counter[] = [new Counter(), new Counter()]
fn bumpit(c: Counter) { c.add(1) }
bumpers.forEach(bumpit)
test.expect(bumpers[0].get(), 1)
test.expect(bumpers[1].get(), 1)
