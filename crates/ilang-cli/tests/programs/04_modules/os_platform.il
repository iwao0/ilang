// `os.platform` is a top-level `pub let` populated once at
// module init from a host helper. Reads as a property —
// `"macos"` / `"linux"` / `"windows"`, or `"other"` for any
// target Rust recognises but we don't have a name for. Used
// to branch behaviour on the host without writing a parallel
// `@cfg`-style attribute.
use os
use test

let p = os.platform
test.expectTrue(p == "macos" || p == "linux" || p == "windows" || p == "other")

// The value is stable across reads (it's a single binding).
test.expectStr(os.platform, p)

// String-equality dispatch matches `if`/`elif` chains.
let kind = if os.platform == "macos" { "darwin-family" }
           elif os.platform == "linux" { "unix" }
           elif os.platform == "windows" { "nt" }
           else { "unknown" }
test.expectTrue(kind != "")
