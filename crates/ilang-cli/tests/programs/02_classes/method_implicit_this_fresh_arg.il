// Fresh heap args passed through an IMPLICIT `this.method(...)`
// call (bare `method(...)` inside a method body) follow the same
// transfer rule as every other call shape: the caller's +1 on a
// fresh arg is released after the call. This arm of the call
// lowering was the last one missing the release — a fresh string
// leaked one registry entry per call (fresh objects often escaped
// notice because escape analysis stack-promotes a non-escaping
// `new Box(..)` argument).
//
// expect: implicit strings=0
// expect: implicit objects delta=0
// expect: total ok

use std.test as test

class Box2 { n: i64; init(x: i64) { this.n = x } }

class Eater {
    total: i64
    init() { this.total = 0 }
    eat(b: Box2) { this.total = this.total + b.n }
    eatS(s: string) { this.total = this.total + s.length }
    feedB(k: i64) { eat(new Box2(k)) }
    feedS(k: i64) { eatS("ss" + k.toString()) }
}

let e = new Eater()

let s1 = test.liveStringCount()
let i = 0
while i < 200 { e.feedS(i); i = i + 1 }
console.log("implicit strings=" + (test.liveStringCount() - s1).toString())

let b1 = test.liveAllocBytes()
let j = 0
while j < 200 { e.feedB(j); j = j + 1 }
console.log("implicit objects delta=" + (test.liveAllocBytes() - b1).toString())

test.expectTrue(e.total > 0)
console.log("total ok")
