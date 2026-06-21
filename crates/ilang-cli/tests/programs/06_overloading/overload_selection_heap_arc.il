// Overload resolution must hand the chosen overload its heap argument
// (and heap return value) with correct ARC — the existing overloading
// fixtures only check that the right overload is *picked*, not that the
// heap value flowing into / out of it is retained and released exactly
// once. Covers: overload by param type with a heap arg, an overloaded
// fn returning heap, overload by subclass, and the exact-`T` vs `T?`
// auto-wrap preference — all churned with an exact deinit count.
//
// expect: 73
// expect: 8
// expect: D:dog
// expect: plain:9
// expect: churn deinits=300

let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}
class Animal { init() {} deinit() { deinits[0] = deinits[0] + 1 } kind(): string { "animal" } }
class Dog: Animal { init() {} override kind(): string { "dog" } }

// overload by parameter type, one taking heap
fn f(b: Box): i64 { b.n }
fn f(n: i64): i64 { n * 10 }

// overloaded fn returning heap, distinct overloads
fn make(n: i64): Box { new Box(n) }
fn make(s: string): Box { new Box(s.length) }

// overload by subclass — a `Dog` value picks the `Dog` overload
fn label(a: Animal): string { "A:" + a.kind() }
fn label(d: Dog): string { "D:" + d.kind() }

// exact `T` preferred over the `T?` auto-wrap overload
fn g(b: Box): string { "plain:" + b.n.toString() }
fn g(b: Box?): string { "opt" }

console.log((f(new Box(3)) + f(7)).toString())       // 3 + 70
console.log((make(5).n + make("abc").n).toString())  // 5 + 3
console.log(label(new Dog()))                        // D:dog
console.log(g(new Box(9)))                           // plain:9

// Churn the heap-taking / heap-returning overloads — deinit exactly
// 3 per iteration (one Box into f, one Box out of make, one Dog into
// label), no leak / over-release.
let d0 = deinits[0]
let i = 0
while i < 100 {
    let _a = f(new Box(1))
    let _b = make(2)
    let _c = label(new Dog())
    i = i + 1
}
console.log("churn deinits=" + (deinits[0] - d0).toString())
