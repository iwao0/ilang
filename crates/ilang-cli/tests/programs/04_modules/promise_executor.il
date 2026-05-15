// `new Promise<T>(executor)` — executor body runs on the pool,
// uses the resolve/reject callbacks to settle. .catch then
// recovers a typed value.
//
// expect: recovered: oops

let _p = new Promise<string>(fn(_resolve: fn(string), reject: fn(string)) {
    reject("oops")
}).catch(fn(msg: string): string {
    let s = "recovered: " + msg
    console.log(s)
    s
})

let _ = 0
