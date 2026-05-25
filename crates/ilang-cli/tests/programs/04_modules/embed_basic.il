// `@embed("path") const X: T` — embed a file at compile time.
// The path is resolved relative to **this** source file. `T` must
// be either `string` (UTF-8 text; invalid UTF-8 = compile error)
// or `u8[]` (raw bytes).
use std.test as test
@embed("embed_data.txt") const greeting: string
@embed("embed_data.txt") const raw: u8[]

test.expectStr(greeting, "hello from embed\nsecond line\n")
test.expect(raw.length, 29)
test.expect(raw[0] as i64, 104)        // 'h'
test.expect(raw[28] as i64, 10)        // '\n'
