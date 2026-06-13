// A generic enum ctor LITERAL is covariant in its type args, the same way
// an array / map literal is: `Result.ok(new Dog())` (`Result<Dog, _>`)
// flows into a `Result<Animal, string>` slot, storing the Dog upcast to
// Animal. Both type-arg positions covary — the ok payload (T) and the err
// payload (E). The ctor is recorded as the declared enum
// (`Result<Animal, string>`), so the monomorphizer builds the parent
// instantiation and virtual dispatch through the parent works.
//
// Covariance is literal-only: an aliased `Result<Dog, string>` *variable*
// still does not assign to `Result<Animal, string>` (see the companion
// `generic_enum_literal_covariant_alias_error.il`). Enums are immutable
// values, so an upcast payload can never be written back through the parent
// type — the rule stays sound.

use std.test as test

let deinits: i64[] = [0]

class Animal {
    init() {}
    deinit() { deinits[0] = deinits[0] + 1 }
    val(): i64 { 1 }
}

class Dog: Animal {
    init() { super() }
    deinit() { super() }
    override val(): i64 { 10 }
}

// err-position covariance: a Dog in the E slot of `Result<i64, Animal>`.
let er: Result<i64, Animal> = Result.err(new Dog())
let ev = match er {
    ok(v) { v }
    err(a) { a.val() }                       // virtual dispatch -> Dog.val()
}
test.expect(ev, 10)

// ok-position covariance across let / array / map, with heap payloads.
// 3 Dogs/round (let, array[0], map value); the array err and absent
// entries carry none.
fn churn(): i64 {
    let r: Result<Animal, string> = Result.ok(new Dog())
    let a = match r {
        ok(animal) { animal.val() }
        err(s) { 0 - 1 }
    }
    let rs: Result<Animal, string>[] = [Result.ok(new Dog()), Result.err("e")]
    let b = match rs[0] {
        ok(animal) { animal.val() }
        err(s) { 0 - 2 }
    }
    let m: Map<string, Result<Animal, string>> = {"k": Result.ok(new Dog())}
    let c = match m.get("k") {
        some(res) {
            match res {
                ok(animal) { animal.val() }
                err(s) { 0 - 3 }
            }
        }
        none { 0 - 9 }
    }
    a + b + c                                // 10 + 10 + 10 = 30
}

test.expect(churn(), 30)

let acc: i64[] = [0]
let bd = deinits[0]
let d0 = test.liveAllocBytes()
let i = 0
while i < 100 { acc[0] = acc[0] + churn(); i = i + 1 }
test.expect(deinits[0] - bd, 300)            // 3 Dog/round x 100
test.expect(test.liveAllocBytes() - d0, 0)
