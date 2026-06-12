// Map value covariance is LITERAL-ONLY. A fresh `{...}` literal can
// land in a `Map<K, Parent>` slot (see map_literal_covariant_value.il),
// but an aliased `Map<K, Child>` variable must NOT — writing a sibling
// `Child2` through the parent face would corrupt the underlying
// `Map<K, Child>`. The checker keeps named maps invariant in V.
//
// expect-error: expected Map<string, Animal>, got Map<string, Dog>

class Animal {
    init() {}
    v(): i64 { 1 }
}
class Dog: Animal {
    init() { super() }
    override v(): i64 { 9 }
}

let md = new Map<string, Dog>()
md["a"] = new Dog()
let ma: Map<string, Animal> = md
console.log(ma["a"].v().toString())
