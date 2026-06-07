// Interface-typed array and downcast (`as?`) from an interface
// slot to a concrete class. `interface_basic.il` covers passing
// an implementing class through an `Iface`-typed parameter; this
// fixture pins down the array / downcast directions.
use std.test as test

interface Drawable {
    draw(): string
}

class Circle: Drawable {
    pub init() {}
    pub draw(): string { "circle" }
}

class Square: Drawable {
    pub init() {}
    pub draw(): string { "square" }
}

// Drawable[] built by push — mixed concrete classes dispatch
// their own `draw` overrides.
let xs: Drawable[] = []
xs.push(new Circle())
xs.push(new Square())
xs.push(new Circle())
test.expect(xs.length, 3)
test.expectStr(xs[0].draw(), "circle")
test.expectStr(xs[1].draw(), "square")
test.expectStr(xs[2].draw(), "circle")

// Iteration through the interface element type still routes to
// the runtime class's vtable slot.
let labels: string[] = []
for d in xs { labels.push(d.draw()) }
test.expectStr(labels.join(","), "circle,square,circle")

// Downcast from an interface-typed slot back to a concrete class.
let any: Drawable = new Circle()
if let some(c) = any as? Circle {
    test.expectStr(c.draw(), "circle")
} else {
    test.fail("Circle slot should downcast to Circle")
}

// Failed downcast — Square instance via Drawable, asked for Circle.
let other: Drawable = new Square()
let bad = other as? Circle
test.expectBool(bad.isNone, true)

// Successful downcast on the same value.
let good = other as? Square
test.expectBool(good.isSome, true)
test.expectStr(good.unwrap().draw(), "square")
