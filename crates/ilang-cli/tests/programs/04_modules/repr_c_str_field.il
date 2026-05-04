// `string` field on a `@repr(C) struct`. Storage is a heap pointer
// to ilang's `StringRc` (8 bytes), pre-initialised to an interned
// empty string at construction so reads on a fresh instance are
// safe. ARC works as on any other heap field — struct drop releases
// the StringRc.
//
// Note: the field's raw bytes are a `StringRc *`, **not** a
// NUL-terminated `char *`, so a C function reading this struct
// directly will not see expected `char *` layout. For C ABI you
// pass strings as separate `string` arguments (existing
// marshalling) or use `u8[N]` for a fixed inline buffer.
use test

@extern(C) {
    @repr(C) struct person {
        age: i32
        name: string
        nick: string
    }
}

let p = new person()
// Defaults: numeric zero, strings empty.
test.expect(p.age as i64, 0)
test.expectStr(p.name, "")
test.expectStr(p.nick, "")

// Assign and read back.
p.age = 42 as i32
p.name = "alice"
p.nick = "ali"
test.expect(p.age as i64, 42)
test.expectStr(p.name, "alice")
test.expectStr(p.nick, "ali")

// Reassignment frees the previous heap allocation (no leak).
p.name = "alice2"
test.expectStr(p.name, "alice2")

// Concat through a field read; assigning back overwrites cleanly.
p.name = p.name + "!"
test.expectStr(p.name, "alice2!")

// Multiple instances independent.
let q = new person()
q.name = "bob"
test.expectStr(p.name, "alice2!")
test.expectStr(q.name, "bob")
