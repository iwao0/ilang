// Promise.race([p1, p2]) — settles with whichever input promise
// settles first. With both inputs already-resolved (eager
// settle), the runtime fires both .then callbacks on the pool;
// the aggregate accepts only the first to arrive thanks to
// `settle_resolve`'s "no-op on already-settled" guard. Output
// order is therefore non-deterministic across the race itself,
// but the .then on the aggregate always sees one of "a" or "b".
//
// expect: got one
// expect-skip: race-prone

let arr: Promise<string>[] = [
    Promise.resolve("a"),
    Promise.resolve("b"),
]

let _p = Promise.race(arr).then(fn(_v: string) {
    console.log("got one")
})

let _ = 0
