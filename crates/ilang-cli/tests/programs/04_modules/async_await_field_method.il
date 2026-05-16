// The mini-inferencer in the async desugar now handles
// `obj.field` and `obj.method(...)` on the RHS of `let v = await
// ...`, so explicit type annotations are no longer required for
// these common shapes.
//
// expect: 42
// expect: 100

class Source {
    pub p: Promise<i64>
    pub init(p: Promise<i64>) { this.p = p }
    pub fetch(): Promise<i64> {
        Promise.resolve(100)
    }
}

async fn run_field(s: Source): i64 {
    // No `let v: i64 = ...` annotation — inferred from `s.p`
    // which has type `Promise<i64>`, stripped to `i64` by await.
    let v = await s.p
    v
}

async fn run_method(s: Source): i64 {
    // Method return type comes from the receiver's class decl.
    let v = await s.fetch()
    v
}

async fn run(): i64 {
    let a = await run_field(new Source(Promise.resolve(42)))
    let b = await run_method(new Source(Promise.resolve(0)))
    console.log(a.toString())
    console.log(b.toString())
    0
}

let _ = run().then(fn(_n: i64) {})
let _ = 0
