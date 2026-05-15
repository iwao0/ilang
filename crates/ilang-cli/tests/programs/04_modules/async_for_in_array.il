// `for v in arr { ...await... }` — non-Range iter desugared to
// an index-based while loop accessing `arr.length` and `arr[i]`.
//
// expect: 120

async fn sum_arr(p: Promise<i64>, arr: i64[]): i64 {
    let total = 0
    for v in arr {
        let m = await p
        total = total + v * m
    }
    total
}

let _ = sum_arr(Promise.resolve(2), [10, 20, 30]).then(fn(n: i64) {
    console.log(n.toString())
})
let _ = 0
