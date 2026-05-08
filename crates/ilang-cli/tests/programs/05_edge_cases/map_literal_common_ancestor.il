// Map literal with mixed-but-related class values unifies to the
// common ancestor, mirroring the array-literal / branch
// unification path.
//
// Pre-fix `{"a": new Circle(), "b": new Square()}` errored at
// type-check time with `expected Circle, got Square` — the value
// type was pinned to the first entry's class. With inheritance
// unification it widens to `Shape` so `Map<string, Shape>` is
// inferred and the `console.log(m)` parity check passes.
//
// expect: {a: Circle { r: 1.0 }, b: Square { s: 2.0 }}

class Shape {
    init() {}
    area(): f64 { 0.0 }
}
class Circle extends Shape {
    r: f64
    init(r: f64) { super(); this.r = r }
}
class Square extends Shape {
    s: f64
    init(s: f64) { super(); this.s = s }
}

let m = {"a": new Circle(1.0), "b": new Square(2.0)}
console.log(m)
