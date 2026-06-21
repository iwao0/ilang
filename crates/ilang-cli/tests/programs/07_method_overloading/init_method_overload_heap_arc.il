// `init` and method overload resolution must pass the chosen overload
// its heap arguments with correct ARC. The existing method-overloading
// fixtures check which overload is picked, not the ARC of heap values
// flowing into the picked init / method. Churn with an exact deinit
// count catches a leak or over-release in the overload-dispatch path.
//
// expect: 15
// expect: churn deinits=600

let deinits: i64[] = [0]
class Box {
    n: i64
    init(x: i64) { this.n = x }
    deinit() { deinits[0] = deinits[0] + 1 }
}
class Sink {
    total: i64
    init(b: Box) { this.total = b.n }
    init(b: Box, c: Box) { this.total = b.n + c.n }
    add(b: Box): i64 { b.n }
    add(b: Box, c: Box): i64 { b.n + c.n }
}

fn run(): i64 {
    let s = new Sink(new Box(10))               // init(Box)
    let s2 = new Sink(new Box(1), new Box(2))   // init(Box, Box) → total 3
    // add(Box) → 3, add(Box, Box) → 9, s2.total → 3
    s.add(new Box(3)) + s.add(new Box(4), new Box(5)) + s2.total
}

console.log(run().toString())                   // 15

// 6 fresh Box per run (1 + 2 into the two inits, 1 + 2 into the two
// add calls), all dropped — deinit exactly 600 over 100 runs.
let d0 = deinits[0]
let i = 0
while i < 100 { let _ = run(); i = i + 1 }
console.log("churn deinits=" + (deinits[0] - d0).toString())
