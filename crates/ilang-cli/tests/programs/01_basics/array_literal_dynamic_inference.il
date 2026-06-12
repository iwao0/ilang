// An array literal with no annotation infers as a DYNAMIC array
// (user rule: `let xs = [...]` is `T[]`; `T[N]` arises only from a
// declared type). So `push` works on a bare literal binding, and a
// heap-element literal passed to a generic fn binds T to `Box[]`,
// not `Box[2]`.
//
// expect: len=4 last=4
// expect: heap-len=3
// expect: generic-ok n=1

class Box {
    n: i64
    init(x: i64) { this.n = x }
}

let a = [1, 2, 3]
a.push(4)
console.log("len=" + a.length.toString() + " last=" + a[3].toString())

let xs = [new Box(1), new Box(2)]
xs.push(new Box(3))
console.log("heap-len=" + xs.length.toString())

fn hold<T>(v: T): T? { some(v) }
let o = hold([new Box(1), new Box(2)])
match o {
    some(arr) { console.log("generic-ok n=" + arr[0].n.toString()) }
    none { console.log("none") }
}
