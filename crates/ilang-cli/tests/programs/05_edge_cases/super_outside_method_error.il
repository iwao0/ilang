// `super` is bound to the surrounding class context. A top-level
// function (or any fn outside a class method body) has no class
// to lift `super` against — the compiler must reject it rather
// than letting it resolve to something arbitrary.
//
// expect-error: `super` is only valid inside a class method

class Base {
    x(): i64 { 1 }
}
class Derived: Base {
    override x(): i64 { super.x() + 1 }
}

fn loose(): i64 {
    super.x()
}

loose()
