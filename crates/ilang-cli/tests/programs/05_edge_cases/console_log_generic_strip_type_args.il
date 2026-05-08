// `console.log(<generic instance>)` strips the `<...>`
// monomorphisation suffix to match the interpreter's `Display`.
//
// Pre-fix the JIT carried monomorphised names like `Box<A>` and
// `Result<i64, string>` directly into the printed output, while
// the interpreter's `ObjectData.class` / `Value::Enum.ty` keep
// just the base name (`Box` / `Result`).
//
// expect: Box { value: 5 }
// expect: Box { value: hi }
// expect: Result::ok(42)
// expect: Result::err(nope)

class Box<T> {
    value: T
    init(v: T) { this.value = v }
}

console.log(new Box<i64>(5))
console.log(new Box<string>("hi"))

let r1: Result<i64, string> = Result.ok(42)
let r2: Result<i64, string> = Result.err("nope")
console.log(r1)
console.log(r2)
