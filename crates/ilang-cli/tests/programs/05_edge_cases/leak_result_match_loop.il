// `Result<T, E>` is the canonical built-in payload-bearing enum
// — both `ok(v)` and `err(e)` allocate a 2-slot enum cell that
// has to drop after the consumer reads its inner. The string-
// payload `err` arm also has to release its StringRc when the
// cell drops. `leak_enum_payload_loop.il` covers a user-defined
// enum; this fixture exercises the stdlib Result type and its
// per-arm cascade.
//

use test

fn pick(n: i64): Result<i64, string> {
    if n % 2 == 0 { Result.ok(n) } else { Result.err("err") }
}

let baseline = test.liveAllocBytes()
let baseline_s = test.liveStringCount()

let i: i64 = 0
while i < 300 {
    let r = pick(i)
    let v = match r {
        ok(n) { n }
        err(_) { -1 }
    }
    let _ = v
    i = i + 1
}

let after = test.liveAllocBytes()
let after_s = test.liveStringCount()
test.expectTrue(after - baseline < 65536)
test.expectTrue(after_s - baseline_s < 50)
