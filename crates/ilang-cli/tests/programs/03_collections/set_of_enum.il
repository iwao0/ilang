// `Set<MyEnum>` / `Map<MyEnum, V>` work when every variant is
// unit-payload (or the whole enum is `@flags`). Those compile to
// a single `i64` tag, so the existing `Int` store keys them
// naturally. Payload-carrying enums need structural equality and
// are rejected at type-check (see the matching negative test).
use std.test as test

// ---- plain unit-variant enum ----
enum Color {
    red,
    green,
    blue,
}

let s = new Set<Color>()
s.add(Color.red)
s.add(Color.green)
s.add(Color.red)   // duplicate tag — collapses
test.expect(s.size(), 2)
test.expectBool(s.has(Color.red), true)
test.expectBool(s.has(Color.blue), false)
test.expectBool(s.delete(Color.red), true)
test.expectBool(s.delete(Color.red), false)
test.expect(s.size(), 1)

// values() snapshot + forEach visit every remaining element once.
s.add(Color.blue)
let touch_count = 0
s.forEach(fn(c: Color) { touch_count = touch_count + 1 })
test.expect(touch_count, 2)
test.expect(s.values().length, 2)
s.clear()
test.expect(s.size(), 0)

// ---- set algebra over enum elements ----
let a = new Set<Color>()
a.add(Color.red)
a.add(Color.green)
let b = new Set<Color>()
b.add(Color.green)
b.add(Color.blue)
test.expect(a.union(b).size(), 3)
test.expect(a.intersection(b).size(), 1)
test.expect(a.difference(b).size(), 1)
test.expectBool(a.union(b).isSupersetOf(a), true)

// ---- @flags enum ----
@flags
enum Modifier {
    none  = 0,
    shift = 1,
    ctrl  = 2,
    alt   = 4,
}

let mods = new Set<Modifier>()
mods.add(Modifier.shift)
mods.add(Modifier.ctrl)
mods.add(Modifier.shift | Modifier.ctrl)   // a different combined bit pattern
test.expect(mods.size(), 3)
test.expectBool(mods.has(Modifier.shift), true)
test.expectBool(mods.has(Modifier.alt), false)

// ---- Map<MyEnum, V> ----
let label = new Map<Color, string>()
label[Color.red]   = "warning"
label[Color.green] = "ok"
label[Color.blue]  = "info"
test.expect(label.size(), 3)
test.expectStr(label[Color.green], "ok")
test.expectBool(label.delete(Color.green), true)
test.expect(label.size(), 2)
