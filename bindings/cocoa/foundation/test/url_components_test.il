// NSURLComponents — parse, mutate, rebuild.

use test
use foundation { NSURLComponents, nsstr, ilstr }

// Parse a full URL.
let c = NSURLComponents.componentsWithString(
    nsstr("https://user:pw@example.com:8080/x?a=1#frag")
)
test.expectStr(ilstr(c.scheme), "https")
test.expectStr(ilstr(c.host), "example.com")
test.expectStr(ilstr(c.path), "/x")
test.expectStr(ilstr(c.query), "a=1")
test.expectStr(ilstr(c.fragment), "frag")
test.expectStr(ilstr(c.user), "user")
test.expectStr(ilstr(c.password), "pw")

// Build from scratch.
let b = NSURLComponents.alloc().init()
b.scheme = nsstr("https")
b.host = nsstr("api.example.com")
b.path = nsstr("/v1/data")
b.query = nsstr("id=42&kind=full")
test.expectStr(
    ilstr(b.string),
    "https://api.example.com/v1/data?id=42&kind=full"
)

// Mutate an existing one.
c.host = nsstr("api.example.com")
c.path = nsstr("/replaced")
c.query = nsstr("k=v")
c.fragment = nsstr("")
test.expectStr(ilstr(c.host), "api.example.com")
test.expectStr(ilstr(c.path), "/replaced")
test.expectStr(ilstr(c.query), "k=v")

// `.url` re-materialises an NSURL from current state.
let resolved = c.url
test.expectStr(ilstr(resolved.host), "api.example.com")

0
