// Array literal with mixed-but-related class elements unifies to
// the common ancestor, matching the if/else / match branch
// unification path.
//
// Pre-fix the type checker fixed the element type to the FIRST
// element's class (here `Circle`) and then rejected later
// siblings (`Square`) with `expected Circle, got Square`. With
// inheritance unification it widens to `Shape` so the array can
// hold both — matching the obvious user expectation when calling
// virtual methods through the array.
//
// expect: 12.56
// expect: 9.0
// expect: [Circle { r: 2.0 }, Square { s: 3.0 }]

class Shape {
    init() {}
    area(): f64 { 0.0 }
}
class Circle: Shape {
    r: f64
    init(r: f64) { super(); this.r = r }
    override area(): f64 { 3.14 * this.r * this.r }
}
class Square: Shape {
    s: f64
    init(s: f64) { super(); this.s = s }
    override area(): f64 { this.s * this.s }
}

let shapes = [new Circle(2.0), new Square(3.0)]
for sh in shapes {
    console.log(sh.area())
}
console.log(shapes)
