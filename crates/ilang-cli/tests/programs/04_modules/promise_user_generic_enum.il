// `Promise<Box<T>>` (ユーザ定義 generic enum を Promise の payload に
// 入れる) — 以前は MIR の monomorphize_enums が FnExpr クロージャの
// 引数型を rewrite し損ねていたため `unsupported in M1: user-defined
// generic types` で失敗していた。本 fixture は修正後の動作確認。
//
// Promise<Result<T, E>> は別の pre-existing 問題 (built-in Result と
// synthesized Result<...> の MirTy::Enum id 重複) があり、ここでは対象外。
//
// expect: hold: hi
// expect: empty

enum Box<T> {
    hold: (T)
    empty
}

fn wrap(s: string, fill: bool): Promise<Box<string>> {
    if fill {
        let b: Box<string> = Box.hold(s)
        Promise.resolve(b)
    } else {
        let b: Box<string> = Box.empty
        Promise.resolve(b)
    }
}

async fn run(): i64 {
    let a: Box<string> = await wrap("hi", true)
    match a {
        hold(s) { console.log("hold: " + s) }
        empty { console.log("empty") }
    }
    let b: Box<string> = await wrap("ignored", false)
    match b {
        hold(s) { console.log("hold: " + s) }
        empty { console.log("empty") }
    }
    0
}

let _ = run().then(fn(_n: i64) {})
let _ = 0
