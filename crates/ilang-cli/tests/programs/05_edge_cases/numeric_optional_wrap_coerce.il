// Boxing a numeric value into a DIFFERENTLY-typed numeric Optional must
// convert it to the inner type first. `8.5` (f64) into an `f32?` slot
// used to store the raw f64 — the f64/f32 width mismatch then crashed
// (SIGSEGV) on read / release. The wrap path only matched when the inner
// type was exactly the value type; now a numeric inner demotes/promotes
// the value before boxing.
//
// expect: ok
use std.test as test

fn uwF(o: f32?): f32 {
    match o {
        some(f) { f }
        none { return 0.0f32 }
    }
}
fn uwI(o: i64?): i64 {
    match o {
        some(n) { n }
        none { return -1 }
    }
}
fn isNoneF(o: f32?): bool {
    match o {
        some(f) { false }
        none { return true }
    }
}

// --- f32? field (the original crash) ---
class C {
    a: f32?
    b: f32
    init() {
        this.a = 8.5      // f64 literal -> f32? field: demote + wrap
        this.b = 9.5
    }
}
let c = new C()
test.expectF64(uwF(c.a) as f64, 8.5)
test.expectF64(c.b as f64, 9.5)
c.a = none
test.expectTrue(isNoneF(c.a))
c.a = some(1.25)          // some(f64 literal) into f32? field
test.expectF64(uwF(c.a) as f64, 1.25)

// --- let / arg / return ---
let o1: f32? = 8.5
test.expectF64(uwF(o1) as f64, 8.5)
test.expectF64(uwF(retFOpt()) as f64, 9.5)

// --- widening numeric optional (i32 -> i64?) ---
let small: i32 = 42
let o2: i64? = small
test.expectTrue(uwI(o2) == 42)

// --- f32? in containers ---
let arr: (f32?)[] = [some(1.5), none]
test.expectF64(uwF(arr[0]) as f64, 1.5)
test.expectTrue(isNoneF(arr[1]))
let m = new Map<string, f32?>()
m["k"] = some(2.5)
test.expectF64(uwF(m["k"]) as f64, 2.5)

console.log("ok")

fn retFOpt(): f32? { 9.5 }
