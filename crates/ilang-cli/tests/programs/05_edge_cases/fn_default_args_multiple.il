// Functions with multiple default-valued parameters. `default_args.il`
// in 01_basics covers a single default param; this fixture pins down
// behavior with three defaults — partial-supply, full-supply, and
// the all-defaulted case — across primitive + heap arg types.
use std.test as test
fn config(host: string = "localhost", port: i64 = 8080, secure: bool = false): string {
    let scheme = if secure { "https" } else { "http" }
    let portStr = if port == 80 || port == 443 { "" } else { ":x" }
    scheme + "://" + host + portStr
}

test.expectStr(config(), "http://localhost:x")
test.expectStr(config("api.example.com"), "http://api.example.com:x")
test.expectStr(config("api.example.com", 443), "http://api.example.com")
test.expectStr(config("api.example.com", 443, true), "https://api.example.com")

// Default value that depends on other defaults — well-formed only
// if all preceding args are also supplied. (Intentionally only
// exercises positional supply since the docs don't promise named
// args.)
fn frame(width: i64 = 10, height: i64 = 5): i64 { width * height }
test.expect(frame(), 50)
test.expect(frame(20), 100)
test.expect(frame(20, 10), 200)

// Default + heap-typed required param.
fn join(items: string[], sep: string = ","): string {
    let acc = ""
    let i = 0
    while i < items.length {
        if i > 0 { acc = acc + sep }
        acc = acc + items[i]
        i += 1
    }
    acc
}
test.expectStr(join(["a", "b", "c"]), "a,b,c")
test.expectStr(join(["a", "b", "c"], "-"), "a-b-c")
test.expectStr(join([]), "")
test.expectStr(join(["one"]), "one")
