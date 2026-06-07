// A method can write through the class's own `set` property —
// `this.name = v` inside the body routes to the setter, not to a
// raw field store.
use std.test as test

class Temp {
    raw: f64
    init(c: f64) { this.raw = c }
    get celsius(): f64 { this.raw }
    set celsius(v: f64) { this.raw = v }
    set fahrenheit(v: f64) { this.celsius = (v - 32.0) * 5.0 / 9.0 }
}

let t = new Temp(0.0)
test.expectF64(t.celsius, 0.0)

// Drives the setter through `this.celsius = ...` inside the
// `fahrenheit` setter; confirms intra-class setter dispatch.
t.fahrenheit = 212.0
test.expectF64(t.celsius, 100.0)
