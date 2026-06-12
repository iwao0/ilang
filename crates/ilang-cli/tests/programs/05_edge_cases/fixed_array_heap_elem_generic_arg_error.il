// A generic type parameter may not resolve to a fixed-length
// heap-element array — neither by inference (generic fn / method
// calls, enum ctors) nor by explicit `new C<Box[2]>` args. The
// generic body was checked with `T` opaque, so `some(v)` /
// container cells inside it would take buffer ownership the
// caller's binding still holds (double free — reproduced as
// SIGABRT before this check existed).
//
// expect-error: can't be generic type arguments

class Box {
    n: i64
    init(x: i64) { this.n = x }
}

fn hold<T>(v: T): T? {
    some(v)
}

let a: Box[2] = [new Box(1), new Box(2)]
let o = hold(a)
