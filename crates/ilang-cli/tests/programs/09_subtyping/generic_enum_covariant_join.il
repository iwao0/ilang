// Generic-enum literal covariance extends through `if` / `match` joins.
// A single covariant arm already worked — `fn f(): Box<Animal> {
// Box.hold(new Dog()) }` — but two arms each building the same generic
// enum at a different subclass joined to the first arm's concrete type
// and rejected the sibling ("expected Box<Dog>, got Box<Cat>"). Now the
// arms join to the common-ancestor instantiation `Box<Animal>`, the way
// a plain-object if-join already takes the classes' common ancestor.
//
// LITERAL-only, like the single-arm rule: only ctor-literal arms covary.
// An aliased `Box<Dog>` variable in an arm keeps the join invariant
// (see generic_enum_literal_covariant_alias_error.il), so this stays
// sound for mutable generics.

// The same join covariance covers every fresh covariant literal — array
// (`[Dog]` ⊔ `[Cat]` = `Animal[]`), map (`{"k": Dog}` ⊔ `{"k": Cat}` =
// `Map<string, Animal>`), tuple (`(Dog, 1)` ⊔ `(Cat, 2)` = `(Animal,
// i64)`), and `some(..)`-wrapped ones (`some(Box<Dog>)` ⊔ `some(Box<Cat>)`
// = `Box<Animal>?`) — still literal-only.

// expect: if dog
// expect: if cat
// expect: match dog
// expect: match cat
// expect: same-subclass dog
// expect: two-param dog
// expect: array dog cat
// expect: opt dog
// expect: map dog cat
// expect: tuple dog cat
// expect: struct dog

use std.test as test

class Animal {
    pub name: string
    init(n: string) { this.name = n }
}
class Dog: Animal {
    init() { super("dog") }
}
class Cat: Animal {
    init() { super("cat") }
}

enum Box<T> {
    hold: (T)
}
// struct-shaped payload — a distinct ctor-literal AST form (`Wrap.one {
// item: ... }`) from the tuple `Box.hold(..)`; the join must recognise it
// the same way.
enum Wrap<T> {
    one: { item: T }
}
enum Either<L, R> {
    left: (L)
    right: (R)
}

fn name(b: Box<Animal>): string {
    match b {
        hold(a) { a.name }
    }
}

// if-join of two different subclasses -> Box<Animal>
fn pickIf(b: bool): Box<Animal> {
    if b { Box.hold(new Dog()) } else { Box.hold(new Cat()) }
}
console.log("if " + name(pickIf(true)))
console.log("if " + name(pickIf(false)))

// match-join, three arms
fn pickMatch(n: i64): Box<Animal> {
    match n {
        0 { Box.hold(new Dog()) }
        1 { Box.hold(new Cat()) }
        _ { Box.hold(new Dog()) }
    }
}
console.log("match " + name(pickMatch(0)))
console.log("match " + name(pickMatch(1)))

// both arms the SAME subclass — the join is `Box<Dog>`, widened to the
// `Box<Animal>` return because the arms are ctor literals.
fn pickSame(b: bool): Box<Animal> {
    if b { Box.hold(new Dog()) } else { Box.hold(new Dog()) }
}
console.log("same-subclass " + name(pickSame(true)))

// multi-param generic: only the covaried arg joins, the other matches.
fn pickEither(b: bool): Either<Animal, i64> {
    if b { Either.left(new Dog()) } else { Either.left(new Cat()) }
}
let lbl = match pickEither(true) {
    left(a) { a.name }
    right(_) { "?" }
}
console.log("two-param " + lbl)

// fresh array literals join to the common-ancestor element type.
fn pickArr(b: bool): Animal[] {
    if b { [new Dog()] } else { [new Cat()] }
}
console.log("array " + pickArr(true)[0].name + " " + pickArr(false)[0].name)

// `some(..)`-wrapped generic-enum literals join through the Optional.
fn pickOpt(b: bool): Box<Animal>? {
    if b { some(Box.hold(new Dog())) } else { some(Box.hold(new Cat())) }
}
let on = match pickOpt(true) {
    some(bx) { name(bx) }
    none { "none" }
}
console.log("opt " + on)

// fresh map literals join on their value type.
fn pickMap(b: bool): Map<string, Animal> {
    if b { {"k": new Dog()} } else { {"k": new Cat()} }
}
console.log("map " + pickMap(true)["k"].name + " " + pickMap(false)["k"].name)

// fresh tuple literals covary element-wise.
fn pickTup(b: bool): (Animal, i64) {
    if b { (new Dog(), 1) } else { (new Cat(), 2) }
}
let ta = pickTup(true)
let tb = pickTup(false)
console.log("tuple " + ta[0].name + " " + tb[0].name)

// struct-payload ctor literals join the same way as tuple-payload ones.
fn pickWrap(b: bool): Wrap<Animal> {
    if b { Wrap.one { item: new Dog() } } else { Wrap.one { item: new Cat() } }
}
let wn = match pickWrap(true) {
    one { item } { item.name }
}
console.log("struct " + wn)
