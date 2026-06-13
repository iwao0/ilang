// Confirms the working repr-enum (`enum E: T`) idioms that lacked a
// fixture, probed in the bit-ops rounds (第68/69弾 fixed the broken
// neighbours: `~`, flags `has`, and `enum == int-literal`):
//
//   * a repr enum as a Map key and a Set element (keyed by its int repr),
//   * repr-enum arithmetic and literal comparison,
//   * the Win32-style dispatch bridge — `match (msg as Msg) { variant ...
//     }` — letting a raw `u32` be matched against the enum's variants
//     after a cast (a direct `match u32 { Msg.quit ... }` is intentionally
//     rejected; cast first).

// expect: repr enum patterns ok

use std.test as test

enum Msg: u32 {
    quit = 16
    paint = 15
    close = 18
}

// Win32-style dispatch: a raw u32 cast into the enum, matched by variant.
fn handle(raw: u32): i64 {
    match raw as Msg {
        quit { 100 }
        paint { 50 }
        close { 200 }
        _ { 0 }
    }
}

test.expect(handle(16), 100)
test.expect(handle(18), 200)
test.expect(handle(99), 0)               // unknown code -> wildcard

// repr enum as a Map key (keyed by the int repr).
let m: Map<Msg, string> = {}
m[Msg.quit] = "q"
m[Msg.close] = "c"
let mv = match m.get(Msg.quit) {
    some(s) { s.length }
    none { 0 - 1 }
}
test.expect(mv, 1)
test.expect(m.size(), 2)

// repr enum as a Set element.
let s = new Set<Msg>()
s.add(Msg.quit)
s.add(Msg.paint)
test.expectTrue(s.has(Msg.quit))
test.expectTrue(!s.has(Msg.close))
test.expect(s.size(), 2)

// arithmetic + literal comparison on the repr.
test.expect((Msg.paint as u32) as i64 + 1, 16)
test.expectTrue(Msg.quit == 16)

console.log("repr enum patterns ok")
