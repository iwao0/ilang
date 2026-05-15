// Generic user enum with type-param payload, awaited inside arm
// with binding. v2 resolves the variant payload type by
// substituting the enum's type params with the scrutinee's
// concrete args (so `s: string` here, not the I64 placeholder).
//
// expect: hello, world

enum Box<T> {
    hold: (T)
    drop
}

async fn pick(b: Box<string>, p: Promise<string>): string {
    match b {
        hold(s) {
            let n = await p
            s + n
        }
        drop {
            let f = await p
            "drop: " + f
        }
    }
}

let _ = pick(Box.hold("hello, "), Promise.resolve("world")).then(fn(s: string) {
    console.log(s)
})
let _ = 0
