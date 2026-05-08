// One getter calling another (`isFreezing` → `description`). The
// getter dispatch path has to support re-entry on the same instance
// without confusing `this` or recursing into the setter side.
// `08_properties/getter_setter_basic.il` covers a single getter-setter
// pair; this fixture adds the cross-getter call.
use test

class Temp {
    celsius: f64
    init(c: f64) { this.celsius = c }
    get fahrenheit(): f64 { this.celsius * 9.0 / 5.0 + 32.0 }
    set fahrenheit(v: f64) { this.celsius = (v - 32.0) * 5.0 / 9.0 }
    get isFreezing(): bool { this.celsius <= 0.0 }
    get description(): string {
        if this.isFreezing { "freezing" } else { "warm" }
    }
}

let t = new Temp(0.0)
test.expectStr(t.description, "freezing")
test.expectBool(t.isFreezing, true)

t.fahrenheit = 70.0
test.expectStr(t.description, "warm")
test.expectBool(t.isFreezing, false)

t.celsius = -10.0
test.expectStr(t.description, "freezing")
test.expectF64(t.fahrenheit, 14.0)
