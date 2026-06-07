// `Promise.all` rejects with the first rejection — subsequent
// resolutions / rejections don't change the aggregate's state.
// Per syntax.md §13 "rejects on first rejection".
//
// expect: caught: boom

fn mk_reject(msg: string): Promise<string> {
    new Promise<string>(fn(resolve: fn(string), reject: fn(string)) {
        reject(msg)
    })
}

let arr: Promise<string>[] = [
    Promise.resolve("ok1"),
    mk_reject("boom"),
    Promise.resolve("ok2"),
]

let _p = Promise.all(arr)
    .then(fn(vs: string[]): string[] {
        // Shouldn't run — one of the inputs rejected.
        console.log("unexpected resolve")
        vs
    })
    .catch(fn(e: string): string[] {
        console.log("caught: " + e)
        []
    })

let _ = 0
