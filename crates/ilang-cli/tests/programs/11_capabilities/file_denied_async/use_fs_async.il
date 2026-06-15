// The std.fs sink sits past an `await`, so it lands in the second
// resume-point of the desugared poll fn. The capability gate must still
// fire there: JIT aborts when the poll runs, AOT fails the build.
//
// expect-error: capability 'file'
use std.fs as fs

async fn check(): bool {
    let p = Promise.resolve(0)
    let _ = await p
    fs.exists("/tmp")
}

check().then(fn(v: bool) {
    console.log(v.toString())
})
