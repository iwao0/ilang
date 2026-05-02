// User-defined generic enum, instantiated with several type args.
// JIT monomorphizes per (Name, args) pair.
use test

enum Either<L, R> {
    left: (L)
    right: (R)
}

let a: Either<i64, string> = Either.left(42)
let b: Either<i64, string> = Either.right("hi")

fn label(e: Either<i64, string>): string {
    match e {
        left(_) { "L" }
        right(s) { s }
    }
}
test.expectStr(label(a), "L")
test.expectStr(label(b), "hi")

fn extract(e: Either<i64, string>): i64 {
    match e {
        left(n) { n }
        right(_) { -1 }
    }
}
test.expect(extract(a), 42)
test.expect(extract(b), -1)

// Different instantiation — Either<string, i64>.
let c: Either<string, i64> = Either.right(100)
fn rightInt(e: Either<string, i64>): i64 {
    match e {
        left(_) { -1 }
        right(n) { n }
    }
}
test.expect(rightInt(c), 100)
