// String-typed static fields: the slot holds an i64 heap pointer
// that gets filled in at program startup by a synthetic
// `AssignField`. Works for both `const` (write-once) and plain
// `static` (mutable).
// expect: hello
// expect: world
// expect: ilang

class Greeter {
    const greeting: string = "hello"
    static name: string = "world"
}

console.log(Greeter.greeting)
console.log(Greeter.name)
Greeter.name = "ilang"
console.log(Greeter.name)
