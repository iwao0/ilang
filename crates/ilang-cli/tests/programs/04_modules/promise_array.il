// `Promise<i64[]>` / `Promise<string[]>` — await yields the array
// and downstream code reads `.length` / `[i]`.
//
// expect: 60
// expect: hello, world

fn makeNums(): Promise<i64[]> {
    let xs: i64[] = [10, 20, 30]
    Promise.resolve(xs)
}

fn makeStrs(): Promise<string[]> {
    let ss: string[] = ["hello, ", "world"]
    Promise.resolve(ss)
}

async fn run(): i64 {
    let arr: i64[] = await makeNums()
    let total = 0
    let i = 0
    while i < arr.length {
        total = total + arr[i]
        i = i + 1
    }
    console.log(total.toString())

    let strs: string[] = await makeStrs()
    console.log(strs[0] + strs[1])
    0
}

let _ = run().then(fn(_n: i64) {})
let _ = 0
