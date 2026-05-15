// Promise rejection paths:
// - executor reject → .catch recovery (Promise<string>)
// - Promise.reject(msg) static → .catch (Promise<()>)
//
// Each test runs as its own chain so the chain-internal order
// is deterministic, but the two chains race on the pool. To
// keep expected output stable, kick off only one chain.
//
// expect: caught: boom

let _p = Promise.reject("boom").catch(fn(s: string) {
    console.log("caught: " + s)
})

let _ = 0
