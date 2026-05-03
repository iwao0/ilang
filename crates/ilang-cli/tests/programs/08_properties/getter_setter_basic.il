// Computed property: read goes through getter, write through setter.
use test

class Temp {
    celsius: f64
    init(c: f64) { this.celsius = c }
    get fahrenheit(): f64 { this.celsius * 9.0 / 5.0 + 32.0 }
    set fahrenheit(v: f64) { this.celsius = (v - 32.0) * 5.0 / 9.0 }
}

let t = new Temp(0.0)
test.expectF64(t.fahrenheit, 32.0)

t.fahrenheit = 100.0
test.expectF64(t.celsius, 37.77777777777778)

t.fahrenheit = 32.0
test.expectF64(t.celsius, 0.0)
