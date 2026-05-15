// Pattern binding 型精密化: `T?` の `some(s)` で s が string
// (heap 型) のケース。v2 の MatchT terminator が variant payload
// に s を string として宣言し、retain/release が正しく動く。
// (legacy / 旧 v2 では I64 placeholder のため、heap 値の管理が
// 暗黙でずれていた。)
//
// expect: hello, world!
// expect: greeting: oops

async fn pick(o: string?, p: Promise<string>): string {
    match o {
        some(s) {
            let n = await p
            "hello, " + n + s
        }
        none {
            let f = await p
            "greeting: " + f
        }
    }
}

async fn run(): i64 {
    let a: string = await pick(some("!"), Promise.resolve("world"))
    let b: string = await pick(none,      Promise.resolve("oops"))
    console.log(a)
    console.log(b)
    0
}

let _ = run().then(fn(_n: i64) {})
let _ = 0
