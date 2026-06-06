// Class-level computed properties: `static get` / `static set`.
// Read via `ClassName.name`, write via `ClassName.name = v`.
// Per syntax.md §7.
use std.test as test

class Palette {
    static raw: i64 = 0xFF8800

    static get accent(): i64 { Palette.raw }
    static set accent(v: i64) { Palette.raw = v }

    static get label(): string { "palette" }
}

test.expect(Palette.accent, 0xFF8800)
test.expectStr(Palette.label, "palette")

Palette.accent = 0x00FFAA
test.expect(Palette.accent, 0x00FFAA)
test.expect(Palette.raw, 0x00FFAA)

Palette.accent = 0
test.expect(Palette.accent, 0)
