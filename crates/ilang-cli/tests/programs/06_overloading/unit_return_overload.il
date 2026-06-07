// Overload on parameter types where every variant returns `()`
// (no return-type annotation). The dispatcher still walks the
// arg signatures, the matching body executes for its side
// effects. Per syntax.md §6.
use std.test as test

let log: string[] = []

fn rec(s: string) { log.push("s:" + s) }
fn rec(n: i64) { log.push("n:" + n.toString()) }
fn rec(b: bool) { log.push("b:" + b.toString()) }

rec("hi")
rec(7)
rec(true)
rec(-1)
rec(false)

test.expect(log.length, 5)
test.expectStr(log[0], "s:hi")
test.expectStr(log[1], "n:7")
test.expectStr(log[2], "b:true")
test.expectStr(log[3], "n:-1")
test.expectStr(log[4], "b:false")
