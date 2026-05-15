// Promise.all([p1, p2, p3]) → resolves with array of all values
// once every input promise has resolved. The `.then` fires on the
// drain pass at main exit.
//
// expect: 3
// expect: a, b, c

let arr: Promise<string>[] = [
    Promise.resolve("a"),
    Promise.resolve("b"),
    Promise.resolve("c"),
]

let _p = Promise.all(arr).then(fn(vs: string[]): string[] {
    console.log(vs.length.toString())
    console.log(vs[0] + ", " + vs[1] + ", " + vs[2])
    vs
})

let _ = 0
