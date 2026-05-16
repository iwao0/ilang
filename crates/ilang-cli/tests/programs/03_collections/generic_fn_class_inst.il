// `fn make<T>(v: T): Box<T> { new Box<T>(v) }` — instantiating a
// generic class inside a generic fn body. Previously failed at the
// type-checker with "expected T, got T" because `check_new` only
// consulted the enclosing class's type params, not the enclosing
// fn's. Now `current_type_params` (already containing both) is
// used, and the MIR pipeline runs class/enum monomorphize a
// second time to pick up class instantiations that first surface
// in specialized generic-fn bodies.
//
// expect: 42
// expect: hi

class Box<T> {
    pub v: T
    pub init(v: T) { this.v = v }
}

fn make<T>(v: T): Box<T> {
    new Box<T>(v)
}

let a: Box<i64> = make(42)
let b: Box<string> = make("hi")
console.log(a.v.toString())
console.log(b.v)
let _ = 0
