// A class can declare a parent plus multiple interfaces via the
// same `:` base list — parent first, interfaces follow after
// commas. Each interface slot accepts the concrete value and
// virtual-dispatches to the class's implementation. Per
// syntax.md §7 "Interfaces".
use std.test as test

interface Drawable {
    draw(): string
}

interface Speaks {
    speak(): string
}

class Animal {
    init() {}
    kind(): string { "animal" }
}

class Cat: Animal, Drawable, Speaks {
    init() { super() }
    draw(): string { "cat-shape" }
    speak(): string { "meow" }
}

let c = new Cat()
test.expectStr(c.kind(), "animal")
test.expectStr(c.draw(), "cat-shape")
test.expectStr(c.speak(), "meow")

// Same value flows into either interface slot and into the
// parent class slot — virtual dispatch always routes to Cat's
// methods.
let d: Drawable = c
test.expectStr(d.draw(), "cat-shape")
let s: Speaks = c
test.expectStr(s.speak(), "meow")
let a: Animal = c
test.expectStr(a.kind(), "animal")

// Function parameters typed by either interface accept Cat.
fn render(x: Drawable): string { x.draw() }
fn say(x: Speaks): string { x.speak() }
test.expectStr(render(c), "cat-shape")
test.expectStr(say(c), "meow")
