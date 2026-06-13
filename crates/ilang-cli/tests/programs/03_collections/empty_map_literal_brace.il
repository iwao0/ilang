// `{}` parses as an empty block (value `()`), so `let m: Map<K,V> = {}`
// used to fail type-checking ("expected Map<K,V>, got ()") — the only way
// to make an empty map was `new Map<K,V>()`. Now, when a `Map<K,V>` is the
// expected type, an empty block is read as the empty-map shorthand (the
// JS-style `{}`), at every position a Map type is pushed in: let
// annotation, reassignment, field assignment, fn return, fn arg, and a
// nested map value. A non-empty `{ k: v }` is already a MapLit and is
// unaffected; an empty block in any non-Map (unit) context still means
// unit.
//
// The empty block lowers to a fresh `NewMap` (rc=1); the binding owns that
// +1 and releases it (and any heap values inside) at scope exit, so ARC
// stays exact and reassigning over an `{}` drops the prior map.

use std.test as test

let deinits: i64[] = [0]

class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}

class Bag {
    m: Map<string, Box>
    init() { this.m = {} }                   // field assignment of `{}`
}

fn mkEmpty(): Map<string, Box> { {} }        // fn return of `{}`

fn count(m: Map<string, Box>): i64 { m.size() }

// let annotation `{}`, filled via index-store and set. 2 Box/call.
fn letFill(): i64 {
    let m: Map<string, Box> = {}
    m["a"] = new Box(1)
    m.set("b", new Box(2))
    let got = match m.get("a") {
        some(b) { b.n }
        none { 0 - 1 }
    }
    got + m.size()                           // 1 + 2 = 3
}

// reassigning over an `{}` must drop the prior map (and its Box). 2/call.
fn reassign(): i64 {
    let m: Map<string, Box> = {}
    m.set("a", new Box(1))                   // Box(1) dies on the reassign
    m = {}
    m.set("b", new Box(2))                   // Box(2) dies at scope exit
    m.size()                                 // 1
}

// field initialised to `{}`, then filled. 1/call.
fn fieldInit(): i64 {
    let b = new Bag()
    b.m.set("x", new Box(3))
    b.m.size()                               // 1
}

// `{}` returned from a fn, then filled. 1/call. Also exercises `{}` as a
// fn argument (count receives an empty map, 0 Box).
fn retAndArg(): i64 {
    let m = mkEmpty()
    m.set("y", new Box(4))
    m.size() + count({})                     // 1 + 0 = 1
}

// nested: outer `{}` holds an inner `{}` value. No Box, but both maps must
// be released (caught by the byte-delta assertion).
fn nested(): i64 {
    let m: Map<string, Map<string, Box>> = {}
    m.set("outer", {})
    match m.get("outer") {
        some(inner) { inner.size() + 7 }     // 0 + 7 = 7
        none { 0 - 1 }
    }
}

test.expect(letFill(), 3)
test.expect(reassign(), 1)
test.expect(fieldInit(), 1)
test.expect(retAndArg(), 1)
test.expect(nested(), 7)

// Per round: letFill 2 + reassign 2 + fieldInit 1 + retAndArg 1 + nested 0
//          = 6.
let acc: i64[] = [0]
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 {
    acc[0] = acc[0]
        + letFill() + reassign() + fieldInit() + retAndArg() + nested()
    i = i + 1
}
test.expect(deinits[0] - bd, 600)
test.expect(test.liveAllocBytes() - d0, 0)
