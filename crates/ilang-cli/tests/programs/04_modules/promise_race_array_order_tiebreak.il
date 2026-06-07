// `Promise.race` with all-synchronously-settled inputs picks the
// first one in array order (deterministic tie-break). Per the
// pre-scan added in 84307028. The pool ordering of the eventual
// stub fires no longer matters — the agg is locked synchronously
// in __promise_race.
//
// expect: got: a

let arr: Promise<string>[] = [
    Promise.resolve("a"),
    Promise.resolve("b"),
    Promise.resolve("c"),
]

let _p = Promise.race(arr).then(fn(v: string) {
    console.log("got: " + v)
})

let _ = 0
