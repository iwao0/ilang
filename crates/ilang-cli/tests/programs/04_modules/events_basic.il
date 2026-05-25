// Built-in `events` module — minimal Node.js-style EventEmitter.
// Generic over a single payload type; listeners run in
// registration order. `off` removes by reference equality on
// the `fn` value (pass back the exact closure you gave to `on`).
use std.events as events
use std.test as test
// Counter class — closures capture this instance and mutate
// `.n`. Working around the closure-captures-array bug
// (where `let count = [0 as i32]` followed by a closure that
// reads `count[0]` reports `1` before any call) by using a
// class field instead.
class Counter {
    pub n: i32
    pub init() { this.n = 0 as i32 }
}

let bus = new events.EventEmitter<i32>()
let c = new Counter()

let inc = fn(n: i32) { c.n = c.n + n }
let scale = fn(n: i32) { c.n = c.n + n * 10 as i32 }

bus.on("inc", inc)
bus.on("inc", scale)

bus.emit("inc", 1 as i32)
test.expect(c.n as i64, 11)              // 1 + 10
test.expect(bus.listenerCount("inc") as i64, 2)

// `off` removes the matching listener.
test.expectBool(bus.off("inc", inc), true)
bus.emit("inc", 1 as i32)
test.expect(c.n as i64, 21)              // 11 + 10 (only `scale` left)
test.expect(bus.listenerCount("inc") as i64, 1)

// Removing the same listener again returns false.
test.expectBool(bus.off("inc", inc), false)

// `removeAllListeners` drops every listener for the event.
bus.removeAllListeners("inc")
bus.emit("inc", 99 as i32)
test.expect(c.n as i64, 21)              // unchanged
test.expect(bus.listenerCount("inc") as i64, 0)

// Different event names are independent.
class GreetLog {
    pub last: string
    pub init() { this.last = "" }
}
let gl = new GreetLog()
let bus2 = new events.EventEmitter<string>()
bus2.on("greet", fn(name: string) { gl.last = "hi " + name })
bus2.on("bye", fn(name: string) { gl.last = "bye " + name })

bus2.emit("greet", "alice")
test.expectStr(gl.last, "hi alice")
bus2.emit("bye", "bob")
test.expectStr(gl.last, "bye bob")
test.expect(bus2.listenerCount("greet") as i64, 1)
test.expect(bus2.listenerCount("bye") as i64, 1)
