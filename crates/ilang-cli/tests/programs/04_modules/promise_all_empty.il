// `Promise.all([])` resolves immediately with an empty array,
// per the empty-input fast path. Confirms the `n == 0` branch
// settles agg without going through the pre-scan / pool path.
//
// expect: empty-resolve
// expect: len=0

let arr: Promise<string>[] = []
let _p = Promise.all(arr).then(fn(vs: string[]): string[] {
    console.log("empty-resolve")
    console.log("len=" + vs.length.toString())
    vs
})

let _ = 0
