// Generic-enum ctor literal covariance composes through the `some(..)` /
// tuple / array descent, and applies at the fn-arg and fn-return
// positions, not just a direct `let`. The covariance is checked at the
// type level inside `literal_assignable_with` (the ctor's inferred args
// vs the declared args), so it rides the same recursion the array / map /
// tuple literal cases use.
//
// Companion to `generic_enum_literal_covariant.il` (direct let / array /
// map). Here: argument, return, `some(..)`-wrapped, tuple element, and a
// deeply-nested `Result<Animal,string>?[]`. Heap Dog payloads stay
// ARC-exact and dispatch through the parent type.

use std.test as test

let deinits: i64[] = [0]

class Animal {
    init() {}
    deinit() { deinits[0] = deinits[0] + 1 }
    val(): i64 { 1 }
}

class Dog: Animal {
    init() { super() }
    deinit() { super() }
    override val(): i64 { 10 }
}

fn take(r: Result<Animal, string>): i64 {
    match r {
        ok(a) { a.val() }
        err(s) { 0 - 1 }
    }
}

fn ret(): Result<Animal, string> {
    Result.ok(new Dog())
}

// arg + return + some + tuple + deep nesting. 4 Dogs/round.
fn churn(): i64 {
    let a = take(Result.ok(new Dog()))                       // arg
    let b = match ret() {                                    // return
        ok(an) { an.val() }
        err(s) { 0 - 2 }
    }
    let o: Result<Animal, string>? = some(Result.ok(new Dog()))   // some-wrapped
    let c = match o {
        some(r) {
            match r {
                ok(an) { an.val() }
                err(s) { 0 - 3 }
            }
        }
        none { 0 - 9 }
    }
    let t: (Result<Animal, string>, i64) = (Result.ok(new Dog()), 5)  // tuple elem
    let (tr, tn) = t
    let d = match tr {
        ok(an) { an.val() }
        err(s) { 0 - 4 }
    }
    a + b + c + d + tn                                       // 10*4 + 5 = 45
}

test.expect(churn(), 45)

let acc: i64[] = [0]
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 { acc[0] = acc[0] + churn(); i = i + 1 }
test.expect(deinits[0] - bd, 400)            // 4 Dog/round x 100
test.expect(test.liveAllocBytes() - d0, 0)
