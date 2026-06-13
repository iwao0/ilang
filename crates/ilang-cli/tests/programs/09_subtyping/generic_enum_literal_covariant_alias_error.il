// Generic-enum covariance is LITERAL-only. An aliased `Result<Dog, string>`
// variable does NOT assign to `Result<Animal, string>` — only a ctor
// literal (`Result.ok(new Dog())`) covaries at the binding site. This
// keeps the rule sound the same way array/map literal covariance does
// (`map_literal_covariant_value_alias_error.il`).

// expect-error: expected Result<Animal, string>, got Result<Dog, string>

class Animal {
    init() {}
    val(): i64 { 1 }
}

class Dog: Animal {
    init() { super() }
    override val(): i64 { 10 }
}

let d: Result<Dog, string> = Result.ok(new Dog())
let a: Result<Animal, string> = d
console.log("unreachable")
