// `for v in start.. { ...await...; break }` — half-open range
// desugars to `while true { ...; v = v + 1 }`. Body must exit
// via break.
//
// expect: 30

async fn sum_until(p: Promise<i64>, stop: i64): i64 {
    let total = 0
    for i in 0.. {
        if i >= stop { break }
        let v = await p
        total = total + v
    }
    total
}

let _ = sum_until(Promise.resolve(10), 3).then(fn(n: i64) {
    console.log(n.toString())
})
let _ = 0
