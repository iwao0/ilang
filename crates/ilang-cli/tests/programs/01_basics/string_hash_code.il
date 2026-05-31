// `"foo".hashCode(): i64` — FNV-1a 64-bit over the bytes. Stable
// across runs so `@derive(Hash)` on a string-bearing class can
// rely on the value for bucketing.
use std.test as test

// Self-consistency: the same string always hashes to the same i64.
test.expectBool("hello".hashCode() == "hello".hashCode(), true)

// Distinct content → distinct hashes (no collision on these short
// strings under FNV-1a).
test.expectBool("a".hashCode() != "b".hashCode(), true)
test.expectBool("abc".hashCode() != "acb".hashCode(), true)
test.expectBool("hello".hashCode() != "world".hashCode(), true)

// Empty string lands on the FNV-1a 64-bit offset basis.
test.expect("".hashCode(), 0xcbf29ce484222325 as i64)
