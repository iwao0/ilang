// `interface` declares a method contract; `class C: I, J { ... }`
// makes C implement every method of every listed interface. Values
// of class type can flow into a slot typed as the interface, and
// method calls on the interface-typed value virtual-dispatch to
// the receiver's actual class.
// expect: circle
// expect: square
// expect: cat-noise
// expect: dog-noise

interface Drawable {
    fn draw(): string
}

interface Speaks {
    fn speak(): string
}

class Circle: Drawable {
    pub init() {}
    pub draw(): string { "circle" }
}

class Square: Drawable {
    pub init() {}
    pub draw(): string { "square" }
}

fn render(d: Drawable) {
    console.log(d.draw())
}

let c: Drawable = new Circle()
let s: Drawable = new Square()
render(c)
render(s)

// Multiple interfaces + parent class. Same `:` list, comma-separated.
class Animal {
    pub init() {}
    pub kind(): string { "animal" }
}

class Cat: Animal, Speaks {
    pub init() { super() }
    pub speak(): string { "cat-noise" }
}

class Dog: Animal, Speaks {
    pub init() { super() }
    pub speak(): string { "dog-noise" }
}

fn shout(s: Speaks) {
    console.log(s.speak())
}

let cat: Speaks = new Cat()
let dog: Speaks = new Dog()
shout(cat)
shout(dog)
