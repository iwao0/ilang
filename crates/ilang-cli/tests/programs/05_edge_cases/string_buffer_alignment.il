// Strings live as `[ i64 length | UTF-8 bytes | \0 ]` on the heap;
// the user-visible pointer points at the first byte. `host_str_*`
// helpers read the length prefix as `*((p - 8) as *const i64)`,
// which Rust's debug builds bounds-check for 8-byte alignment.
//
// Pre-fix two of the underlying allocators returned byte-aligned
// memory:
//   1. `leak_cstring` used `Vec<u8>::into_boxed_slice()` whose
//      backing has align=1 — every `int.toString()`, every string
//      concat result, and every other freshly-built string would
//      land at a random byte offset and trip the misaligned-deref
//      check inside `cstr_bytes`.
//   2. The string-literal data segments declared via Cranelift's
//      `DataDescription` defaulted to byte alignment — even bare
//      `let s = "hi" + " world"` panicked.
//
// This fixture exercises every shape that surfaced the crash:
// concat in expression position, captured-string mutation, getter
// returning a concat result, fresh string into a `Map<string,
// string>`, and `let`-shadow rebinding a fresh string.
use test

// 1. Bare concat.
let s = "hi" + " world"
test.expect(s.length, 8)

// 2. Captured-string concat inside a closure body.
fn make(): fn(): string {
    let acc = "hello"
    fn(): string { acc + "x" }
}
let f = make()
test.expectStr(f(), "hellox")

// 3. Property getter returning a concat result.
class Person {
    fname: string
    lname: string
    init(a: string, b: string) { this.fname = a; this.lname = b }
    get fullName(): string { this.fname + " " + this.lname }
}
let p = new Person("Ada", "Lovelace")
test.expect(p.fullName.length, 12)
test.expectStr(p.fullName, "Ada Lovelace")

// 4. Fresh string into `Map<string, string>`.
let m = new Map<string, string>()
let v = "p" + "q"
m["a"] = v
test.expectStr(m["a"], "pq")
m["b"] = "r" + "s"
test.expectStr(m["b"], "rs")

// 5. `let`-shadow rebinding a fresh string.
let i: i64 = 0
while i < 3 {
    let s = "first-" + i.toString()
    let s = "second-" + i.toString()
    let _ = s.length
    i = i + 1
}

// 6. `int.toString()` results — these flow through the same
// `leak_cstring` path that triggered the crash for many users.
let n = 42
let str = n.toString() + "!"
test.expectStr(str, "42!")
