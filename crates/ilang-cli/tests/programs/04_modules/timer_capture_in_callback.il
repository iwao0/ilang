// A `setTimeout` callback captures a heap value (string) from
// the outer scope. The capture's lifetime must outlive the
// timer's pool task — the runtime retains the closure cell on
// submit and releases it after the callback fires, so the cell's
// captured string lives until the firing path completes.
//
// expect: hello, ada

use std.time as time

let who = "ada"
let _id = time.setTimeout(fn() {
    console.log("hello, " + who)
}, 0)

let _ = 0
