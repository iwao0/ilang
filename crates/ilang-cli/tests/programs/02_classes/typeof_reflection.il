// `typeof(x).<member>` reflection on a class instance. Per
// syntax.md §13a.
use std.test as test

class Animal {
    name: string
    init(n: string) { this.name = n }
    greet(): string { "hi " + this.name }
}

class Dog: Animal {
    breed: string
    init(n: string, b: string) {
        super.init(n)
        this.breed = b
    }
    bark(): string { "woof" }
}

let t = typeof(new Dog("Rex", "lab"))

// `.name` works through let-bound handles.
test.expectStr(t.name, "Dog")

// `.kind` — TypeKind enum. A class instance reports `class`.
let kind_label = match t.kind {
    class { "class" }
    primitive { "primitive" }
    enum { "enum" }
    _ { "other" }
}
test.expectStr(kind_label, "class")

// `.fields` — only declared on this class (no inherited `name`).
test.expect(t.fields.length, 1)
test.expectStr(t.fields[0], "breed")

// `.methods` — declared methods including `init`. Order isn't
// part of the contract, so scan.
let m = t.methods
let has_init = false
let has_bark = false
for name in m {
    if name == "init" { has_init = true }
    if name == "bark" { has_bark = true }
}
test.expectBool(has_init, true)
test.expectBool(has_bark, true)

// `.parent` — `some(Animal)` for Dog, `none` for Animal.
if let some(p) = t.parent {
    test.expectStr(p.name, "Animal")
} else {
    test.fail("Dog.parent should be Animal")
}
let ta = typeof(new Animal("x"))
if let some(_p) = ta.parent {
    test.fail("Animal.parent should be none")
}

// `.typeArgs` — empty for non-generic types.
test.expect(t.typeArgs.length, 0)
test.expect(ta.typeArgs.length, 0)

// Generic instances surface their concrete type arguments in order.
class Box<T> { x: T; init(v: T) { this.x = v } }
class Pair<A, B> {
    a: A
    b: B
    init(x: A, y: B) { this.a = x; this.b = y }
}
let tb = typeof(new Box<i64>(0))
test.expectStr(tb.name, "Box")
test.expect(tb.typeArgs.length, 1)
test.expectStr(tb.typeArgs[0].name, "i64")
let tp = typeof(new Pair<string, i64>("k", 1))
test.expectStr(tp.name, "Pair")
test.expect(tp.typeArgs.length, 2)
test.expectStr(tp.typeArgs[0].name, "string")
test.expectStr(tp.typeArgs[1].name, "i64")
// Nested generic — outer arg is "Box<i64>" which resolves to a real
// monomorph class id, so `.name` (which strips the type-arg suffix)
// reports the base class name.
let tn = typeof(new Box<Box<i64>>(new Box<i64>(0)))
test.expect(tn.typeArgs.length, 1)
test.expectStr(tn.typeArgs[0].name, "Box")

// `.fieldType(name)` — declared types of named fields.
if let some(ft) = t.fieldType("breed") {
    test.expectStr(ft.name, "string")
} else {
    test.fail("fieldType(\"breed\") should be some")
}
// Inherited field `name` is on Animal, not Dog — fieldType returns
// none here (callers chase `.parent` for those).
if let some(_) = t.fieldType("name") {
    test.fail("fieldType(\"name\") should be none on Dog (chase parent)")
}
if let some(_) = t.fieldType("nope") {
    test.fail("fieldType(\"nope\") should be none")
}

// `.methodReturn(name)` — return type of `bark` is string.
if let some(rt) = t.methodReturn("bark") {
    test.expectStr(rt.name, "string")
} else {
    test.fail("methodReturn(\"bark\") should be some")
}
if let some(_) = t.methodReturn("nope") {
    test.fail("methodReturn(\"nope\") should be none")
}

// `.methodParams(name)` — bark takes zero user args, init takes
// (string, string) on Dog.
if let some(ps) = t.methodParams("bark") {
    test.expect(ps.length, 0)
} else {
    test.fail("methodParams(\"bark\") should be some")
}
if let some(ps) = t.methodParams("init") {
    test.expect(ps.length, 2)
    test.expectStr(ps[0].name, "string")
    test.expectStr(ps[1].name, "string")
} else {
    test.fail("methodParams(\"init\") should be some")
}
if let some(_) = t.methodParams("nope") {
    test.fail("methodParams(\"nope\") should be none")
}
